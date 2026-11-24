//! `story check-pr`: check a pull request's `Stories:` line, the acceptance of each named story,
//! and its closing keywords (ST-152). Design: `docs/designs/merge-sync-design.md`.

use crate::stories_line::{self, Reference};
use crate::{config, gateway::Cli, lenient_story, story, Failure};
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, path::Path};

const FLAGS: [&str; 5] = ["body-file", "json", "config", "dir", "link-for"];
const STORY_LABELS: [&str; 4] = ["story", "spike", "bug", "chore"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IssueKind {
    Story,
    Other,
    PullRequest,
}

pub(crate) trait IssueSource {
    fn kind(&mut self, n: u64) -> Result<IssueKind, String>;
}

/// Looks up issues in `REPO` with `gh`, whatever the `tracker` setting is.
struct GhIssues {
    cli: Cli,
    repo: Option<String>,
}
impl IssueSource for GhIssues {
    fn kind(&mut self, n: u64) -> Result<IssueKind, String> {
        let repo = self.repo.as_deref().ok_or("REPO is not set.")?;
        let out = self.cli.run(
            &[
                "api".into(),
                format!("repos/{repo}/issues/{n}"),
                "--jq".into(),
                "if .pull_request then \"PULL_REQUEST\" else .labels[].name end".into(),
            ],
            None,
        )?;
        let names: Vec<&str> = out.lines().map(str::trim).collect();
        Ok(if names.contains(&"PULL_REQUEST") {
            IssueKind::PullRequest
        } else if names.iter().any(|n| STORY_LABELS.contains(n)) {
            IssueKind::Story
        } else {
            IssueKind::Other
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Warn,
    Fail,
}

#[derive(Debug)]
struct Finding {
    rule: u8,
    reference: String,
    line: String,
}

/// What `evaluate` needs to know about the repository.
struct Input<'a> {
    body: &'a str,
    prefix: &'a str,
    repo: &'a str,
    incident: bool,
    sync_off: bool,
}

/// A card read for rules 2 and 3.
struct Card {
    id: String,
    status: String,
    remote: Option<u64>,
    criteria: Vec<bool>,
}

fn card_from(value: &Value, id: &str) -> Card {
    let remote = match &value["remote"] {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    };
    Card {
        id: id.to_string(),
        status: value["status"].as_str().unwrap_or("").to_string(),
        remote,
        criteria: story::acceptance_criteria(value)
            .iter()
            .map(|c| c["done"].as_bool().unwrap_or(false))
            .collect(),
    }
}

/// Reads backlog cards on demand.
trait Cards {
    fn by_id(&mut self, id: &str) -> Result<Option<Card>, String>;
    fn by_remote(&mut self, n: u64) -> Result<Option<Card>, String>;
}

struct Backlog<'a> {
    dir: &'a Path,
    all: Option<Vec<Card>>,
}
impl Backlog<'_> {
    fn read(&self, id: &str) -> Result<Option<Card>, String> {
        let path = self.dir.join(format!("{id}.md"));
        if !path.is_file() {
            return Ok(None);
        }
        let text =
            fs::read_to_string(&path).map_err(|_| format!("{id}: card could not be read"))?;
        let (parsed, _) = lenient_story::parse_noting_drops(&text)
            .map_err(|_| format!("{id}: card could not be parsed"))?;
        Ok(Some(card_from(&parsed.to_value(), id)))
    }
}
impl Cards for Backlog<'_> {
    fn by_id(&mut self, id: &str) -> Result<Option<Card>, String> {
        self.read(id)
    }
    fn by_remote(&mut self, n: u64) -> Result<Option<Card>, String> {
        if self.all.is_none() {
            let mut cards = Vec::new();
            let entries = fs::read_dir(self.dir).map_err(|_| "Could not read the backlog.")?;
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if let Some(id) = crate::story_file_id(&name) {
                    if let Ok(Some(card)) = self.read(id) {
                        cards.push(card);
                    }
                }
            }
            self.all = Some(cards);
        }
        Ok(self
            .all
            .as_ref()
            .and_then(|all| all.iter().find(|c| c.remote == Some(n)))
            .map(|c| Card {
                id: c.id.clone(),
                status: c.status.clone(),
                remote: c.remote,
                criteria: c.criteria.clone(),
            }))
    }
}

