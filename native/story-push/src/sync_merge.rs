//! `story sync-merge`: after a push to the default branch, mirror merged cards to their issues
//! and comment once per pull request on each named issue (ST-151; Gitea: ST-157).
//! Design: `docs/designs/merge-sync-design.md`.

use crate::gateway::{CommentOutcome, ErrorKind, Gateway, MergedPull, TrackerError};
use crate::stories_line::{self, Reference};
use crate::{config, lenient_story, Failure};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

const FLAGS: [&str; 6] = ["all", "json", "workflow", "from", "config", "dir"];

/// The tracker calls `sync-merge` makes, so tests and other trackers can stand in.
pub(crate) trait MergeTracker {
    fn last_success(
        &mut self,
        workflow: &str,
        branch: &str,
    ) -> Result<Option<String>, TrackerError>;
    fn pulls_for_commit(&mut self, sha: &str) -> Result<Vec<MergedPull>, TrackerError>;
    fn update(&mut self, story: &Value) -> Result<(), TrackerError>;
    fn comment_once(
        &mut self,
        n: u64,
        marker: &str,
        body: &str,
    ) -> Result<CommentOutcome, TrackerError>;
}
impl MergeTracker for Gateway {
    fn last_success(
        &mut self,
        workflow: &str,
        branch: &str,
    ) -> Result<Option<String>, TrackerError> {
        Gateway::last_success(self, workflow, branch)
    }
    fn pulls_for_commit(&mut self, sha: &str) -> Result<Vec<MergedPull>, TrackerError> {
        Gateway::pulls_for_commit(self, sha)
    }
    fn update(&mut self, story: &Value) -> Result<(), TrackerError> {
        self.update_classified(story).map(|_| ())
    }
    fn comment_once(
        &mut self,
        n: u64,
        marker: &str,
        body: &str,
    ) -> Result<CommentOutcome, TrackerError> {
        Gateway::comment_once(self, n, marker, body)
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, TrackerError> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|_| TrackerError::from("git could not start.".to_string()))?;
    if !out.status.success() {
        return Err(format!("git {} failed.", args.first().unwrap_or(&"")).into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A file's text at a revision, or `None` when it does not exist there.
fn show(root: &Path, rev: &str, path: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["show", &format!("{rev}:{path}")])
        .current_dir(root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_card(text: &str) -> Option<Value> {
    lenient_story::parse_noting_drops(text)
        .ok()
        .map(|(parsed, _)| parsed.to_value())
}

fn remote_of(card: &Value) -> Option<u64> {
    match &card["remote"] {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// The card IDs among `paths` under `dir` (a path relative to the repository root).
fn card_ids(paths: &str, dir: &str, prefix: &str) -> BTreeSet<String> {
    paths
        .lines()
        .filter_map(|p| p.strip_prefix(dir)?.strip_prefix('/'))
        .filter(|name| !name.contains('/'))
        .filter_map(crate::story_file_id)
        .filter(|id| id.starts_with(&format!("{prefix}-")))
        .map(str::to_string)
        .collect()
}

#[derive(Default)]
struct Report {
    from: Option<String>,
    to: String,
    mode: &'static str,
    reason: Option<String>,
    prs: Vec<u64>,
    updated: Vec<String>,
    skipped: Vec<(String, String)>,
    commented: Vec<(u64, Option<u64>, &'static str, String)>,
    existing: Vec<(u64, Option<u64>)>,
    failed: Vec<(String, ErrorKind, String)>,
    notes: Vec<String>,
}
impl Report {
    fn fail(&mut self, target: String, error: TrackerError) {
        self.failed.push((target, error.kind, error.message));
    }
    fn text(&self) -> String {
        let mut out = String::new();
        for id in &self.updated {
            out += &format!("updated {id}\n");
        }
        for (id, reason) in &self.skipped {
            out += &format!("skipped {id}: {reason}\n");
        }
        for (issue, pr, kind, sha) in &self.commented {
            match (kind, pr) {
                (&"merge", Some(pr)) => out += &format!("commented #{issue} (PR #{pr})\n"),
                _ => out += &format!("state #{issue} ({sha})\n"),
            }
        }
        for (issue, pr) in &self.existing {
            match pr {
                Some(pr) => out += &format!("existing #{issue} (PR #{pr})\n"),
                None => out += &format!("existing #{issue}\n"),
            }
        }
        for (target, kind, message) in &self.failed {
            let kind = if *kind == ErrorKind::Retryable {
                "retryable"
            } else {
                "permanent"
            };
            out += &format!("failed {target}: {kind}: {message}\n");
        }
        for note in &self.notes {
            out += &format!("{note}\n");
        }
        out += &format!(
            "range {}..{} ({}): {} PRs, {} updated, {} skipped, {} commented, {} failed\n",
            self.from.as_deref().unwrap_or("-"),
            self.to,
            self.mode,
            self.prs.len(),
            self.updated.len(),
            self.skipped.len(),
            self.commented.len(),
            self.failed.len()
        );
        out
    }
    fn json(&self) -> Value {
        json!({
            "range": {"from": self.from, "to": self.to, "mode": self.mode, "reason": self.reason},
            "prs": self.prs,
            "updated": self.updated,
            "skipped": self.skipped.iter().map(|(id, reason)| json!({"id": id, "reason": reason})).collect::<Vec<_>>(),
            "commented": self.commented.iter().map(|(issue, pr, kind, _)| json!({"issue": issue, "pr": pr, "kind": kind})).collect::<Vec<_>>(),
            "existing": self.existing.iter().map(|(issue, pr)| json!({"issue": issue, "pr": pr})).collect::<Vec<_>>(),
            "failed": self.failed.iter().map(|(target, kind, message)| json!({
                "target": target,
                "kind": if *kind == ErrorKind::Retryable { "retryable" } else { "permanent" },
                "message": message,
            })).collect::<Vec<_>>(),
        })
    }
}

struct Repo<'a> {
    root: &'a Path,
    dir: String,
    prefix: String,
}
impl Repo<'_> {
    fn path(&self, id: &str) -> String {
        format!("{}/{id}.md", self.dir)
    }
    fn card_at(&self, rev: &str, id: &str) -> Option<String> {
        show(self.root, rev, &self.path(id))
    }
    /// Every card ID at HEAD.
    fn all_ids(&self) -> Result<BTreeSet<String>, TrackerError> {
        let listed = git(
            self.root,
            &["ls-tree", "-r", "--name-only", "HEAD", "--", &self.dir],
        )?;
        Ok(card_ids(&listed, &self.dir, &self.prefix))
    }
    /// The card whose `remote` is `n` at HEAD.
    fn id_by_remote(&self, n: u64) -> Result<Option<String>, TrackerError> {
        for id in self.all_ids()? {
            let remote = self
                .card_at("HEAD", &id)
                .as_deref()
                .and_then(parse_card)
                .and_then(|c| remote_of(&c));
            if remote == Some(n) {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }
}

/// Mirrors one card from HEAD. Returns its parsed value when it was mirrored.
fn mirror(repo: &Repo, tracker: &mut dyn MergeTracker, id: &str, report: &mut Report) {
    let Some(text) = repo.card_at("HEAD", id) else {
        report.skipped.push((id.into(), "deleted".into()));
        return;
    };
    let Some(card) = parse_card(&text) else {
        report.skipped.push((id.into(), "unparseable".into()));
        return;
    };
    if card["id"].as_str() != Some(id) {
        report.skipped.push((id.into(), "id_mismatch".into()));
        return;
    }
    if card
        .get(crate::PENDING_CREATE)
        .is_some_and(|v| !v.is_null())
    {
        report.skipped.push((id.into(), "pending_create".into()));
        return;
    }
    if remote_of(&card).is_none() {
        report.skipped.push((id.into(), "unlinked".into()));
        return;
    }
    match tracker.update(&card) {
        Ok(()) => report.updated.push(id.into()),
        Err(e) => report.fail(id.into(), e),
    }
}

struct Settings<'a> {
    workflow: &'a str,
    branch: Option<&'a str>,
    all: bool,
    /// The range start from the push event (`--from`), used in place of the run history.
    from: Option<&'a str>,
}

/// `sha` when it is an ancestor of HEAD, else the reason it cannot start the range.
fn anchor(root: &Path, sha: &str) -> Result<String, &'static str> {
    let ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", sha, "HEAD"])
        .current_dir(root)
        .status()
        .is_ok_and(|s| s.success());
    if ancestor {
        Ok(sha.to_string())
    } else {
        Err("anchor is not an ancestor of HEAD")
    }
}

fn sync(repo: &Repo, tracker: &mut dyn MergeTracker, settings: &Settings, report: &mut Report) {
    report.to = git(repo.root, &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    // Range: from --from or the last successful push run, else the --all path.
    let mut start = None;
    let mut take = |found: Result<String, &str>, report: &mut Report| match found {
        Ok(sha) => start = Some(sha),
        Err(reason) => report.reason = Some(reason.into()),
    };
    if settings.all {
        report.reason = Some("requested".into());
    } else if let Some(from) = settings.from {
        // A push that creates the branch carries the zero commit as `before`.
        if from.is_empty() || from.bytes().all(|b| b == b'0') {
            take(Err("push has no earlier commit"), report);
        } else {
            take(anchor(repo.root, from), report);
        }
    } else if let Some(branch) = settings.branch {
        match tracker.last_success(settings.workflow, branch) {
            Ok(None) => report.reason = Some("no earlier successful push run".into()),
            Err(_) => report.reason = Some("run history cannot be read".into()),
            Ok(Some(sha)) => take(anchor(repo.root, &sha), report),
        }
    }
    let Some(start) = start else {
        report.mode = "all";
        report.notes.push(format!(
            "all: {}",
            report.reason.clone().unwrap_or_default()
        ));
        match repo.all_ids() {
            Ok(ids) => ids.iter().for_each(|id| mirror(repo, tracker, id, report)),
            Err(e) => report.fail("backlog".into(), e),
        }
        return;
    };
    report.mode = "range";
    report.from = Some(start.clone());
    let branch = settings.branch.unwrap_or_default();
    let commits: Vec<String> = match git(
        repo.root,
        &["rev-list", "--first-parent", &format!("{start}..HEAD")],
    ) {
        Ok(out) => out.lines().map(str::to_string).collect(),
        Err(e) => return report.fail("range".into(), e),
    };
    let in_range: BTreeSet<&str> = commits.iter().map(String::as_str).collect();
    // Merged pull requests whose merge commit is in the range.
    let mut pulls: BTreeMap<u64, MergedPull> = BTreeMap::new();
    for sha in &commits {
        match tracker.pulls_for_commit(sha) {
            Ok(found) => {
                let kept: Vec<_> = found
                    .into_iter()
                    .filter(|p| {
                        p.merged
                            && p.base == branch
                            && p.merge_commit_sha
                                .as_deref()
                                .is_some_and(|m| in_range.contains(m))
                    })
                    .collect();
                if kept.is_empty() {
                    let changed = git(
                        repo.root,
                        &[
                            "diff",
                            "--name-only",
                            "--no-renames",
                            &format!("{sha}^1"),
                            sha,
                            "--",
                            &repo.dir,
                        ],
                    )
                    .map(|out| card_ids(&out, &repo.dir, &repo.prefix).len())
                    .unwrap_or(0);
                    let noun = if changed == 1 { "card" } else { "cards" };
                    report.notes.push(format!(
                        "no pull request for {sha}; mirrored {changed} {noun}"
                    ));
                }
                for p in kept {
                    pulls.insert(p.number, p);
                }
            }
            Err(e) => report.fail(format!("commit {sha}"), e),
        }
    }
    report.prs = pulls.keys().copied().collect();
    // Cards to mirror: every card the range changed, and every card a kept PR names.
    let changed = match git(
        repo.root,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            &start,
            "HEAD",
            "--",
            &repo.dir,
        ],
    ) {
        Ok(out) => card_ids(&out, &repo.dir, &repo.prefix),
        Err(e) => return report.fail("range".into(), e),
    };
    let mut named: BTreeMap<u64, Vec<Reference>> = BTreeMap::new();
    let mut named_ids = BTreeSet::new();
    for (number, pull) in &pulls {
        let refs = stories_line::parse(&pull.body, &repo.prefix);
        if refs.is_empty() {
            report
                .notes
                .push(format!("pull request #{number} has no Stories: line"));
        }
        for r in &refs {
            if let Some(id) = &r.id {
                named_ids.insert(id.clone());
            }
        }
        named.insert(*number, refs);
    }
    let mut selected: BTreeSet<String> = changed.clone();
    selected.extend(named_ids.iter().cloned());
    for id in &selected {
        mirror(repo, tracker, id, report);
    }
    // Merge comments: one per PR on each issue it names.
    for (number, refs) in &named {
        let sha = pulls[number].merge_commit_sha.clone().unwrap_or_default();
        let marker = format!("<!-- story-time:v1:merge-sync={sha} -->");
        let body = format!("Merged in #{number} ({sha})");
        for r in refs {
            let card_remote = match &r.id {
                Some(id) => repo
                    .card_at("HEAD", id)
                    .as_deref()
                    .and_then(parse_card)
                    .and_then(|c| remote_of(&c)),
                None => None,
            };
            let issue = match (r.issue, card_remote, &r.id) {
                (Some(n), Some(m), Some(id)) if n != m => {
                    report.fail(
                        r.label(),
                        TrackerError::permanent(format!("{id} (#{n}) does not match remote {m}")),
                    );
                    continue;
                }
                (Some(n), _, _) => n,
                (None, Some(m), _) => m,
                (None, None, Some(id)) => {
                    report.skipped.push((id.clone(), "unlinked".into()));
                    continue;
                }
                (None, None, None) => continue,
            };
            if r.id.is_none() {
                match repo.id_by_remote(issue) {
                    Ok(Some(id)) => {
                        if !selected.contains(&id) {
                            mirror(repo, tracker, &id, report);
                        }
                    }
                    Ok(None) => {
                        report
                            .skipped
                            .push((format!("#{issue}"), "unmapped".into()));
                        continue;
                    }
                    Err(e) => {
                        report.fail(format!("#{issue}"), e);
                        continue;
                    }
                }
            }
            match tracker.comment_once(issue, &marker, &body) {
                Ok(CommentOutcome::Posted) => {
                    report
                        .commented
                        .push((issue, Some(*number), "merge", sha.clone()))
                }
                Ok(CommentOutcome::Existing) => report.existing.push((issue, Some(*number))),
                Err(e) => report.fail(format!("#{issue}"), e),
            }
        }
    }
    // State comments: a card that no kept PR names and whose status changed in the range.
    for id in changed.difference(&named_ids) {
        let Some(now) = repo.card_at("HEAD", id).as_deref().and_then(parse_card) else {
            continue;
        };
        let Some(issue) = remote_of(&now) else {
            continue;
        };
        let before = repo.card_at(&start, id).as_deref().and_then(parse_card);
        let status = now["status"].as_str().unwrap_or("");
        if before.as_ref().and_then(|b| b["status"].as_str()) == Some(status) {
            continue;
        }
        let commit = match git(
            repo.root,
            &[
                "log",
                "-1",
                "--first-parent",
                "--format=%H",
                &format!("{start}..HEAD"),
                "--",
                &repo.path(id),
            ],
        ) {
            Ok(out) => out.trim().to_string(),
            Err(e) => {
                report.fail(id.clone(), e);
                continue;
            }
        };
        let marker = format!("<!-- story-time:v1:merge-sync-state={commit} -->");
        match tracker.comment_once(issue, &marker, &format!("Status {status} from {commit}")) {
            Ok(CommentOutcome::Posted) => report.commented.push((issue, None, "state", commit)),
            Ok(CommentOutcome::Existing) => report.existing.push((issue, None)),
            Err(e) => report.fail(format!("#{issue}"), e),
        }
    }
}

pub(crate) fn run(args: &[String]) -> Result<(String, u8), Failure> {
    let usage = |message: String| Failure { code: 2, message };
    let flags = config::Flags::parse(args)?;
    if let Some(name) = flags.names().iter().find(|n| !FLAGS.contains(&n.as_str())) {
        return Err(usage(format!("Unknown sync-merge flag --{name}.")));
    }
    if !flags.positional.is_empty() {
        return Err(usage("sync-merge takes no positional arguments.".into()));
    }
    let all = flags.has("all");
    let workflow = flags
        .value("workflow")
        .unwrap_or("story-sync.yml")
        .to_string();
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let (workflow_repo, branch) = (env("GITHUB_REPOSITORY"), env("GITHUB_REF_NAME"));
    if !all && (workflow_repo.is_none() || branch.is_none()) {
        return Err(usage(
            "sync-merge needs GITHUB_REPOSITORY and GITHUB_REF_NAME, or --all.".into(),
        ));
    }
    let ctx = crate::context(&flags)?;
    let head = git(&ctx.root, &["rev-parse", "HEAD"]).map_err(|e| e.message)?;
    let head = head.trim();
    let output = |text: String, json_value: Value, code: u8| {
        if let Some(path) = env("GITHUB_STEP_SUMMARY") {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)
            {
                let _ = writeln!(file, "```\n{text}```");
            }
        }
        let out = if flags.has("json") {
            format!("{json_value}\n")
        } else {
            text
        };
        Ok((out, code))
    };
    let skip = |reason: &str| {
        let text = format!("skipped ({reason}) at {head}\n");
        output(text, json!({"skipped": reason, "at": head}), 0)
    };
    if ctx.config["sync"] == "off" {
        return skip("sync off");
    }
    if ctx.config["tracker"] != "github" && ctx.config["tracker"] != "gitea" {
        return skip("unsupported tracker");
    }
    let mut gateway = Gateway::new(&ctx.config, &ctx.root)?;
    let repo_name = gateway.repo_name().unwrap_or_default();
    if let Some(workflow_repo) = &workflow_repo {
        if !workflow_repo.eq_ignore_ascii_case(&repo_name) {
            let mut report = Report {
                to: head.into(),
                mode: "range",
                ..Report::default()
            };
            report.fail(
                "settings".into(),
                TrackerError::permanent(format!(
                    "the tracker repository {repo_name} is not the workflow repository {workflow_repo}"
                )),
            );
            return output(report.text(), report.json(), 0);
        }
    }
    if let Some(branch) = &branch {
        match gateway.default_branch() {
            Ok(default) if &default == branch => {}
            Ok(default) => {
                return Err(usage(format!(
                    "sync-merge runs only on the default branch {default}, not {branch}."
                )))
            }
            Err(e) => {
                return Err(Failure {
                    code: 1,
                    message: e.message,
                })
            }
        }
    }
    let dir = ctx
        .dir
        .strip_prefix(&ctx.root)
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "backlog".into());
    let repo = Repo {
        root: &ctx.root,
        dir,
        prefix: ctx.config["prefix"].as_str().unwrap_or("ST").to_string(),
    };
    let settings = Settings {
        workflow: &workflow,
        branch: branch.as_deref(),
        all,
        from: flags.value("from"),
    };
    let mut report = Report::default();
    sync(&repo, &mut gateway, &settings, &mut report);
    let retryable = report
        .failed
        .iter()
        .any(|(_, kind, _)| *kind == ErrorKind::Retryable);
    output(report.text(), report.json(), u8::from(retryable))
}
