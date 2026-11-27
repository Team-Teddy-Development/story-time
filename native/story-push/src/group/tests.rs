//! Contract tests for `story group` against a local fake of the Jev API.

use super::{group, Env};
use crate::judge::tests::{answer_for, Fake, Reply, FAST, KEY};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const TAXONOMY: &str = r#"{"epics":[
  {"name":"Core cutover","description":"Cut varsity-engine callers over to the Core API","features":[
    {"name":"Engine call cutovers","description":"Cut one engine module over to Core API calls"},
    {"name":"Bypass retirement","description":"Remove bypass credentials"}]},
  {"name":"Platform & infrastructure","description":"Hosting EC2 database roles","features":[
    {"name":"Core runtime hosting","description":"EC2 OS roles"}]}]}"#;

struct Project(PathBuf);
impl Project {
    fn new(taxonomy: Option<&str>) -> Self {
        let root = std::env::temp_dir().join(format!(
            "story-group-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("backlog")).unwrap();
        fs::create_dir_all(root.join(".story-time")).unwrap();
        fs::write(root.join(".story-time/settings.json"), "{}\n").unwrap();
        if let Some(t) = taxonomy {
            fs::write(root.join(".story-time/taxonomy.json"), t).unwrap();
        }
        let story = |id: &str, title: &str, status: &str, epic: &str| {
            fs::write(
                root.join(format!("backlog/{id}.md")),
                format!("---\nid: {id}\ntitle: {title}\nstatus: {status}\ntype: story\n{epic}---\n\n## Story\nAs a maintainer, I want {title}, so that Core owns it.\n\n## Acceptance Criteria\n- [ ] Given x, when y, then z\n\n## Changelog\n- 2026-11-01 created\n"),
            )
            .unwrap();
        };
        story(
            "VC-073",
            "Cut varsity-engine results calls over to the Core API",
            "refined",
            "epic: VC-CORE\n",
        );
        story(
            "VC-074",
            "Cut varsity-engine media calls over to the Core API",
            "draft",
            "",
        );
        story("VC-075", "Done work", "done", "epic: VC-CORE\n");
        Self(root.canonicalize().unwrap())
    }
    fn args(&self, extra: &[&str]) -> Vec<String> {
        extra
            .iter()
            .map(|s| s.to_string())
            .chain([
                "--config".into(),
                self.0
                    .join(".story-time/settings.json")
                    .display()
                    .to_string(),
            ])
            .chain(["--dir".into(), self.0.join("backlog").display().to_string()])
            .collect()
    }
    fn read(&self, id: &str) -> String {
        fs::read_to_string(self.0.join(format!("backlog/{id}.md"))).unwrap()
    }
    fn run(&self, fake: Option<&Fake>, extra: &[&str]) -> Result<String, String> {
        let env = Env {
            key: fake.map(|_| KEY.into()),
            switch: None,
            anthropic: None,
        };
        let url = fake
            .map(|f| f.url.clone())
            .unwrap_or_else(|| "http://127.0.0.1:9/unused".into());
        group(&self.args(extra), &env, &url, FAST)
            .map(|(out, _)| out)
            .map_err(|f| f.message)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Every choice question answered with its first option at 0.8, the canary answered truly.
fn first_options(request: &Value, _: usize) -> Reply {
    let answers: serde_json::Map<String, Value> = request["questions"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, q)| (id.clone(), answer_for(id, q)))
        .collect();
    (200, json!({"model":"jev-1.14.0","answers":answers,"usage":{"input_tokens":300,"output_tokens":9}}).to_string(), vec![])
}

#[test]
fn dry_run_writes_nothing() {
    let p = Project::new(Some(TAXONOMY));
    let fake = Fake::new(first_options);
    let before: Vec<String> = ["VC-073", "VC-074", "VC-075"]
        .iter()
        .map(|id| p.read(id))
        .collect();
    let out = p.run(Some(&fake), &[]).unwrap();
    assert!(out.starts_with("engine: jev (jev-1.13.0)\n"), "{out}");
    assert!(out.contains("VC-073  Cut varsity-engine results calls over to the Core API\n  VC-CORE → Core cutover / Engine call cutovers  (epic 0.8, feature 0.8, jev-1.14.0)\n"), "{out}");
    assert!(out.contains("VC-074"), "{out}");
    assert!(
        !out.contains("VC-075"),
        "done stories are not grouped: {out}"
    );
    assert!(out.ends_with("2 assigned, 0 unchanged, 0 unresolved, 0 kept. Dry run: rerun with --apply to write.\n"), "{out}");
    let after: Vec<String> = ["VC-073", "VC-074", "VC-075"]
        .iter()
        .map(|id| p.read(id))
        .collect();
    assert_eq!(before, after);
    let calls: Vec<Value> = fake
        .requests()
        .iter()
        .filter(|r| !r.is_canary())
        .map(|r| r.json())
        .collect();
    assert_eq!(calls.len(), 2, "one batched request per story");
    let q = calls[0]["questions"].as_object().unwrap();
    assert_eq!(
        q.keys().collect::<Vec<_>>(),
        ["epic", "feature_1", "feature_2"]
    );
    assert!(calls[0]["state"].get("id").is_none());
}

#[test]
fn apply_writes_names_and_the_changelog_and_json_reports_each_placement() {
    let p = Project::new(Some(TAXONOMY));
    let fake = Fake::new(first_options);
    let out = p.run(Some(&fake), &["--apply", "--json"]).unwrap();
    let report: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(report["engine"]["mode"], "jev");
    assert_eq!(
        report["summary"],
        json!({"assigned":2,"unchanged":0,"unresolved":0,"kept":0,"written":["VC-073","VC-074"]})
    );
    let first = &report["placements"][0];
    assert_eq!(first["id"], "VC-073");
    assert_eq!(first["from"], json!({"epic":"VC-CORE","feature":null}));
    assert_eq!(
        first["epic"],
        json!({"name":"Core cutover","confidence":0.8})
    );
    assert_eq!(
        first["feature"],
        json!({"name":"Engine call cutovers","confidence":0.8})
    );
    assert_eq!(first["source"], json!({"mode":"jev","model":"jev-1.14.0"}));
    assert_eq!(first["outcome"], "assigned");
    let text = p.read("VC-073");
    assert!(
        text.contains("\nepic: Core cutover\nfeature: Engine call cutovers\n"),
        "{text}"
    );
    assert!(text.contains("set epic=Core cutover, feature=Engine call cutovers — grouped by jev-1.14.0 (epic 0.8, feature 0.8)"), "{text}");
    let again = p.run(Some(&fake), &[]).unwrap();
    assert!(again.ends_with("0 assigned, 0 unchanged, 0 unresolved, 2 kept. Dry run: rerun with --apply to write.\n"), "{again}");
}

#[test]
fn without_a_key_keywords_suggest_below_the_threshold_and_nothing_is_written() {
    let p = Project::new(Some(TAXONOMY));
    let before = p.read("VC-073");
    let out = p.run(None, &["--apply"]).unwrap();
    assert!(
        out.starts_with("engine: code (no TypeSafe key; run story secret set jev to use Jev)\n"),
        "{out}"
    );
    assert!(out.contains("unresolved: epic confidence"), "{out}");
    assert!(
        out.contains("suggests Core cutover / Engine call cutovers"),
        "{out}"
    );
    assert_eq!(p.read("VC-073"), before);
    let lowered = p
        .run(None, &["VC-073", "--min-confidence", "0.3", "--apply"])
        .unwrap();
    assert!(lowered.contains("1 assigned"), "{lowered}");
    assert!(p.read("VC-073").contains("grouped by code keywords"));
}

#[test]
fn a_missing_or_invalid_taxonomy_refuses_and_writes_nothing() {
    let p = Project::new(None);
    let e = p.run(None, &[]).unwrap_err();
    assert!(e.starts_with("No taxonomy at "), "{e}");
    assert!(e.contains("docs/designs/epic-feature-grouping.md"), "{e}");
    let p = Project::new(Some(r#"{"epics":[]}"#));
    assert_eq!(
        p.run(None, &["--apply"]).unwrap_err(),
        "taxonomy: needs 1 to 15 epics"
    );
    let bad = Project::new(Some(TAXONOMY));
    assert_eq!(
        bad.run(None, &["--min-confidence", "2"]).unwrap_err(),
        "--min-confidence must be a number from 0 to 1, got \"2\"."
    );
}

#[test]
fn none_fit_is_reported_unresolved_and_left_alone() {
    let p = Project::new(Some(TAXONOMY));
    let fake = Fake::new(|request: &Value, n| {
        let mut reply = first_options(request, n);
        if !request["questions"]
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.starts_with("canary_"))
        {
            let mut body: Value = serde_json::from_str(&reply.1).unwrap();
            body["answers"]["epic"] = json!({"type":"choice","choice":"none_fit","confidence":0.93,"probabilities":{"none_fit":0.93}});
            reply.1 = body.to_string();
        }
        reply
    });
    let before = p.read("VC-073");
    let out = p.run(Some(&fake), &["VC-073", "--apply"]).unwrap();
    assert!(
        out.contains("VC-CORE → unresolved: no taxonomy epic fits"),
        "{out}"
    );
    assert_eq!(p.read("VC-073"), before);
}

#[test]
fn sync_fields_needs_a_github_tracker_and_names_what_it_will_create() {
    let p = Project::new(Some(TAXONOMY));
    assert_eq!(
        p.run(None, &["--sync-fields"]).unwrap_err(),
        "story group --sync-fields mirrors to GitHub organization issue fields; set tracker to github first."
    );
}

#[test]
fn sync_fields_report_lists_each_change() {
    use crate::github_fields::Change;
    let out = super::report::sync(
        "Varsity-Engine",
        &[
            Change::Created {
                field: "Epic".into(),
                options: vec!["Core cutover".into(), "Recruit portal".into()],
            },
            Change::Extended {
                field: "Feature".into(),
                added: vec!["Engine call cutovers".into()],
            },
        ],
    );
    assert_eq!(
        out,
        "Varsity-Engine issue fields:\n  Epic: created with Core cutover, Recruit portal\n  Feature: added Engine call cutovers\n"
    );
    let same = super::report::sync(
        "o",
        &[Change::Unchanged {
            field: "Epic".into(),
        }],
    );
    assert_eq!(same, "o issue fields:\n  Epic: unchanged\n");
}

// ---- story taxonomy propose ----

use super::{taxonomy_cmd, Endpoints};

impl Project {
    fn add(&self, id: &str, title: &str, frontmatter: &str) {
        fs::write(
            self.0.join(format!("backlog/{id}.md")),
            format!("---\nid: {id}\ntitle: {title}\nstatus: refined\ntype: story\n{frontmatter}---\n\n## Story\nAs a program admin, I want to {title}.\n\n## Acceptance Criteria\n- [ ] Given x, when y, then z\n\n## Changelog\n- 2026-11-01 created\n"),
        )
        .unwrap();
    }
    fn taxonomy_run(
        &self,
        jev: &Fake,
        anthropic: Option<&Fake>,
        extra: &[&str],
    ) -> Result<String, String> {
        let env = Env {
            key: Some(KEY.into()),
            switch: None,
            anthropic: anthropic.map(|_| "sk-ant-test".into()),
        };
        let anthropic_url = anthropic
            .map(|f| f.url.clone())
            .unwrap_or_else(|| "http://127.0.0.1:9/unused".into());
        let endpoints = Endpoints {
            jev: &jev.url,
            anthropic: &anthropic_url,
        };
        let mut args = vec!["propose"];
        args.extend_from_slice(extra);
        taxonomy_cmd(&self.args(&args), &env, endpoints, FAST)
            .map(|(out, _)| out)
            .map_err(|f| f.message)
    }
    fn taxonomy_text(&self) -> String {
        fs::read_to_string(self.0.join(".story-time/taxonomy.json")).unwrap()
    }
}

/// A Jev that answers by story title: the option whose text starts with the epic or feature
/// name for that title, `none_fit` when the question offers no such option.
fn by_title(request: &Value, _: usize) -> Reply {
    let questions = request["questions"].as_object().unwrap();
    if questions.keys().any(|k| k.starts_with("canary_")) {
        return first_options(request, 0);
    }
    let title = request["state"]["title"].as_str().unwrap_or("");
    let (epic, feature) = if title.contains("guardian") {
        ("Athletes & privacy", "Guardian claim review")
    } else if title.contains("results") || title.contains("Keep") {
        ("Core cutover", "Engine call cutovers")
    } else {
        ("-", "-")
    };
    let answers: serde_json::Map<String, Value> = questions
        .iter()
        .map(|(id, q)| {
            let want = if id == "epic" { epic } else { feature };
            let key = q["criteria"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, text)| text.as_str().unwrap().starts_with(&format!("{want}:")))
                .map(|(k, _)| k.clone())
                .unwrap_or("none_fit".into());
            (
                id.clone(),
                json!({"type":"choice","choice":key,"confidence":0.93,"probabilities":{}}),
            )
        })
        .collect();
    (200, json!({"model":"jev-1.14.0","answers":answers,"usage":{"input_tokens":300,"output_tokens":9}}).to_string(), vec![])
}

fn guardian_project() -> Project {
    let p = Project::new(Some(TAXONOMY));
    p.add("VC-301", "review guardian claims before approval", "");
    p.add("VC-302", "escalate disputed guardian claims", "");
    p.add(
        "VC-076",
        "Keep engine calls on Core",
        "epic: Core cutover\nfeature: Engine call cutovers\n",
    );
    p
}

const PROPOSALS: &str = r#"{"proposals":[
  {"epic":"Athletes & privacy","epic_description":"Athlete profiles, guardians and personal data","feature":"Guardian claim review","feature_description":"Review, approve and escalate guardian claims","stories":["VC-301","VC-302"]},
  {"epic":"Core cutover","feature":"Media cutovers","feature_description":"Move media calls onto Core","stories":["VC-074"]}]}"#;

#[test]
fn taxonomy_propose_dry_run_reports_each_verdict_and_writes_nothing() {
    let p = guardian_project();
    let file = p.0.join("proposals.json");
    fs::write(&file, PROPOSALS).unwrap();
    let before = p.taxonomy_text();
    let fake = Fake::new(by_title);
    let out = p
        .taxonomy_run(
            &fake,
            None,
            &["--names-from", file.to_str().unwrap(), "--min-gaps", "2"],
        )
        .unwrap();
    assert!(
        out.starts_with("engine: jev (jev-1.13.0)\nnamer: file\n3 uncovered stories.\n"),
        "{out}"
    );
    assert!(out.contains("\naccepted  Athletes & privacy / Guardian claim review (new epic)\n  covers VC-301, VC-302\n"), "{out}");
    assert!(out.contains("\nrejected  Core cutover / Media cutovers: Jev placed 0 of 1 claimed stories on it; at least 2 are needed\n"), "{out}");
    assert!(out.ends_with("1 accepted, 1 rejected. Dry run: rerun with --write to add the accepted names to taxonomy.json.\n"), "{out}");
    assert_eq!(p.taxonomy_text(), before);
}

#[test]
fn taxonomy_propose_write_adds_only_the_accepted_names_and_json_reports_them() {
    let p = guardian_project();
    let file = p.0.join("proposals.json");
    fs::write(&file, PROPOSALS).unwrap();
    let fake = Fake::new(by_title);
    let out = p
        .taxonomy_run(
            &fake,
            None,
            &[
                "--names-from",
                file.to_str().unwrap(),
                "--min-gaps",
                "2",
                "--write",
                "--json",
            ],
        )
        .unwrap();
    let report: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(report["wrote"], true);
    assert_eq!(report["uncovered"], 3);
    assert_eq!(report["proposals"][0]["accepted"], true);
    assert_eq!(report["proposals"][0]["new_epic"], true);
    assert_eq!(
        report["proposals"][0]["captured"],
        json!(["VC-301", "VC-302"])
    );
    assert_eq!(report["proposals"][1]["accepted"], false);
    let t = super::taxonomy::Taxonomy::parse(&p.taxonomy_text()).unwrap();
    let names: Vec<&str> = t.epics.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Core cutover",
            "Platform & infrastructure",
            "Athletes & privacy"
        ]
    );
    assert_eq!(
        t.epic("Core cutover").unwrap().features.len(),
        2,
        "the rejected Media cutovers was not added"
    );
    let grouped: Value =
        serde_json::from_str(&p.run(Some(&fake), &["VC-301", "--json"]).unwrap()).unwrap();
    assert_eq!(
        grouped["placements"][0]["feature"]["name"], "Guardian claim review",
        "story group now places the story on the new name"
    );
    assert_eq!(grouped["placements"][0]["outcome"], "assigned");
}