fn add(findings: &mut Vec<Finding>, rule: u8, reference: String, line: String) {
    findings.push(Finding {
        rule,
        reference,
        line,
    });
}

fn evaluate(
    input: &Input,
    cards: &mut dyn Cards,
    issues: &mut dyn IssueSource,
) -> (Vec<Reference>, Vec<Finding>) {
    let refs = stories_line::parse(input.body, input.prefix);
    let mut findings = Vec::new();
    if refs.is_empty() && !input.incident {
        add(
            &mut findings,
            1,
            "PR body".into(),
            format!(
                "PR body: no Stories: line; add Stories: {}-### (#issue)",
                input.prefix
            ),
        );
    }
    // Rule 1: every ID has a card, and every issue is a story issue.
    let mut resolved: Vec<Option<Card>> = Vec::new();
    let mut linked: Vec<bool> = Vec::new();
    for r in &refs {
        let label = r.label();
        let before = findings.len();
        let mut card = None;
        if let Some(id) = &r.id {
            match cards.by_id(id) {
                Ok(Some(c)) => card = Some(c),
                Ok(None) => add(
                    &mut findings,
                    1,
                    label.clone(),
                    format!("{id}: card not found"),
                ),
                Err(e) => add(&mut findings, 1, label.clone(), e),
            }
        }
        if let Some(n) = r.issue {
            match issues.kind(n) {
                Ok(IssueKind::Story) => {}
                Ok(IssueKind::Other) => add(
                    &mut findings,
                    1,
                    label.clone(),
                    format!("#{n}: not a story issue"),
                ),
                Ok(IssueKind::PullRequest) => add(
                    &mut findings,
                    1,
                    label.clone(),
                    format!("#{n}: is a pull request"),
                ),
                Err(_) => add(
                    &mut findings,
                    1,
                    label.clone(),
                    format!("#{n}: issue lookup failed"),
                ),
            }
        }
        resolved.push(card);
        linked.push(findings.len() == before);
    }
    if input.incident {
        return (refs, findings);
    }
    // Rules 2 and 3 for each reference not marked (partial).
    let mut covered = BTreeSet::new();
    for ((r, card), linked) in refs.iter().zip(resolved).zip(linked) {
        if r.partial || !linked {
            continue;
        }
        let label = r.label();
        let card = match (card, &r.id, r.issue) {
            (Some(c), _, _) => c,
            (None, Some(_), _) => continue, // already a rule 1 failure
            (None, None, Some(n)) => match cards.by_remote(n) {
                Ok(Some(c)) => c,
                Ok(None) => {
                    add(
                        &mut findings,
                        2,
                        label,
                        format!("#{n}: no card has remote {n}"),
                    );
                    continue;
                }
                Err(e) => {
                    add(&mut findings, 1, label, e);
                    continue;
                }
            },
            (None, None, None) => continue,
        };
        let mut passed = true;
        let unticked = card.criteria.iter().filter(|done| !**done).count();
        if card.status != "done" || unticked > 0 {
            passed = false;
            add(&mut findings, 2,
                label.clone(),
                format!(
                    "{} is {} with {unticked} unticked criteria; run story-accept or mark the reference (partial)",
                    card.id, card.status
                ),
            );
        } else if card.criteria.is_empty() {
            passed = false;
            add(
                &mut findings,
                2,
                label.clone(),
                format!("{} has no acceptance criteria", card.id),
            );
        }
        if !input.sync_off {
            match (card.remote, r.issue) {
                (None, _) => {
                    passed = false;
                    add(
                        &mut findings,
                        3,
                        label.clone(),
                        format!("{} has no remote; run story push before merge", card.id),
                    );
                }
                (Some(m), Some(n)) if m != n => {
                    passed = false;
                    add(
                        &mut findings,
                        3,
                        label.clone(),
                        format!("{label} does not match remote {m}"),
                    );
                }
                _ => {}
            }
        }
        if passed {
            covered.extend(card.remote);
            covered.extend(r.issue);
        }
    }
    // Rule 4: a closing keyword may target only an issue that an accepted reference covers.
    for n in stories_line::closing_targets(input.body, input.repo) {
        if covered.contains(&n) {
            continue;
        }
        match issues.kind(n) {
            Ok(IssueKind::Story) => add(
                &mut findings,
                4,
                format!("#{n}"),
                format!("#{n} is not covered by a passing reference; remove the closing keyword"),
            ),
            Ok(_) => {}
            Err(_) => add(
                &mut findings,
                1,
                format!("#{n}"),
                format!("#{n}: issue lookup failed"),
            ),
        }
    }
    (refs, findings)
}

