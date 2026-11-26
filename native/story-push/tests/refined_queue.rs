//! ST-211: refined stories come back for their readiness review.

use chrono::{Duration, Utc};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox(PathBuf);

impl Sandbox {
    fn new(team: Value) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "story-refined-queue-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(dir.join(".story-time")).unwrap();
        fs::create_dir_all(dir.join("backlog")).unwrap();
        fs::write(
            dir.join(".story-time/settings.json"),
            json!({"tracker": "files", "sync": "off", "team": team}).to_string(),
        )
        .unwrap();
        Self(dir)
    }

    fn story(&self, id: &str, title: &str, status: &str, updated: Option<i64>, log: &str) {
        let updated = updated.map_or(String::new(), |d| format!("updated: {}\n", ago(d)));
        fs::write(
            self.0.join(format!("backlog/{id}.md")),
            format!(
                "---\nid: {id}\ntitle: {title}\nstatus: {status}\ntype: story\n{updated}---\n\n\
                 ## Story\nAs a team lead, I want x, so that y.\n\n\
                 ## Acceptance Criteria\n- [ ] Given a, when b, then c.\n\n\
                 ## Changelog\n{log}"
            ),
        )
        .unwrap();
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_story"))
            .args(args)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn json(&self, args: &[&str]) -> Value {
        let (_, stdout, stderr) = self.run(args);
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout} {stderr}"))
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ago(days: i64) -> String {
    (Utc::now().date_naive() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

fn refined_on(days: i64) -> String {
    format!(
        "- {} created\n- {} set size=3, status=refined \u{2014} gaps remain\n- {} noted\n",
        ago(days + 5),
        ago(days),
        ago(0)
    )
}

fn backlog(team: Value) -> Sandbox {
    let s = Sandbox::new(team);
    s.story("ST-001", "Ten", "refined", Some(0), &refined_on(10));
    s.story("ST-002", "Thirty", "refined", Some(0), &refined_on(30));
    s.story("ST-003", "Two", "refined", Some(0), &refined_on(2));
    s.story("ST-004", "No event", "refined", Some(20), "- created\n");
    s.story("ST-005", "No date", "refined", None, "");
    s.story(
        "ST-006",
        "Ready",
        "ready",
        Some(0),
        &format!("{}- {} set status=ready\n", refined_on(40), ago(1)),
    );
    s
}

fn stale(lint: &Value, id: &str) -> Vec<String> {
    lint.as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no lint result for {id}"))["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "refined.stale")
        .map(|f| {
            format!(
                "{}: {}",
                f["severity"].as_str().unwrap(),
                f["message"].as_str().unwrap()
            )
        })
        .collect()
}

#[test]
fn rank_lists_the_refined_review_queue_oldest_first_with_ages() {
    let s = backlog(json!({}));
    let (code, text, stderr) = s.run(&["rank"]);
    assert_eq!(code, 0, "{stderr}");
    let queue = text
        .split("Refined review queue")
        .nth(1)
        .unwrap_or_else(|| panic!("no queue in:\n{text}"));
    assert_eq!(
        queue,
        " (oldest first; run story-ready on each):\n\
         \x20 ST-004  age unknown  No event\n\
         \x20 ST-005  age unknown  No date\n\
         \x20 ST-002  30 days  Thirty\n\
         \x20 ST-001  10 days  Ten\n\
         \x20 ST-003  2 days  Two\n"
    );

    let rank = s.json(&["rank", "--json"]);
    assert_eq!(
        rank["refinedQueue"],
        json!([
            {"id": "ST-004", "title": "No event"},
            {"id": "ST-005", "title": "No date"},
            {"id": "ST-002", "title": "Thirty", "refinedSince": ago(30), "days": 30},
            {"id": "ST-001", "title": "Ten", "refinedSince": ago(10), "days": 10},
            {"id": "ST-003", "title": "Two", "refinedSince": ago(2), "days": 2},
        ])
    );
    assert_eq!(rank["ranked"], json!([]));
}

#[test]
fn an_empty_refined_queue_prints_nothing_extra() {
    let s = Sandbox::new(json!({}));
    s.story("ST-001", "Ready", "ready", Some(0), "");
    let (_, text, _) = s.run(&["rank"]);
    assert!(!text.contains("Refined review queue"), "{text}");
    assert_eq!(s.json(&["rank", "--json"])["refinedQueue"], json!([]));
}

#[test]
fn lint_warns_when_a_story_sits_in_refined_past_the_default_limit() {
    let s = backlog(json!({}));
    let lint = s.json(&["lint", "--json"]);
    let message = |days: i64| {
        format!(
            "warning: In refined for {days} days, past the 7-day limit. Run story-ready: promote it to ready or name the remaining gap."
        )
    };
    assert_eq!(stale(&lint, "ST-001"), vec![message(10)]);
    assert_eq!(stale(&lint, "ST-002"), vec![message(30)]);
    assert_eq!(stale(&lint, "ST-004"), Vec::<String>::new());
    assert_eq!(stale(&lint, "ST-003"), Vec::<String>::new());
    assert_eq!(stale(&lint, "ST-005"), Vec::<String>::new());
    assert_eq!(stale(&lint, "ST-006"), Vec::<String>::new());
}

#[test]
fn the_limit_comes_from_team_refined_stale_days() {
    let s = backlog(json!({"refinedStaleDays": 20}));
    let lint = s.json(&["lint", "--json"]);
    assert_eq!(stale(&lint, "ST-001"), Vec::<String>::new());
    assert_eq!(
        stale(&lint, "ST-002"),
        vec!["warning: In refined for 30 days, past the 20-day limit. Run story-ready: promote it to ready or name the remaining gap."]
    );
    let shown = s.json(&["config", "--json"]);
    assert_eq!(shown["team"]["refinedStaleDays"], json!(20));
}

#[test]
fn a_non_positive_limit_fails_config_validation() {
    for bad in [json!(0), json!(-3), json!(2.5), json!("7")] {
        let s = backlog(json!({"refinedStaleDays": bad}));
        let (code, _, stderr) = s.run(&["config", "--json"]);
        assert_ne!(code, 0, "{bad} was accepted");
        assert!(
            stderr.contains("team.refinedStaleDays must be a positive integer."),
            "{bad}: {stderr}"
        );
    }
}

#[test]
fn a_future_entry_date_reads_as_zero_days() {
    let s = Sandbox::new(json!({}));
    s.story(
        "ST-001",
        "Clock skew",
        "refined",
        Some(0),
        &format!("- {} set status=refined\n", ago(-3)),
    );
    assert_eq!(
        s.json(&["rank", "--json"])["refinedQueue"][0]["days"],
        json!(0)
    );
}

#[test]
fn mcp_story_lint_reads_the_limit_from_settings() {
    let s = backlog(json!({"refinedStaleDays": 20}));
    let mut child = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["mcp", "--root", s.0.to_str().unwrap()])
        .env_clear()
        .env("PATH", "/nonexistent")
        .env("HOME", &s.0)
        .current_dir(&s.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "name": "story_lint",
        "arguments": {},
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "refined-queue-test", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }
    }});
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let _ = child.kill();
    let _ = child.wait();
    let response: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
    let results = &response["result"]["structuredContent"]["results"];
    assert_eq!(stale(results, "ST-001"), Vec::<String>::new(), "{response}");
    assert_eq!(
        stale(results, "ST-002"),
        vec!["warning: In refined for 30 days, past the 20-day limit. Run story-ready: promote it to ready or name the remaining gap."]
    );
}