#[test]
fn taxonomy_propose_with_too_few_uncovered_stories_calls_no_model() {
    let p = guardian_project();
    let fake = Fake::new(by_title);
    let out = p.taxonomy_run(&fake, None, &[]).unwrap();
    assert!(
        out.ends_with(
            "3 uncovered stories; at least 5 are needed to propose names. Nothing to do.\n"
        ),
        "{out}"
    );
}

#[test]
fn taxonomy_propose_asks_claude_and_proves_its_names_with_jev() {
    let p = guardian_project();
    let jev = Fake::new(by_title);
    let claude = Fake::new(|request: &Value, _| {
        let task = request["messages"][0]["content"].as_str().unwrap();
        let stories: Value = serde_json::from_str(&task[task.find('{').unwrap()..]).unwrap();
        let keys: Vec<String> = stories["stories"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["title"].as_str().unwrap().contains("guardian"))
            .map(|s| s["key"].as_str().unwrap().to_owned())
            .collect();
        let text = json!({"proposals":[{"epic":"Athletes & privacy","epic_description":"Athlete profiles, guardians and personal data","feature":"Guardian claim review","feature_description":"Review guardian claims","stories":keys}]}).to_string();
        (200, json!({"content":[{"type":"text","text":text}],"stop_reason":"end_turn","model":"claude-opus-5-5"}).to_string(), vec![])
    });
    let out = p
        .taxonomy_run(&jev, Some(&claude), &["--min-gaps", "2", "--write"])
        .unwrap();
    assert!(out.contains("namer: claude-opus-5-5\n"), "{out}");
    assert!(
        out.contains("accepted  Athletes & privacy / Guardian claim review (new epic)"),
        "{out}"
    );
    assert!(out.ends_with("Wrote 1 name to taxonomy.json. Review it in a pull request, then run story group --sync-fields and story group --apply.\n"), "{out}");
    assert_eq!(claude.requests().len(), 1, "one Messages API request");
    assert!(
        !claude.requests()[0].body.contains("VC-30"),
        "no ids sent to Claude"
    );
}

#[test]
fn taxonomy_propose_without_a_namer_says_how_to_add_one() {
    let p = guardian_project();
    let fake = Fake::new(by_title);
    assert_eq!(
        p.taxonomy_run(&fake, None, &["--min-gaps", "2"]).unwrap_err(),
        "No Anthropic key: set ANTHROPIC_API_KEY (or .story-time/.anthropic.env, mode 600), or pass --names-from proposals.json."
    );
    assert_eq!(
        p.taxonomy_run(&fake, None, &["--max", "0"]).unwrap_err(),
        "--max must be a whole number of 1 or more, got \"0\"."
    );
}