fn link_for(id: &str, flags: &config::Flags) -> Result<(String, u8), Failure> {
    let ctx = crate::context(flags)?;
    let prefix = ctx.config["prefix"].as_str().unwrap_or("ST");
    if !crate::valid_id(id) || id.rsplit_once('-').map(|(p, _)| p) != Some(prefix) {
        return Err("Expected a story ID with the configured repository prefix.".into());
    }
    if ctx.config["tracker"] != "github" || ctx.config["sync"] == "off" {
        return Err(
            "Verified PR links require a configured GitHub tracker with sync enabled.".into(),
        );
    }
    let repo = ctx.config["github"]["repo"]
        .as_str()
        .ok_or("GitHub tracker has no configured repository.")?;
    let story = crate::list::get(&ctx.dir, id)?.ok_or_else(|| format!("{id}: story not found."))?;
    let story = story.to_value();
    if story["id"] != id || !matches!(story["status"].as_str(), Some("ready" | "in-progress")) {
        return Err(format!("{id}: expected a matching ready or in-progress story.").into());
    }
    let issue = card_from(&story, id)
        .remote
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("{id}: publish the story before linking its PR."))?;
    let raw = Cli::gh(&ctx.root)
        .run(
            &["api".into(), format!("repos/{repo}/issues/{issue}")],
            None,
        )
        .map_err(|_| format!("{id}: GitHub issue lookup failed; check tracker access."))?;
    let remote: Value = serde_json::from_str(&raw)
        .map_err(|_| format!("{id}: GitHub returned invalid issue data."))?;
    let story_label = remote["labels"].as_array().is_some_and(|labels| {
        labels.iter().any(|label| {
            label["name"]
                .as_str()
                .is_some_and(|name| STORY_LABELS.contains(&name))
        })
    });
    if remote["number"].as_u64() != Some(issue)
        || remote.get("pull_request").is_some()
        || remote["state"] != "open"
        || !remote["title"]
            .as_str()
            .is_some_and(|title| title.starts_with(&format!("{id}: ")))
        || !story_label
    {
        return Err(format!("{id}: issue #{issue} is not its open mirrored story issue.").into());
    }
    Ok((format!("Stories: {id} (#{issue})\n"), 0))
}