fn skill(name: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills")
        .join(name)
        .join("SKILL.md");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_skills_take_the_refined_queue_first() {
    let ready = skill("story-ready");
    let queue = ready
        .find("refined review queue")
        .expect("story-ready names the refined review queue");
    let decide = ready.find("## 3. Decide").expect("story-ready decide step");
    assert!(
        queue < decide,
        "story-ready reads the queue before deciding"
    );
    assert!(
        ready.contains("story rank --json"),
        "story-ready names the command"
    );
    assert!(
        ready.contains("name the remaining gap"),
        "story-ready ends each review with ready or a named gap"
    );

    let refinement = skill("refinement");
    let queue = refinement
        .find("refined review queue")
        .expect("refinement names the refined review queue");
    let review = refinement
        .find("Review stale requests")
        .expect("refinement reviews the inventory");
    assert!(
        queue < review,
        "refinement takes the refined queue before the wider review"
    );
    assert!(
        ready.contains("restarts its clock") && refinement.contains("story-ready"),
        "a re-review that fails records the gap"
    );

    let router = skill("story-time");
    let lifecycle = router.find("`draft` → `refined` → `ready`").unwrap();
    assert!(
        router[lifecycle..].contains("refined review queue"),
        "story-time lifecycle names the refined review queue"
    );
}
