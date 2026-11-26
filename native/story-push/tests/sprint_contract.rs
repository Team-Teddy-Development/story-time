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
    fn new(team_extra: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-sprint-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::write(
            path.join(".story-time/settings.json"),
            format!(
                r#"{{"tracker":"files","sync":"off","team":{{"sprintDays":10,"wipLimit":1{team_extra}}}}}"#
            ),
        )
        .unwrap();
        Self(path)
    }
    fn dated() -> Self {
        Self::new(r#","sprintAnchor":{"date":"2026-01-05","tz":"America/Chicago"}"#)
    }
    fn story(&self, id: &str, status: &str, changelog: &[&str]) -> &Self {
        let log = changelog
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            self.0.join("backlog").join(format!("{id}.md")),
            format!("---\nid: {id}\ntitle: {id}\nstatus: {status}\ntype: story\ncreated: 2026-01-02\n---\n\n## Story\nAs a user, I want {id}.\n\n## Changelog\n- 2026-01-02 created\n{log}\n"),
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
    fn json(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn metrics(&self, to: &str) -> Value {
        self.json(&["metrics", "--from", "2026-01-01", "--to", to, "--json"])
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Three closed sprints, 2026-S01 to S03 (10 working days each from Monday 2026-01-05).
///
/// S01 committed ST-001 (3), ST-002 (5, re-sized to 8 after commitment) and ST-003 (2); ST-003
/// is dropped to S02 on 01-12 and ST-004 (1) is added on 01-13. It completes 3 + 5 + 1 = 9.
/// S02 commits ST-003 (2), ST-005 (4) and ST-006 (3, unfinished); it completes 6.
/// ST-006 carries to S03 on its start date and S03 also commits ST-007 (7); it completes 10.
fn three_closed() -> Project {
    let p = Project::dated();
    p.story(
        "ST-001",
        "done",
        &[
            "2026-01-02 set size=3, sprint=2026-S01",
            "2026-01-06 set status=in-progress",
            "2026-01-09 set status=done",
        ],
    )
    .story(
        "ST-002",
        "done",
        &[
            "2026-01-02 set size=5, sprint=2026-S01",
            "2026-01-07 set status=in-progress",
            "2026-01-08 set size=8",
            "2026-01-14 set status=done",
        ],
    )
    .story(
        "ST-003",
        "done",
        &[
            "2026-01-02 set size=2, sprint=2026-S01",
            "2026-01-12 set sprint=2026-S02",
            "2026-01-27 set status=done",
        ],
    )
    .story(
        "ST-004",
        "done",
        &[
            "2026-01-02 set size=1",
            "2026-01-13 set sprint=2026-S01 \u{2014} pulled in",
            "2026-01-15 set status=done",
        ],
    )
    .story(
        "ST-005",
        "done",
        &[
            "2026-01-16 set size=4, sprint=2026-S02",
            "2026-01-28 set status=done",
        ],
    )
    .story(
        "ST-006",
        "done",
        &[
            "2026-01-16 set size=3, sprint=2026-S02",
            "2026-01-20 set status=in-progress",
            "2026-02-02 set sprint=2026-S03",
            "2026-02-10 set status=done",
        ],
    )
    .story(
        "ST-007",
        "done",
        &[
            "2026-01-30 set size=7, sprint=2026-S03",
            "2026-02-12 set status=done",
        ],
    )
    .story("ST-008", "draft", &[]);
    p
}

fn velocity(report: &Value) -> &Value {
    report["figures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["metric"] == "velocity")
        .unwrap()
}

#[test]
fn three_closed_sprints_report_points_per_sprint_and_mean_min_max() {
    let report = three_closed().metrics("2026-02-13");
    let v = velocity(&report);
    assert_eq!(v["units"], "points per sprint");
    assert_eq!(v["value"], 8.33);
    assert_eq!(v["closed_sprints"], 3);
    assert_eq!(
        v["sprints"],
        json!([
            {"id": "2026-S01", "start": "2026-01-05", "end": "2026-01-16", "completed_points": 9},
            {"id": "2026-S02", "start": "2026-01-19", "end": "2026-01-30", "completed_points": 6},
            {"id": "2026-S03", "start": "2026-02-02", "end": "2026-02-13", "completed_points": 10}
        ])
    );
    assert_eq!(
        v["last_3"],
        json!({"sprints": 3, "mean": 8.33, "min": 6, "max": 10})
    );
    assert_eq!(v["last_6"], json!("unknown"));
    assert_eq!(v["window"], "2026-01-05..2026-02-13");
    assert!(v["source"].as_str().unwrap().contains("set sprint="));
    assert_eq!(
        v["unknown"],
        json!([{"id": null, "reason": "last_6 needs 6 closed sprints with known points, found 3"}])
    );
}

#[test]
fn a_re_size_after_commitment_does_not_change_the_points_counted() {
    let report = three_closed().metrics("2026-01-16");
    let v = velocity(&report);
    assert_eq!(
        v["sprints"][0]["completed_points"], 9,
        "ST-002 stays 5, not 8"
    );
}

#[test]
fn fewer_than_three_closed_sprints_stay_unknown_with_the_count() {
    let report = three_closed().metrics("2026-01-30");
    let v = velocity(&report);
    assert_eq!(v["value"], "unknown");
    assert_eq!(v["closed_sprints"], 2);
    assert_eq!(v["last_3"], "unknown");
    assert_eq!(
        v["unknown"],
        json!([{"id": null, "reason": "2 closed sprints with known points; velocity needs at least 3"}])
    );
}

#[test]
fn a_sprint_not_yet_closed_at_the_window_end_is_left_out() {
    let report = three_closed().metrics("2026-02-12");
    assert_eq!(velocity(&report)["closed_sprints"], 2);
}

#[test]
fn six_closed_sprints_fill_last_six() {
    let p = Project::dated();
    let sprints = [
        ("2026-S01", "2026-01-05"),
        ("2026-S02", "2026-01-19"),
        ("2026-S03", "2026-02-02"),
        ("2026-S04", "2026-02-16"),
        ("2026-S05", "2026-03-02"),
        ("2026-S06", "2026-03-16"),
    ];
    for (n, (sprint, start)) in sprints.iter().enumerate() {
        let size = format!("{start} set size={}, sprint={sprint}", n + 1);
        let done = format!("{start} set status=done");
        p.story(&format!("ST-{:03}", n + 1), "done", &[&size, &done]);
    }
    let report = p.metrics("2026-03-27");
    let v = velocity(&report);
    assert_eq!(v["closed_sprints"], 6);
    assert_eq!(
        v["last_3"],
        json!({"sprints": 3, "mean": 5, "min": 4, "max": 6})
    );
    assert_eq!(
        v["last_6"],
        json!({"sprints": 6, "mean": 3.5, "min": 1, "max": 6})
    );
    assert_eq!(v["value"], 5);
    assert_eq!(v["unknown"], json!([]));
}

#[test]
fn a_completed_story_with_no_size_at_commitment_makes_its_sprint_unknown() {
    let p = three_closed();
    p.story(
        "ST-009",
        "done",
        &[
            "2026-01-02 set sprint=2026-S01",
            "2026-01-08 set status=done",
        ],
    );
    let report = p.metrics("2026-02-13");
    let v = velocity(&report);
    assert_eq!(v["closed_sprints"], 2);
    assert_eq!(v["value"], "unknown");
    assert_eq!(
        v["unknown"][1],
        json!({"id": "2026-S01", "reason": "completed stories have no size at commitment: ST-009"})
    );
}

#[test]
fn a_story_size_set_only_after_commitment_has_unknown_points() {
    let p = Project::dated();
    p.story(
        "ST-001",
        "done",
        &[
            "2026-01-02 set sprint=2026-S01",
            "2026-01-06 set size=3",
            "2026-01-08 set status=done",
        ],
    );
    let report = p.metrics("2026-01-16");
    assert_eq!(
        velocity(&report)["unknown"][1]["reason"],
        "completed stories have no size at commitment: ST-001"
    );
}

#[test]
fn without_a_sprint_calendar_velocity_is_unknown_and_says_why() {
    let p = Project::new("");
    p.story(
        "ST-001",
        "done",
        &[
            "2026-01-02 set size=3, sprint=2026-S01",
            "2026-01-06 set status=done",
        ],
    );
    let report = p.metrics("2026-02-13");
    let v = velocity(&report);
    assert_eq!(v["value"], "unknown");
    assert_eq!(v["closed_sprints"], 0);
    assert_eq!(
        v["unknown"],
        json!([{"id": null, "reason": "no sprint calendar in settings (team.sprintAnchor or team.sprints)"}])
    );
}

#[test]
fn a_team_sprints_entry_moves_one_sprint_and_names_its_goal() {
    let p = Project::new(
        r#","sprintAnchor":{"date":"2026-01-05","tz":"UTC"},"sprints":[{"id":"2026-S02","start":"2026-01-19","end":"2026-01-23","goal":"Short sprint"}]"#,
    );
    p.story(
        "ST-001",
        "done",
        &[
            "2026-01-02 set size=2, sprint=2026-S02",
            "2026-01-27 set status=done",
        ],
    );
    let shown = p.json(&["sprint", "show", "2026-S02", "--json"]);
    assert_eq!(shown["sprint"]["end"], "2026-01-23");
    assert_eq!(shown["sprint"]["goal"], "Short sprint");
    assert_eq!(shown["sprint"]["source"], "team.sprints");
    assert_eq!(
        shown["done"]["stories"],
        json!([]),
        "done 01-27 is after the short sprint ends"
    );
    let s3 = p.json(&["sprint", "show", "2026-S03", "--json"]);
    assert_eq!(
        s3["sprint"]["start"], "2026-02-02",
        "the generated calendar does not shift"
    );
    assert_eq!(s3["sprint"]["source"], "team.sprintAnchor");
}

#[test]
fn sprint_show_lists_dates_committed_changes_and_done_and_changes_nothing() {
    let p = three_closed();
    let before: Vec<_> = fs::read_dir(p.0.join("backlog"))
        .unwrap()
        .map(|e| fs::read(e.unwrap().path()).unwrap())
        .collect();
    let shown = p.json(&["sprint", "show", "2026-S01", "--json"]);
    assert_eq!(shown["sprint"]["id"], "2026-S01");
    assert_eq!(shown["sprint"]["start"], "2026-01-05");
    assert_eq!(shown["sprint"]["end"], "2026-01-16");
    assert_eq!(shown["sprint"]["tz"], "America/Chicago");
    assert_eq!(
        shown["committed"],
        json!({
            "stories": [
                {"id": "ST-001", "points": 3},
                {"id": "ST-002", "points": 5},
                {"id": "ST-003", "points": 2}
            ],
            "points": 10
        })
    );
    assert_eq!(
        shown["changes"],
        json!([
            {"date": "2026-01-12", "id": "ST-003", "change": "drop", "to": "2026-S02", "points": 2},
            {"date": "2026-01-13", "id": "ST-004", "change": "add", "points": 1}
        ])
    );
    assert_eq!(
        shown["done"],
        json!({
            "stories": [
                {"id": "ST-001", "date": "2026-01-09", "points": 3},
                {"id": "ST-002", "date": "2026-01-14", "points": 5},
                {"id": "ST-004", "date": "2026-01-15", "points": 1}
            ],
            "points": 9
        })
    );
    let text = String::from_utf8(p.run(&["sprint", "show", "2026-S01"]).stdout).unwrap();
    for needle in [
        "2026-S01",
        "2026-01-05",
        "2026-01-16",
        "ST-003",
        "drop",
        "ST-004",
        "add",
        "9",
    ] {
        assert!(text.contains(needle), "{needle} missing:\n{text}");
    }
    let after: Vec<_> = fs::read_dir(p.0.join("backlog"))
        .unwrap()
        .map(|e| fs::read(e.unwrap().path()).unwrap())
        .collect();
    assert_eq!(before, after);
}

#[test]
fn sprint_show_refuses_an_unknown_or_malformed_sprint() {
    let p = three_closed();
    for args in [
        vec!["sprint", "show", "2026-S1"],
        vec!["sprint", "show"],
        vec!["sprint"],
        vec!["sprint", "list"],
    ] {
        let out = p.run(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
    let out = p.run(&["sprint", "show", "2025-S01"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("before the sprint anchor"));
    let none = Project::new("");
    let out = none.run(&["sprint", "show", "2026-S01"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("team.sprintAnchor"));
}

#[test]
fn the_generated_calendar_restarts_numbering_each_year() {
    let p = Project::new(r#","sprintAnchor":{"date":"2026-12-07","tz":"UTC"}"#);
    let s = |id: &str| p.json(&["sprint", "show", id, "--json"])["sprint"].clone();
    assert_eq!(s("2026-S01")["start"], "2026-12-07");
    assert_eq!(s("2026-S01")["end"], "2026-12-18");
    assert_eq!(s("2026-S02")["start"], "2026-12-21");
    assert_eq!(s("2026-S02")["end"], "2027-01-01");
    assert_eq!(s("2027-S01")["start"], "2027-01-04");
    let out = p.run(&["sprint", "show", "2026-S03"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn story_set_writes_the_sprint_and_its_changelog_line_and_refuses_a_bad_id() {
    let p = Project::dated();
    p.story("ST-001", "draft", &[]);
    let story = |p: &Project| fs::read_to_string(p.0.join("backlog/ST-001.md")).unwrap();
    let out = p.run(&["set", "ST-001", "sprint=2026-S23"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = story(&p);
    assert!(text.contains("\nsprint: 2026-S23\n"), "{text}");
    assert!(
        text.lines().any(|l| l.ends_with(" set sprint=2026-S23")),
        "{text}"
    );
    let before = story(&p);
    for bad in [
        "2026-23", "2026-S1", "2026-S00", "26-S01", "2026-s01", "next",
    ] {
        let out = p.run(&["set", "ST-001", &format!("sprint={bad}")]);
        assert_eq!(out.status.code(), Some(1), "{bad}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("sprint must be"),
            "{bad}"
        );
        assert_eq!(story(&p), before, "{bad} wrote");
    }
    let out = p.run(&["set", "ST-001", "sprint="]);
    assert!(out.status.success());
    let text = story(&p);
    assert!(!text.contains("sprint:"), "{text}");
    assert!(text.lines().any(|l| l.ends_with(" set sprint=")), "{text}");
}

#[test]
fn story_list_filters_by_sprint() {
    let p = Project::dated();
    p.story("ST-001", "draft", &[])
        .story("ST-002", "draft", &[])
        .story("ST-003", "draft", &[]);
    for id in ["ST-001", "ST-003"] {
        let sprint = if id == "ST-001" {
            "2026-S01"
        } else {
            "2026-S02"
        };
        assert!(p
            .run(&["set", id, &format!("sprint={sprint}")])
            .status
            .success());
    }
    let listed = p.json(&["list", "--sprint", "2026-S01", "--json"]);
    let ids: Vec<_> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["ST-001"]);
    assert_eq!(p.json(&["list", "--json"]).as_array().unwrap().len(), 3);
}

#[test]
fn lint_reports_a_malformed_sprint_and_ignores_a_valid_or_absent_one() {
    let p = Project::dated();
    let write = |sprint: &str| {
        fs::write(
            p.0.join("backlog/ST-001.md"),
            format!("---\nid: ST-001\ntitle: One\nstatus: draft\ntype: story\n{sprint}created: 2026-01-02\n---\n\n## Story\nAs a user, I want one, so that two.\n\n## Changelog\n- 2026-01-02 created\n"),
        )
        .unwrap();
        serde_json::from_slice::<Value>(&p.run(&["lint", "ST-001", "--json"]).stdout).unwrap()
    };
    let rules = |report: &Value| -> Vec<String> {
        report
            .to_string()
            .match_indices("sprint.format")
            .map(|_| "sprint.format".to_owned())
            .collect()
    };
    assert_eq!(rules(&write("sprint: soon\n")).len(), 1);
    assert!(rules(&write("sprint: 2026-S23\n")).is_empty());
    assert!(rules(&write("")).is_empty());
}