pub(crate) fn run(args: &[String]) -> Result<(String, u8), Failure> {
    let usage = |message: String| Failure { code: 2, message };
    let flags = config::Flags::parse(args)?;
    if let Some(name) = flags.names().iter().find(|n| !FLAGS.contains(&n.as_str())) {
        return Err(usage(format!("Unknown check-pr flag --{name}.")));
    }
    if !flags.positional.is_empty() {
        return Err(usage("check-pr takes no positional arguments.".into()));
    }
    if let Some(id) = flags.value("link-for") {
        if flags.has("body-file")
            || flags.has("json")
            || flags.values("link-for").is_some_and(|ids| ids.len() != 1)
        {
            return Err(usage(
                "Use --link-for with one story ID and no body file or JSON flag.".into(),
            ));
        }
        return link_for(id, &flags);
    }
    let mode = match std::env::var("STORY_ACCEPTANCE")
        .unwrap_or_default()
        .as_str()
    {
        "" | "warn" => Mode::Warn,
        "fail" => Mode::Fail,
        _ => return Err(usage("STORY_ACCEPTANCE must be warn or fail.".into())),
    };
    let body = match flags.value("body-file") {
        Some(path) => {
            fs::read_to_string(path).map_err(|_| usage(format!("Could not read {path}.")))?
        }
        None => std::env::var("PR_BODY").unwrap_or_default(),
    };
    let incident = std::env::var("IS_INCIDENT").as_deref() == Ok("true");
    let repo = std::env::var("REPO").ok().filter(|r| !r.is_empty());
    let repo_name = repo.clone().unwrap_or_default();
    let ctx = crate::context(&flags)?;
    let prefix = ctx.config["prefix"].as_str().unwrap_or("ST").to_string();
    let input = Input {
        body: &body,
        prefix: &prefix,
        repo: &repo_name,
        incident,
        sync_off: ctx.config["sync"] == "off",
    };
    let mut cards = Backlog {
        dir: &ctx.dir,
        all: None,
    };
    let mut issues = GhIssues {
        cli: Cli::gh(&ctx.root),
        repo,
    };
    let (refs, findings) = evaluate(&input, &mut cards, &mut issues);

    let failing = |f: &Finding| f.rule == 1 || mode == Mode::Fail;
    for f in &findings {
        if failing(f) {
            eprintln!("FAIL: {}", f.line);
            eprintln!("::error::{}", f.line);
        } else {
            eprintln!("::warning::{}", f.line);
        }
    }
    let failed = findings.iter().any(failing);
    let warnings = findings.iter().filter(|f| !failing(f)).count();
    let result = if failed {
        "fail"
    } else if warnings > 0 {
        "warn"
    } else {
        "pass"
    };
    let stdout = if flags.has("json") {
        let report = json!({
            "mode": if mode == Mode::Fail { "fail" } else { "warn" },
            "incident": incident,
            "references": refs.iter().map(|r| json!({"id": r.id, "issue": r.issue, "partial": r.partial})).collect::<Vec<_>>(),
            "findings": findings.iter().map(|f| json!({"rule": f.rule, "reference": f.reference, "message": f.line})).collect::<Vec<_>>(),
            "result": result,
        });
        format!("{report}\n")
    } else if failed {
        String::new()
    } else if incident && refs.is_empty() {
        "PASS: incident exception; file the bug story within one business day.\n".into()
    } else if warnings > 0 {
        format!("PASS: every declared story reference exists. Warnings: {warnings}.\n")
    } else {
        "PASS: every declared story reference exists.\n".into()
    };
    Ok((stdout, u8::from(failed)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Fake(HashMap<String, Card>);
    impl Cards for Fake {
        fn by_id(&mut self, id: &str) -> Result<Option<Card>, String> {
            Ok(self.0.get(id).map(|c| Card {
                id: c.id.clone(),
                status: c.status.clone(),
                remote: c.remote,
                criteria: c.criteria.clone(),
            }))
        }
        fn by_remote(&mut self, n: u64) -> Result<Option<Card>, String> {
            let id = self
                .0
                .values()
                .find(|c| c.remote == Some(n))
                .map(|c| c.id.clone());
            match id {
                Some(id) => self.by_id(&id),
                None => Ok(None),
            }
        }
    }
    struct Labels(HashMap<u64, IssueKind>);
    impl IssueSource for Labels {
        fn kind(&mut self, n: u64) -> Result<IssueKind, String> {
            self.0
                .get(&n)
                .copied()
                .ok_or_else(|| "lookup failed".into())
        }
    }

    fn card(id: &str, status: &str, remote: Option<u64>, criteria: &[bool]) -> (String, Card) {
        (
            id.into(),
            Card {
                id: id.into(),
                status: status.into(),
                remote,
                criteria: criteria.to_vec(),
            },
        )
    }

    fn check(body: &str, cards: Vec<(String, Card)>) -> Vec<(u8, String)> {
        let input = Input {
            body,
            prefix: "ST",
            repo: "acme/shop",
            incident: false,
            sync_off: false,
        };
        let mut issues = Labels(HashMap::from([
            (5, IssueKind::Story),
            (6, IssueKind::Story),
            (7, IssueKind::Other),
            (8, IssueKind::PullRequest),
        ]));
        let (_, findings) = evaluate(&input, &mut Fake(cards.into_iter().collect()), &mut issues);
        findings.into_iter().map(|f| (f.rule, f.line)).collect()
    }

    #[test]
    fn a_done_card_with_every_box_ticked_passes_and_covers_its_keyword() {
        let cards = vec![card("ST-001", "done", Some(5), &[true, true])];
        assert!(check("Stories: ST-001 (#5)\nCloses #5", cards).is_empty());
    }

    #[test]
    fn an_unaccepted_card_gives_one_rule_2_finding_in_the_cuj_text() {
        let cards = vec![card(
            "ST-901",
            "in-progress",
            Some(5),
            &[true, false, false],
        )];
        assert_eq!(
            check("Stories: ST-901 (#5)", cards),
            vec![(2, "ST-901 is in-progress with 2 unticked criteria; run story-accept or mark the reference (partial)".into())]
        );
    }

    #[test]
    fn a_partial_reference_skips_acceptance_but_never_covers_a_keyword() {
        let cards = vec![card("ST-904", "in-progress", Some(5), &[false])];
        assert_eq!(
            check("Stories: ST-904 (#5) (partial)\nCloses #5", cards),
            vec![(
                4,
                "#5 is not covered by a passing reference; remove the closing keyword".into()
            )]
        );
    }

    #[test]
    fn keywords_on_non_story_issues_and_pull_requests_pass() {
        let cards = vec![card("ST-001", "done", Some(5), &[true])];
        assert!(check("Stories: ST-001\nFixes #7\nCloses #8", cards).is_empty());
    }

    #[test]
    fn a_bare_issue_uses_the_card_whose_remote_matches() {
        let cards = vec![card("ST-002", "done", Some(6), &[true])];
        assert!(check("Stories: #6", cards).is_empty());
        assert_eq!(
            check("Stories: #5", vec![]),
            vec![(2, "#5: no card has remote 5".into())]
        );
    }

    #[test]
    fn link_consistency_and_zero_criteria_are_findings() {
        let cards = vec![
            card("ST-001", "done", None, &[true]),
            card("ST-002", "done", Some(6), &[true]),
            card("ST-003", "done", Some(5), &[]),
        ];
        assert_eq!(
            check("Stories: ST-001, ST-002 (#5), ST-003", cards),
            vec![
                (
                    3,
                    "ST-001 has no remote; run story push before merge".into()
                ),
                (3, "ST-002 (#5) does not match remote 6".into()),
                (2, "ST-003 has no acceptance criteria".into()),
            ]
        );
    }

    #[test]
    fn rule_1_reports_every_link_failure() {
        assert_eq!(
            check("Stories: ST-009 #7 #8 #9", vec![]),
            vec![
                (1, "ST-009: card not found".into()),
                (1, "#7: not a story issue".into()),
                (1, "#8: is a pull request".into()),
                (1, "#9: issue lookup failed".into()),
            ]
        );
        assert_eq!(check("Related ST-001", vec![])[0].0, 1);
    }
}
