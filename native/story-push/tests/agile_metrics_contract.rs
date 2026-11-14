use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = "changelog `set status=` lines in the backlog story files, day resolution";

struct Backlog(PathBuf);
impl Backlog {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-metrics-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        Self(path)
    }
    fn story(&self, id: &str, status: &str, changelog: &[&str]) -> &Self {
        let log = changelog
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            self.0.join("backlog").join(format!("{id}.md")),
            format!("---\nid: {id}\ntitle: {id}\nstatus: {status}\ntype: story\ncreated: 2026-10-01\n---\n\n## Story\nAs a user, I want {id}.\n\n## Changelog\n- 2026-10-01 created\n{log}\n"),
        )
        .unwrap();
        self
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .arg("metrics")
            .args(args)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn json(&self, from: &str, to: &str) -> Value {
        let out = self.run(&["--from", from, "--to", to, "--json"]);
        assert!(
            out.status.success(),
            "{}",
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

/// One gap: ST-003 reached done with no in-progress event, so its cycle time is unknown.
fn one_gap() -> Backlog {
    let backlog = Backlog::new();
    backlog
        .story(
            "ST-001",
            "done",
            &[
                "2026-11-02 set status=in-progress",
                "2026-11-05 set status=done — accepted",
            ],
        )
        .story(
            "ST-002",
            "done",
            &[
                "2026-11-03 set size=2, status=in-progress",
                "2026-11-04 set status=done",
            ],
        )
        .story(
            "ST-003",
            "done",
            &[
                "2026-11-01 set status=ready",
                "2026-11-06 set status=done — note says status=in-progress",
            ],
        )
        .story(
            "ST-004",
            "in-progress",
            &["2026-11-06 set status=in-progress"],
        )
        .story(
            "ST-005",
            "done",
            &[
                "2026-10-15 set status=in-progress",
                "2026-10-20 set status=done",
            ],
        )
        .story("ST-006", "draft", &[]);
    fs::write(backlog.0.join("backlog/EP-001.map.md"), "# not a story\n").unwrap();
    backlog
}

#[test]
fn every_figure_names_its_source_window_and_units_and_the_gap_stays_unknown() {
    let window = "2026-11-01..2026-11-07";
    assert_eq!(
        one_gap().json("2026-11-01", "2026-11-07"),
        json!({
            "window": window,
            "source": SOURCE,
            "figures": [
                {
                    "metric": "throughput",
                    "value": 3,
                    "units": "stories",
                    "window": window,
                    "source": SOURCE,
                    "definition": "stories with a status=done event dated in the window",
                    "stories": ["ST-001", "ST-002", "ST-003"],
                    "unknown": []
                },
                {
                    "metric": "wip",
                    "value": 1,
                    "units": "stories",
                    "window": window,
                    "source": SOURCE,
                    "definition": "stories whose last status event on or before the window end is in-progress",
                    "stories": ["ST-004"],
                    "unknown": []
                },
                {
                    "metric": "cycle_time_median",
                    "value": 2,
                    "units": "days",
                    "window": window,
                    "source": SOURCE,
                    "definition": "median days from the first in-progress event to the done event, over stories done in the window with both events recorded",
                    "per_story": [
                        {"id": "ST-001", "days": 3},
                        {"id": "ST-002", "days": 1},
                        {"id": "ST-003", "days": "unknown"}
                    ],
                    "unknown": [{"id": "ST-003", "reason": "done with no earlier in-progress event"}]
                },
                {
                    "metric": "velocity",
                    "value": "unknown",
                    "units": "points per sprint",
                    "window": window,
                    "source": "no sprint record is stored",
                    "definition": "completed points per sprint",
                    "unknown": [{"id": null, "reason": "no sprint record is stored"}]
                }
            ]
        })
    );
}

#[test]
fn a_status_with_no_matching_changelog_event_makes_that_story_unknown_everywhere() {
    let backlog = Backlog::new();
    backlog
        .story(
            "ST-001",
            "in-progress",
            &["2026-11-02 set status=in-progress"],
        )
        .story("ST-002", "done", &["2026-11-01 set status=ready"]);
    let report = backlog.json("2026-11-01", "2026-11-07");
    let reason = json!([{"id": "ST-002", "reason": "status done has no matching changelog event"}]);
    for figure in &report["figures"].as_array().unwrap()[..3] {
        assert_eq!(figure["unknown"], reason, "{figure}");
    }
    assert_eq!(report["figures"][0]["value"], 0);
    assert_eq!(report["figures"][1]["value"], 1);
    assert_eq!(report["figures"][2]["value"], "unknown");
}

#[test]
fn the_text_report_names_source_window_and_units_on_every_figure_line() {
    let out = one_gap().run(&["--from", "2026-11-01", "--to", "2026-11-07"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    for (metric, value) in [
        ("throughput", "3 stories"),
        ("wip", "1 stories"),
        ("cycle_time_median", "2 days"),
        ("velocity", "unknown points per sprint"),
    ] {
        let line = text
            .lines()
            .find(|line| line.starts_with(&format!("{metric}: ")))
            .unwrap_or_else(|| panic!("no {metric} line:\n{text}"));
        assert!(line.contains(value), "{line}");
        assert!(line.contains("window 2026-11-01..2026-11-07"), "{line}");
        assert!(line.contains("source "), "{line}");
    }
    assert!(
        text.contains("ST-003 unknown: done with no earlier in-progress event"),
        "{text}"
    );
}

#[test]
fn the_window_must_be_two_ordered_dates() {
    let backlog = one_gap();
    for args in [
        vec!["--to", "2026-11-07"],
        vec!["--from", "2026-11-01"],
        vec!["--from", "2026-11-08", "--to", "2026-11-07"],
        vec!["--from", "11/01/2026", "--to", "2026-11-07"],
    ] {
        let out = backlog.run(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
}
