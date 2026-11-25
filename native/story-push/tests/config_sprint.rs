//! ST-194: `team.sprintAnchor` and `team.sprints` in settings.

use serde_json::{json, Value};
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn config(team: Value) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!(
        "story-config-sprint-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(dir.join(".story-time")).unwrap();
    fs::write(
        dir.join(".story-time/settings.json"),
        json!({"tracker": "files", "sync": "off", "team": team}).to_string(),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["config", "--json"])
        .env_clear()
        .env("PATH", "/nonexistent")
        .env("HOME", &dir)
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&dir);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn refused(team: Value, needle: &str) {
    let (ok, stderr, _) = config(team.clone());
    assert!(!ok, "{team} was accepted");
    assert!(stderr.contains(needle), "{team}: {stderr}");
}

#[test]
fn a_weekday_anchor_with_a_zone_and_a_sprints_list_are_accepted_and_kept() {
    let team = json!({
        "sprintDays": 10,
        "sprintAnchor": {"date": "2026-01-05", "tz": "America/Chicago"},
        "sprints": [{"id": "2026-S02", "start": "2026-01-19", "end": "2026-01-23", "goal": "Short"}]
    });
    let (ok, stderr, shown) = config(team);
    assert!(ok, "{stderr}");
    let shown: Value = serde_json::from_str(&shown).unwrap();
    assert_eq!(
        shown["team"]["sprintAnchor"],
        json!({"date": "2026-01-05", "tz": "America/Chicago"})
    );
    assert_eq!(
        shown["team"]["sprints"],
        json!([{"id": "2026-S02", "start": "2026-01-19", "end": "2026-01-23", "goal": "Short"}])
    );
}

#[test]
fn settings_without_a_calendar_read_as_before() {
    let (ok, stderr, shown) = config(json!({"sprintDays": 10}));
    assert!(ok, "{stderr}");
    assert!(
        !shown.contains("sprintAnchor") && !shown.contains("sprints"),
        "{shown}"
    );
}

#[test]
fn a_bad_anchor_is_refused_naming_the_field() {
    refused(
        json!({"sprintAnchor": {"tz": "UTC"}}),
        "team.sprintAnchor.date",
    );
    refused(
        json!({"sprintAnchor": {"date": "2026-02-30", "tz": "UTC"}}),
        "team.sprintAnchor.date",
    );
    refused(
        json!({"sprintAnchor": {"date": "2026-01-03", "tz": "UTC"}}),
        "must be a weekday",
    );
    refused(
        json!({"sprintAnchor": {"date": "2026-01-05"}}),
        "team.sprintAnchor.tz",
    );
    refused(
        json!({"sprintAnchor": {"date": "2026-01-05", "tz": "chicago"}}),
        "team.sprintAnchor.tz",
    );
    refused(
        json!({"sprintAnchor": "2026-01-05"}),
        "team.sprintAnchor.date",
    );
}

#[test]
fn a_bad_sprints_list_is_refused_naming_the_entry() {
    let entry = |id: &str, start: &str, end: &str| json!({"id": id, "start": start, "end": end});
    refused(
        json!({"sprints": "2026-S01"}),
        "team.sprints must be a list",
    );
    refused(
        json!({"sprints": [entry("2026-1", "2026-01-05", "2026-01-16")]}),
        "team.sprints[0].id",
    );
    refused(
        json!({"sprints": [entry("2026-S01", "soon", "2026-01-16")]}),
        "team.sprints[0].start",
    );
    refused(
        json!({"sprints": [entry("2026-S01", "2026-01-05", "2026-13-01")]}),
        "team.sprints[0].end",
    );
    refused(
        json!({"sprints": [entry("2026-S01", "2026-01-16", "2026-01-05")]}),
        "team.sprints[0].end is before",
    );
    refused(
        json!({"sprints": [entry("2026-S01", "2026-01-05", "2026-01-16"), entry("2026-S01", "2026-02-02", "2026-02-13")]}),
        "team.sprints[1].id repeats",
    );
    refused(
        json!({"sprints": [{"id": "2026-S01", "start": "2026-01-05", "end": "2026-01-16", "goal": 5}]}),
        "team.sprints[0].goal",
    );
}
