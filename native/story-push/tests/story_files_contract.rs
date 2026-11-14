use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const STORY: &str = "---
id: ST-001
title: Login
status: ready
type: story
size: 2
---

## Story
As a billing admin, I want login, so that I can pay.

## Acceptance Criteria
- [ ] Given a session, when I log in, then I see the dashboard

## Changelog
- 2026-11-01 created
";

struct Backlog(PathBuf);
impl Backlog {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-files-contract-{}-{}",
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
        fs::write(path.join("backlog/ST-001.md"), STORY).unwrap();
        fs::write(path.join("backlog/EP-001.map.md"), "# EP-001 map\n").unwrap();
        fs::write(path.join("backlog/notes.md"), "scratch notes\n").unwrap();
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
            out.status.code().is_some_and(|c| c <= 1),
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
fn list_lint_stats_and_plan_skip_a_stray_markdown_file_and_still_read_the_story_and_map() {
    let backlog = Backlog::new();

    let list = backlog.json(&["list", "--json"]);
    assert_eq!(list.as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list[0]["id"], "ST-001");

    let lint = backlog.json(&["lint", "--json"]);
    let linted: Vec<&Value> = lint.as_array().unwrap().iter().map(|r| &r["id"]).collect();
    assert_eq!(linted, ["ST-001"], "{lint}");

    let stats = backlog.json(&["stats", "--json"]);
    assert_eq!(stats["total"], 1, "{stats}");

    let snapshot = backlog.json(&["plan", "--snapshot"]);
    let text = snapshot.to_string();
    assert!(text.contains("backlog/ST-001.md"), "{text}");
    assert!(text.contains("backlog/EP-001.map.md"), "{text}");
    assert!(!text.contains("notes.md"), "{text}");
}
