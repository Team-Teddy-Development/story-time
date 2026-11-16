//! ST-151: `story sync-merge` against a temporary git repository and a fake `gh`.
mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const BOT: u64 = 41898282;

struct Repo(PathBuf);
impl Repo {
    fn new(settings: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-sync-merge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::write(path.join(".story-time/settings.json"), settings).unwrap();
        fs::write(
            path.join(".gitignore"),
            "bin/\nresponses.tsv\nresponse-*\ngh-calls\n",
        )
        .unwrap();
        common::install_stub_gh(&path.join("bin"));
        let repo = Self(path.canonicalize().unwrap());
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "t@example.com"]);
        repo.git(&["config", "user.name", "T"]);
        common::respond(&repo.0, "repo view", 0, "main\n", "");
        common::respond(&repo.0, "label list", 0, "[]\n", "");
        repo
    }
    fn github() -> Self {
        Self::new(r#"{"tracker":"github","sync":"auto","github":{"repo":"acme/shop"}}"#)
    }
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
    fn card(&self, id: &str, status: &str, remote: Option<u64>) {
        let remote = remote.map(|n| format!("remote: {n}\n")).unwrap_or_default();
        fs::write(
            self.0.join(format!("backlog/{id}.md")),
            format!("---\nid: {id}\ntitle: Story {id}\nstatus: {status}\ntype: story\n{remote}---\n\n## Story\nAs a user, I want it.\n\n## Acceptance Criteria\n- [x] Given a, when b, then c\n"),
        )
        .unwrap();
    }
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
    fn last_success(&self, sha: &str) {
        common::respond_first(
            &self.0,
            "run list",
            0,
            &format!("[{{\"headSha\":\"{sha}\"}}]\n"),
            "",
        );
    }
    fn pull(&self, sha: &str, number: u64, body: &str) {
        let pr = serde_json::json!([{"number": number, "body": body, "base": {"ref": "main"}, "merged_at": "2026-11-16T00:00:00Z", "merge_commit_sha": sha}]);
        common::respond(
            &self.0,
            &format!("api repos/acme/shop/commits/{sha}/pulls"),
            0,
            &format!("{pr}\n"),
            "",
        );
    }
    fn issue(&self, n: u64, state: &str) {
        common::respond(
            &self.0,
            &format!("issue view {n} "),
            0,
            &format!("{{\"labels\":[],\"state\":\"{state}\",\"url\":\"https://github.com/acme/shop/issues/{n}\"}}\n"),
            "",
        );
    }
    fn comments(&self, n: u64, lines: &[(u64, &str)]) {
        let out: String = lines
            .iter()
            .map(|(user, body)| format!("{}\n", serde_json::json!([user, body])))
            .collect();
        common::respond_first(
            &self.0,
            &format!("api --paginate repos/acme/shop/issues/{n}/comments"),
            0,
            &out,
            "",
        );
    }
    fn sync(&self, extra: &[&str]) -> Output {
        // GitHub answers an empty list for a commit with no pull request.
        common::respond(&self.0, "api repos/acme/shop/commits/", 0, "[]\n", "");
        Command::new(env!("CARGO_BIN_EXE_story"))
            .arg("sync-merge")
            .args(extra)
            .env_clear()
            .env("PATH", common::path_with_git(&self.0.join("bin")))
            .env("HOME", &self.0)
            .env("GH_TOKEN", "test-token")
            .env("GITHUB_REPOSITORY", "acme/shop")
            .env("GITHUB_REF_NAME", "main")
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.0.join("gh-calls")).unwrap_or_default()
    }
    fn reset_calls(&self) {
        let _ = fs::remove_file(self.0.join("gh-calls"));
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}
fn count(calls: &str, needle: &str) -> usize {
    calls.lines().filter(|l| l.starts_with(needle)).count()
}
fn report(o: &Output) -> serde_json::Value {
    serde_json::from_slice(&o.stdout).unwrap_or_else(|_| panic!("{}\n{}", out(o), err(o)))
}

/// A base commit with two linked cards, then a merge commit that sets both done.
fn merged_two(r: &Repo) -> (String, String) {
    r.card("ST-001", "in-progress", Some(12));
    r.card("ST-002", "in-progress", Some(13));
    let base = r.commit("base");
    r.card("ST-001", "done", Some(12));
    r.card("ST-002", "done", Some(13));
    let merge = r.commit("Merge pull request #20");
    r.last_success(&base);
    r.pull(&merge, 20, "Stories: ST-001 (#12), ST-002 (#13)");
    r.issue(12, "OPEN");
    r.issue(13, "OPEN");
    (base, merge)
}

