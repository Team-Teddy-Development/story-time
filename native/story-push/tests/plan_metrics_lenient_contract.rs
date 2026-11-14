use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const COLON: &str = "---
id: ST-001
title: Fix: login fails
status: ready
type: story
---

## Story
As a user, I want login.

## Changelog
- 2026-11-01 created
";

const DUPLICATE_KEY: &str = "---
id: ST-002
title: Old title
status: ready
type: story
title: New title
---

## Story
As a user, I want login.

## Changelog
- 2026-11-01 created
";

const PREAMBLE: &str = "---
id: ST-003
title: Login
status: ready
type: story
---
Prose that sits before the first section.

## Story
As a user, I want login.

## Changelog
- 2026-11-01 created
";

struct Backlog(PathBuf);
impl Backlog {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-plan-metrics-lenient-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::write(
            path.join(".story-time/settings.json"),
            r#"{"tracker":"files","prefix":"ST","backlogDir":"backlog"}"#,
        )
        .unwrap();
        for (id, text) in [
            ("ST-001", COLON),
            ("ST-002", DUPLICATE_KEY),
            ("ST-003", PREAMBLE),
        ] {
            fs::write(path.join(format!("backlog/{id}.md")), text).unwrap();
        }
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .args(args)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn json(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?} refused: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}
impl Drop for Backlog {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn plan_snapshot_reads_stories_the_strict_parser_refused() {
    let backlog = Backlog::new();
    let snapshot = backlog.json(&["plan", "--snapshot"]);
    let text = snapshot.to_string();
    for id in ["ST-001", "ST-002", "ST-003"] {
        assert!(
            text.contains(&format!("backlog/{id}.md")),
            "{id} missing: {text}"
        );
    }
}

#[test]
fn metrics_reads_stories_the_strict_parser_refused() {
    let backlog = Backlog::new();
    let report = backlog.json(&[
        "metrics",
        "--from",
        "2026-11-01",
        "--to",
        "2026-11-14",
        "--json",
    ]);
    let text = report.to_string();
    assert!(!text.contains("does not parse"), "{text}");
}
