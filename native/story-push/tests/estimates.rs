use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-estimates-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::write(
            path.join(".story-time/settings.json"),
            r#"{"tracker":"files","sync":"off","team":{"sprintDays":10,"wipLimit":1}}"#,
        )
        .unwrap();
        Self(path)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.0.join("backlog").join(format!("{id}.md"))
    }

    /// A story whose status is the last `status=` in its changelog, as `story set` leaves it.
    fn story(&self, id: &str, front: &str, changelog: &[&str]) -> &Self {
        let status = changelog
            .iter()
            .rev()
            .find_map(|l| l.split("status=").nth(1))
            .map(|s| s.split([' ', ',']).next().unwrap())
            .unwrap_or("draft");
        let log: String = changelog.iter().map(|l| format!("- {l}\n")).collect();
        fs::write(
            self.path(id),
            format!("---\nid: {id}\ntitle: {id}\nstatus: {status}\ntype: story\n{front}created: 2026-11-01\n---\n\n## Story\nAs a team lead, I want {id}, so that it ships.\n\n## Acceptance Criteria\n- [ ] Given a story, when it runs, then it works\n\n## Changelog\n- 2026-11-01 created\n{log}"),
        )
        .unwrap();
        self
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

    fn estimates(&self) -> Value {
        let out = self.run(&["estimates", "--json"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn row<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no row for {id}: {report}"))
}

fn group<'a>(report: &'a Value, size: u64, built_by: &str) -> &'a Value {
    report["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["size"] == size && g["built_by"] == built_by)
        .unwrap_or_else(|| panic!("no group for size {size} and {built_by}: {report}"))
}

/// Done stories of one size and builder, taking 1, 2, ... `days` days from in-progress to done.
fn finished(p: &Project, first: usize, count: usize, size: u32, built_by: &str) {
    for i in 0..count {
        let id = format!("ST-{:03}", first + i);
        let done = format!("2026-10-{:02} set status=done", 1 + i + 1);
        p.story(
            &id,
            &format!("built_by: {built_by}\n"),
            &[
                &format!("2026-09-30 set size={size}"),
                "2026-10-01 set status=in-progress",
                &done,
            ],
        );
    }
}

#[test]
fn a_delivered_story_row_has_estimate_start_done_days_builder_and_pr() {
    let p = Project::new();
    p.story(
        "ST-001",
        "",
        &[
            "2026-11-02 set size=3",
            "2026-11-03 set status=in-progress",
            "2026-11-07 set status=done \u{2014} delivered: PR #12 (abc1234)",
        ],
    );
    assert_eq!(
        row(&p.estimates(), "ST-001"),
        &json!({"id": "ST-001", "estimate": 3, "start": "2026-11-03", "start_event": "in-progress", "done": "2026-11-07", "actual_days": 4, "built_by": "unknown", "pr": 12})
    );
}

#[test]
fn a_story_with_no_in_progress_event_starts_at_ready() {
    let p = Project::new();
    p.story(
        "ST-001",
        "",
        &[
            "2026-10-30 set size=2",
            "2026-11-01 set status=ready",
            "2026-11-05 set status=done",
        ],
    );
    let report = p.estimates();
    let r = row(&report, "ST-001");
    assert_eq!(r["start_event"], "ready");
    assert_eq!(r["start"], "2026-11-01");
    assert_eq!(r["actual_days"], 4);
    assert_eq!(r["pr"], Value::Null);
}

#[test]
fn a_resize_after_work_starts_keeps_the_estimate_made_before_it() {
    let p = Project::new();
    p.story(
        "ST-001",
        "",
        &[
            "2026-11-02 set size=2",
            "2026-11-03 set status=in-progress",
            "2026-11-04 set size=5",
            "2026-11-06 set status=done",
        ],
    );
    let report = p.estimates();
    assert_eq!(row(&report, "ST-001")["estimate"], 2);
    assert_eq!(row(&report, "ST-001")["actual_days"], 3);
}

#[test]
fn only_done_stories_are_rows() {
    let p = Project::new();
    p.story(
        "ST-001",
        "",
        &["2026-11-02 set size=3", "2026-11-03 set status=in-progress"],
    );
    p.story(
        "ST-002",
        "",
        &[
            "2026-11-02 set size=3, status=ready",
            "2026-11-04 set status=done",
        ],
    );
    let report = p.estimates();
    let ids: Vec<&Value> = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["id"])
        .collect();
    assert_eq!(ids, vec!["ST-002"]);
}

#[test]
fn set_built_by_robot_exits_1_and_leaves_the_file_unchanged() {
    let p = Project::new();
    p.story("ST-001", "", &["2026-11-02 set size=3"]);
    let before = fs::read(p.path("ST-001")).unwrap();
    let out = p.run(&["set", "ST-001", "built_by=robot"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("built_by must be factory or person"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fs::read(p.path("ST-001")).unwrap(), before);
}

#[test]
fn set_built_by_factory_is_accepted_and_lints_clean() {
    let p = Project::new();
    p.story("ST-001", "value: 3\n", &["2026-11-02 set size=3"]);
    for kind in ["factory", "person"] {
        let out = p.run(&["set", "ST-001", &format!("built_by={kind}")]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = fs::read_to_string(p.path("ST-001")).unwrap();
        assert!(text.contains(&format!("built_by: {kind}")), "{text}");
        let lint = p.run(&["lint", "ST-001", "--json"]);
        let findings = String::from_utf8_lossy(&lint.stdout);
        assert!(!findings.contains("built_by"), "{findings}");
    }
}

#[test]
fn lint_reports_an_invalid_built_by() {
    let p = Project::new();
    p.story("ST-001", "built_by: robot\n", &["2026-11-02 set size=3"]);
    let out = p.run(&["lint", "ST-001"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("built_by must be factory or person"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn twelve_factory_size_two_stories_form_a_size_builder_group() {
    let p = Project::new();
    finished(&p, 1, 12, 2, "factory");
    assert_eq!(
        group(&p.estimates(), 2, "factory"),
        &json!({"size": 2, "built_by": "factory", "level": "size_builder", "n": 12, "median_days": 6.5, "p85_days": 11})
    );
}

#[test]
fn a_builder_group_under_ten_falls_back_to_size() {
    let p = Project::new();
    finished(&p, 1, 4, 5, "person");
    finished(&p, 5, 7, 5, "factory");
    let report = p.estimates();
    let person = group(&report, 5, "person");
    assert_eq!(
        (&person["level"], &person["n"]),
        (&json!("size"), &json!(11))
    );
    // Days 1 to 4 and 1 to 7: the median of the 11 is 3, the 85th percentile rank 10 is 6.
    assert_eq!(
        (&person["median_days"], &person["p85_days"]),
        (&json!(3), &json!(6))
    );
    assert_eq!(group(&report, 5, "factory")["level"], "size");
}

#[test]
fn a_size_under_ten_stories_is_uncalibrated() {
    let p = Project::new();
    finished(&p, 1, 3, 8, "factory");
    let g = group(&p.estimates(), 8, "factory").clone();
    assert_eq!(
        g,
        json!({"size": 8, "built_by": "factory", "level": "none", "n": 3, "median_days": null, "p85_days": null})
    );
}

#[test]
fn human_output_is_a_table_of_rows_and_groups() {
    let p = Project::new();
    p.story(
        "ST-001",
        "built_by: person\n",
        &[
            "2026-11-02 set size=3",
            "2026-11-03 set status=in-progress",
            "2026-11-07 set status=done \u{2014} delivered: PR #12 (abc1234)",
        ],
    );
    let out = p.run(&["estimates"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("id"), "{text}");
    let line = text.lines().find(|l| l.contains("ST-001")).unwrap();
    for cell in [
        "3",
        "2026-11-03",
        "in-progress",
        "2026-11-07",
        "4",
        "person",
        "#12",
    ] {
        assert!(line.contains(cell), "{line}");
    }
    assert!(text.contains("level"), "{text}");
}