#[test]
fn ac1_each_named_issue_gets_one_merge_comment_and_a_rerun_adds_none() {
    let r = Repo::github();
    let (_, merge) = merged_two(&r);
    let o = r.sync(&["--json"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", out(&o), err(&o));
    let calls = r.calls();
    assert_eq!(count(&calls, "issue comment 12 "), 1, "{calls}");
    assert_eq!(count(&calls, "issue comment 13 "), 1, "{calls}");
    let rep = report(&o);
    assert_eq!(rep["prs"], serde_json::json!([20]));
    assert_eq!(rep["commented"].as_array().unwrap().len(), 2);
    // The second run sees the bot's markers.
    let marker = format!("Merged in #20 ({merge})\n<!-- story-time:v1:merge-sync={merge} -->");
    r.comments(12, &[(BOT, &marker)]);
    r.comments(13, &[(BOT, &marker)]);
    r.reset_calls();
    let o = r.sync(&["--json"]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(count(&r.calls(), "issue comment"), 0, "{}", r.calls());
    assert_eq!(report(&o)["existing"].as_array().unwrap().len(), 2);
}

#[test]
fn ac2_issues_close_and_reopen_with_the_card() {
    let r = Repo::github();
    r.card("ST-001", "in-progress", Some(12));
    r.card("ST-002", "done", Some(13));
    let base = r.commit("base");
    r.card("ST-001", "done", Some(12));
    r.card("ST-002", "in-progress", Some(13));
    let merge = r.commit("Merge pull request #21");
    r.last_success(&base);
    r.pull(&merge, 21, "Stories: ST-001 (#12), ST-002 (#13)");
    r.issue(12, "OPEN");
    r.issue(13, "CLOSED");
    let o = r.sync(&[]);
    assert_eq!(o.status.code(), Some(0), "{}{}", out(&o), err(&o));
    let calls = r.calls();
    assert_eq!(count(&calls, "issue close 12 "), 1, "{calls}");
    assert_eq!(count(&calls, "issue reopen 13 "), 1, "{calls}");
    assert_eq!(
        count(&calls, "issue comment 12 "),
        1,
        "the close has its merge comment: {calls}"
    );
}

#[test]
fn ac3_a_drifted_issue_is_updated_to_match_the_card() {
    let r = Repo::github();
    let _ = merged_two(&r);
    let o = r.sync(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    let calls = r.calls();
    assert!(
        calls.lines().any(|l| l.starts_with(
            "issue edit 12 --repo acme/shop --title ST-001: Story ST-001 --body-file -"
        )),
        "{calls}"
    );
}

#[test]
fn ac4_card_bytes_are_unchanged_and_a_not_done_story_stays_open() {
    let r = Repo::github();
    r.card("ST-001", "ready", Some(12));
    let base = r.commit("base");
    r.card("ST-001", "in-progress", Some(12));
    let merge = r.commit("Merge pull request #22");
    r.last_success(&base);
    r.pull(&merge, 22, "Stories: ST-001 (#12)");
    r.issue(12, "OPEN");
    let before = fs::read(r.0.join("backlog/ST-001.md")).unwrap();
    let o = r.sync(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    assert_eq!(fs::read(r.0.join("backlog/ST-001.md")).unwrap(), before);
    assert_eq!(count(&r.calls(), "issue close"), 0, "{}", r.calls());
}

#[test]
fn ac5_sync_off_and_files_make_no_tracker_call() {
    for settings in [
        r#"{"tracker":"github","sync":"off","github":{"repo":"acme/shop"}}"#,
        r#"{"tracker":"files","sync":"off"}"#,
    ] {
        let r = Repo::new(settings);
        r.card("ST-001", "done", Some(12));
        r.commit("base");
        let o = r.sync(&[]);
        assert_eq!(o.status.code(), Some(0), "{}", err(&o));
        assert!(out(&o).starts_with("skipped (sync off) at "), "{}", out(&o));
        assert_eq!(r.calls(), "", "{settings}");
    }
}

#[test]
fn ac6_a_push_with_no_pull_request_mirrors_and_posts_a_state_comment() {
    let r = Repo::github();
    r.card("ST-909", "in-progress", Some(30));
    let base = r.commit("base");
    r.card("ST-909", "archived", Some(30));
    let push = r.commit("archive ST-909");
    r.last_success(&base);
    r.issue(30, "OPEN");
    let o = r.sync(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains(&format!("no pull request for {push}; mirrored 1 card")),
        "{text}"
    );
    let calls = r.calls();
    assert_eq!(count(&calls, "issue close 30 "), 1, "{calls}");
    assert_eq!(count(&calls, "issue comment 30 "), 1, "{calls}");
    assert!(text.contains(&format!("state #30 ({push})")), "{text}");
}

#[test]
fn ac7_the_checkout_stays_clean() {
    let r = Repo::github();
    let _ = merged_two(&r);
    let o = r.sync(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    assert_eq!(r.git(&["status", "--porcelain"]), "");
}

#[test]
fn a_successful_all_run_does_not_move_the_range_start() {
    // The anchor comes only from push runs, so the stub's answer to `run list` is the push anchor.
    let r = Repo::github();
    let _ = merged_two(&r);
    let _ = r.sync(&[]);
    assert!(r.calls().lines().any(|l| l.starts_with("run list --repo acme/shop --workflow story-sync.yml --branch main --event push --status success --limit 1")), "{}", r.calls());
}

#[test]
fn no_history_or_a_non_ancestor_anchor_takes_the_all_path() {
    let r = Repo::github();
    r.card("ST-001", "done", Some(12));
    r.commit("base");
    r.issue(12, "OPEN");
    common::respond_first(&r.0, "run list", 0, "[]\n", "");
    let o = r.sync(&["--json"]);
    let rep = report(&o);
    assert_eq!(rep["range"]["mode"], "all");
    assert_eq!(rep["range"]["reason"], "no earlier successful push run");
    assert_eq!(rep["updated"], serde_json::json!(["ST-001"]));
    assert_eq!(count(&r.calls(), "issue comment"), 0);
    r.last_success("0000000000000000000000000000000000000000");
    let rep = report(&r.sync(&["--json"]));
    assert_eq!(rep["range"]["reason"], "anchor is not an ancestor of HEAD");
}

#[test]
fn a_merge_of_a_long_branch_makes_one_pull_request_lookup() {
    let r = Repo::github();
    r.card("ST-001", "in-progress", Some(12));
    let base = r.commit("base");
    r.git(&["switch", "-q", "-c", "topic"]);
    for i in 0..50 {
        r.commit(&format!("work {i}"));
    }
    r.git(&["switch", "-q", "main"]);
    r.card("ST-001", "done", Some(12));
    r.git(&["add", "-A"]);
    r.git(&["commit", "-q", "-m", "card"]);
    r.git(&[
        "merge",
        "-q",
        "--no-ff",
        "topic",
        "-m",
        "Merge pull request #23",
    ]);
    let merge = r.git(&["rev-parse", "HEAD"]);
    r.last_success(&base);
    r.pull(&merge, 23, "Stories: ST-001 (#12)");
    r.issue(12, "OPEN");
    let o = r.sync(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    assert_eq!(
        count(&r.calls(), "api repos/acme/shop/commits/"),
        2,
        "{}",
        r.calls()
    );
}

#[test]
fn a_server_error_exits_1_and_a_missing_issue_exits_0() {
    let r = Repo::github();
    let _ = merged_two(&r);
    common::respond_first(
        &r.0,
        "issue view 12 ",
        1,
        "",
        "HTTP 503: Service Unavailable",
    );
    let o = r.sync(&["--json"]);
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert_eq!(report(&o)["failed"][0]["kind"], "retryable");
    common::respond_first(&r.0, "issue view 12 ", 1, "", "HTTP 404: Not Found");
    let o = r.sync(&["--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
    assert_eq!(report(&o)["failed"][0]["kind"], "permanent");
    assert!(
        !out(&o).contains("HTTP 404"),
        "tracker output never reaches the report"
    );
}

#[test]
fn a_marker_from_another_user_does_not_suppress_the_comment() {
    let r = Repo::github();
    let (_, merge) = merged_two(&r);
    let marker = format!("<!-- story-time:v1:merge-sync={merge} -->");
    r.comments(12, &[(7, &marker)]);
    let o = r.sync(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    assert_eq!(count(&r.calls(), "issue comment 12 "), 1);
}

#[test]
fn a_rerun_posts_a_missed_state_comment_once() {
    let r = Repo::github();
    r.card("ST-909", "in-progress", Some(30));
    let base = r.commit("base");
    r.card("ST-909", "done", Some(30));
    let push = r.commit("done directly");
    r.last_success(&base);
    r.issue(30, "CLOSED");
    common::respond_first(&r.0, "issue comment 30", 1, "", "HTTP 502");
    assert_eq!(r.sync(&[]).status.code(), Some(1));
    fs::write(
        r.0.join("responses.tsv"),
        fs::read_to_string(r.0.join("responses.tsv"))
            .unwrap()
            .lines()
            .skip(1)
            .map(|l| format!("{l}\n"))
            .collect::<String>(),
    )
    .unwrap();
    r.reset_calls();
    assert_eq!(r.sync(&[]).status.code(), Some(0));
    assert_eq!(count(&r.calls(), "issue comment 30 "), 1);
    r.comments(
        30,
        &[(
            BOT,
            &format!("Status done from {push}\n<!-- story-time:v1:merge-sync-state={push} -->"),
        )],
    );
    r.reset_calls();
    assert_eq!(r.sync(&[]).status.code(), Some(0));
    assert_eq!(count(&r.calls(), "issue comment"), 0);
}

#[test]
fn skipped_cards_are_reported_with_their_reasons() {
    let r = Repo::github();
    r.card("ST-001", "ready", Some(12));
    r.card("ST-002", "ready", None);
    let base = r.commit("base");
    fs::remove_file(r.0.join("backlog/ST-001.md")).unwrap();
    r.card("ST-002", "in-progress", None);
    fs::write(
        r.0.join("backlog/ST-003.md"),
        "---\nid: ST-004\ntitle: x\nstatus: ready\ntype: story\n---\n",
    )
    .unwrap();
    r.card("ST-005", "ready", None);
    let text = fs::read_to_string(r.0.join("backlog/ST-005.md"))
        .unwrap()
        .replace("type: story\n", "type: story\npending_create: 2026-11-16\n");
    fs::write(r.0.join("backlog/ST-005.md"), text).unwrap();
    r.commit("changes");
    r.last_success(&base);
    let rep = report(&r.sync(&["--json"]));
    let reasons: Vec<(String, String)> = rep["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["id"].as_str().unwrap().into(),
                s["reason"].as_str().unwrap().into(),
            )
        })
        .collect();
    for expected in [
        ("ST-001", "deleted"),
        ("ST-002", "unlinked"),
        ("ST-003", "id_mismatch"),
        ("ST-005", "pending_create"),
    ] {
        assert!(
            reasons.contains(&(expected.0.into(), expected.1.into())),
            "{reasons:?}"
        );
    }
}

#[test]
fn a_bare_issue_reference_with_no_card_is_unmapped() {
    let r = Repo::github();
    r.card("ST-001", "ready", Some(12));
    let base = r.commit("base");
    let merge = r.commit("Merge pull request #24");
    r.last_success(&base);
    r.pull(&merge, 24, "Stories: #99");
    let rep = report(&r.sync(&["--json"]));
    assert_eq!(
        rep["skipped"][0],
        serde_json::json!({"id": "#99", "reason": "unmapped"})
    );
    assert_eq!(count(&r.calls(), "issue comment"), 0);
}

#[test]
fn a_feature_branch_ref_is_refused() {
    let r = Repo::github();
    r.commit("base");
    let o = Command::new(env!("CARGO_BIN_EXE_story"))
        .arg("sync-merge")
        .env_clear()
        .env("PATH", common::path_with_git(&r.0.join("bin")))
        .env("GH_TOKEN", "test-token")
        .env("GITHUB_REPOSITORY", "acme/shop")
        .env("GITHUB_REF_NAME", "feature")
        .current_dir(&r.0)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2), "{}", err(&o));
}

#[test]
fn all_mirrors_a_card_that_no_range_changed() {
    let r = Repo::github();
    r.card("ST-001", "done", Some(12));
    r.commit("base");
    r.issue(12, "OPEN");
    let rep = report(&r.sync(&["--all", "--json"]));
    assert_eq!(rep["updated"], serde_json::json!(["ST-001"]));
    assert_eq!(count(&r.calls(), "run list"), 0);
    let _ = Path::new("unused");
}
