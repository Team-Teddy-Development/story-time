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
            "story-burn-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::write(
            path.join(".story-time/settings.json"),
            r#"{"tracker":"files","sync":"off","team":{"sprintDays":10,"wipLimit":1,"sprints":[{"id":"2026-S01","start":"2026-11-02","end":"2026-11-15"}]}}"#,
        )
        .unwrap();
        Self(path)
    }

    fn story(&self, id: &str, front: &str, changelog: &[&str]) -> &Self {
        let status = changelog
            .iter()
            .rev()
            .find_map(|l| l.split("status=").nth(1))
            .unwrap_or("draft");
        let log: String = changelog.iter().map(|l| format!("- {l}\n")).collect();
        fs::write(
            self.0.join("backlog").join(format!("{id}.md")),
            format!("---\nid: {id}\ntitle: {id}\nstatus: {status}\ntype: story\n{front}created: 2026-11-01\n---\n\n## Story\nAs a user, I want {id}.\n\n## Changelog\n- 2026-11-01 created\n{log}"),
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

    fn burndown(&self, today: &str) -> Value {
        self.json(&[
            "burndown", "--sprint", "2026-S01", "--today", today, "--json",
        ])
    }

    /// Stories A (5), B (3) and C (2), each in 2026-S01 from 2026-11-01.
    fn fixture() -> Self {
        let p = Self::new();
        p.story("ST-001", "", &["2026-11-01 set size=5, sprint=2026-S01"])
            .story("ST-002", "", &["2026-11-01 set size=3, sprint=2026-S01"])
            .story("ST-003", "", &["2026-11-01 set size=2, sprint=2026-S01"]);
        p
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn row<'a>(report: &'a Value, day: &str) -> &'a Value {
    report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["day"] == day)
        .unwrap_or_else(|| panic!("no row for {day}"))
}

fn finish_b(p: &Project) {
    p.story(
        "ST-002",
        "",
        &[
            "2026-11-01 set size=3, sprint=2026-S01",
            "2026-11-03 set status=done",
        ],
    );
}

#[test]
fn a_story_done_on_day_two_lowers_remaining_that_day() {
    let p = Project::fixture();
    finish_b(&p);
    let report = p.burndown("2026-11-06");
    let day = row(&report, "2026-11-03");
    assert_eq!(
        (day["scope"].clone(), day["remaining"].clone()),
        (json!(10), json!(7))
    );
    assert_eq!(row(&report, "2026-11-02")["remaining"], json!(10));
    assert_eq!(row(&report, "2026-11-06")["remaining"], json!(7));
}

#[test]
fn a_story_added_on_day_four_raises_scope_that_day() {
    let p = Project::fixture();
    finish_b(&p);
    p.story(
        "ST-004",
        "",
        &["2026-11-01 set size=2", "2026-11-05 set sprint=2026-S01"],
    );
    let report = p.burndown("2026-11-06");
    let day = row(&report, "2026-11-05");
    assert_eq!(
        (day["scope"].clone(), day["remaining"].clone()),
        (json!(12), json!(9))
    );
    assert_eq!(row(&report, "2026-11-04")["scope"], json!(10));
}

#[test]
fn a_story_moved_to_another_sprint_leaves_scope_that_day() {
    let p = Project::fixture();
    p.story(
        "ST-003",
        "",
        &[
            "2026-11-01 set size=2, sprint=2026-S01",
            "2026-11-04 set sprint=2026-S02",
        ],
    );
    let report = p.burndown("2026-11-06");
    let day = row(&report, "2026-11-04");
    assert_eq!(
        (day["scope"].clone(), day["remaining"].clone()),
        (json!(8), json!(8))
    );
    assert_eq!(row(&report, "2026-11-03")["scope"], json!(10));
}

#[test]
fn rows_cover_every_day_with_ideal_weekends_today_and_future_null() {
    let report = Project::fixture().burndown("2026-11-06");
    let rows = report["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 14);
    assert_eq!(rows[0]["day"], "2026-11-02");
    assert_eq!(rows[13]["day"], "2026-11-15");
    assert_eq!(rows[0]["ideal"], json!(10));
    assert_eq!(rows[13]["ideal"], json!(0));
    let flagged = |key: &str| -> Vec<&str> {
        rows.iter()
            .filter(|r| r[key] == json!(true))
            .map(|r| r["day"].as_str().unwrap())
            .collect()
    };
    assert_eq!(
        flagged("weekend"),
        ["2026-11-07", "2026-11-08", "2026-11-14", "2026-11-15"]
    );
    assert_eq!(flagged("today"), ["2026-11-06"]);
    for r in rows {
        let after = r["day"].as_str().unwrap() > "2026-11-06";
        assert_eq!(r["remaining"].is_null(), after, "{r}");
        assert_eq!(r["scope"], json!(10), "{r}");
    }
    assert_eq!(report["metric"], "burndown");
    assert_eq!(report["units"], "points");
    assert_eq!(report["window"], "2026-11-02..2026-11-15");
    assert_eq!(report["rows"][1]["ideal"], json!(9.23));
    assert_eq!(report["rows"][12]["ideal"], json!(0.77));
    assert_eq!(report["unknown"], json!([]));
    assert_eq!(report["sprint"]["id"], "2026-S01");
    assert_eq!(report["as_of"], "2026-11-06");
}

#[test]
fn an_unsized_story_is_unknown_and_adds_nothing_to_scope() {
    let p = Project::fixture();
    p.story("ST-005", "", &["2026-11-01 set sprint=2026-S01"]);
    let report = p.burndown("2026-11-06");
    assert_eq!(row(&report, "2026-11-02")["scope"], json!(10));
    assert_eq!(row(&report, "2026-11-06")["remaining"], json!(10));
    assert_eq!(
        report["unknown"],
        json!([{"id": "ST-005", "reason": "no size when it entered the sprint"}])
    );
}

#[test]
fn today_before_the_sprint_has_no_remaining_and_after_it_has_every_day() {
    let p = Project::fixture();
    let before = p.burndown("2026-10-30");
    assert!(before["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["remaining"].is_null() && r["today"] == json!(false)));
    let after = p.burndown("2026-12-01");
    let rows = after["rows"].as_array().unwrap();
    assert!(rows
        .iter()
        .all(|r| r["remaining"] == json!(10) && r["today"] == json!(false)));
}

#[test]
fn without_today_the_command_uses_the_clock() {
    let p = Project::fixture();
    let report = p.json(&["burndown", "--sprint", "2026-S01", "--json"]);
    assert_eq!(report["rows"].as_array().unwrap().len(), 14);
}

#[test]
fn text_output_names_each_day_and_the_unknowns() {
    let p = Project::fixture();
    p.story("ST-005", "", &["2026-11-01 set sprint=2026-S01"]);
    let out = p.run(&["burndown", "--sprint", "2026-S01", "--today", "2026-11-06"]);
    let text = String::from_utf8(out.stdout).unwrap();
    for needle in [
        "2026-S01",
        "2026-11-02",
        "2026-11-15",
        "weekend",
        "today",
        "unknown ST-005",
    ] {
        assert!(text.contains(needle), "{needle} missing:\n{text}");
    }
}

#[test]
fn bad_arguments_exit_two_with_usage() {
    let p = Project::fixture();
    for args in [
        vec!["burndown"],
        vec!["burndown", "--sprint", "nope"],
        vec!["burndown", "--sprint", "2026-S01", "--today", "2026-13-01"],
        vec!["burnup"],
        vec!["burnup", "--epic", "EP-1"],
        vec![
            "burnup",
            "--epic",
            "EP-1",
            "--from",
            "2026-11-08",
            "--to",
            "2026-11-02",
        ],
        vec![
            "burnup",
            "--epic",
            "EP-1",
            "--ids",
            "ST-001",
            "--from",
            "2026-11-02",
            "--to",
            "2026-11-08",
        ],
    ] {
        let out = p.run(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("Usage: story burn"),
            "{args:?}"
        );
    }
    let out = p.run(&["burndown", "--sprint", "2026-S09", "--json"]);
    assert_eq!(out.status.code(), Some(1));
}

fn epic() -> Project {
    let p = Project::new();
    p.story(
        "ST-011",
        "epic: EP-1\n",
        &["2026-11-01 set size=3", "2026-11-04 set status=done"],
    )
    .story("ST-012", "epic: EP-1\n", &["2026-11-01 set size=5"])
    .story("ST-013", "epic: EP-2\n", &["2026-11-01 set size=8"]);
    p
}

#[test]
fn burnup_for_an_epic_counts_done_against_total_scope() {
    let p = epic();
    let report = p.json(&[
        "burnup",
        "--epic",
        "EP-1",
        "--from",
        "2026-11-02",
        "--to",
        "2026-11-08",
        "--json",
    ]);
    let days: Vec<_> = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["day"].as_str().unwrap().to_owned(),
                r["scope"].clone(),
                r["done"].clone(),
            )
        })
        .collect();
    assert_eq!(days.len(), 7);
    let day = row(&report, "2026-11-04");
    assert_eq!(
        (day["scope"].clone(), day["done"].clone()),
        (json!(8), json!(3))
    );
    assert_eq!(row(&report, "2026-11-03")["done"], json!(0));
    assert_eq!(row(&report, "2026-11-08")["done"], json!(3));
    assert_eq!(report["metric"], "burnup");
    assert_eq!(report["units"], "points");
    assert_eq!(report["window"], "2026-11-02..2026-11-08");
    assert_eq!(report["unknown"], json!([]));
}

#[test]
fn burnup_by_ids_and_unsized_stories_stay_unknown() {
    let p = epic();
    p.story("ST-014", "", &[]);
    let report = p.json(&[
        "burnup",
        "--ids",
        "ST-011,ST-014",
        "--from",
        "2026-11-02",
        "--to",
        "2026-11-05",
        "--json",
    ]);
    assert_eq!(row(&report, "2026-11-05")["scope"], json!(3));
    assert_eq!(
        report["unknown"],
        json!([{"id": "ST-014", "reason": "no size"}])
    );
}

#[test]
fn burnup_scope_follows_a_resize_and_ignores_stories_not_yet_sized() {
    let p = Project::new();
    p.story(
        "ST-015",
        "epic: EP-1\n",
        &["2026-11-02 set size=2", "2026-11-04 set size=5"],
    );
    let report = p.json(&[
        "burnup",
        "--epic",
        "EP-1",
        "--from",
        "2026-11-01",
        "--to",
        "2026-11-05",
        "--json",
    ]);
    let scope: Vec<Value> = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["scope"].clone())
        .collect();
    assert_eq!(scope, [json!(0), json!(2), json!(2), json!(5), json!(5)]);
}

#[test]
fn an_unknown_epic_or_id_is_refused() {
    let p = epic();
    let out = p.run(&[
        "burnup",
        "--epic",
        "EP-9",
        "--from",
        "2026-11-02",
        "--to",
        "2026-11-08",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let out = p.run(&[
        "burnup",
        "--ids",
        "ST-011,ST-099",
        "--from",
        "2026-11-02",
        "--to",
        "2026-11-08",
    ]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn a_story_done_and_then_moved_out_leaves_scope_without_counting_done_twice() {
    let p = Project::fixture();
    p.story(
        "ST-001",
        "",
        &[
            "2026-11-01 set size=5, sprint=2026-S01",
            "2026-11-03 set status=done",
            "2026-11-05 set sprint=2026-S02",
        ],
    );
    let report = p.burndown("2026-11-06");
    let before = row(&report, "2026-11-04");
    assert_eq!(
        (before["scope"].clone(), before["remaining"].clone()),
        (json!(10), json!(10))
    );
    let after = row(&report, "2026-11-05");
    assert_eq!(
        (after["scope"].clone(), after["remaining"].clone()),
        (json!(5), json!(5))
    );
}

#[test]
fn a_story_already_done_when_it_joins_adds_scope_and_done_the_same_day() {
    let p = Project::fixture();
    p.story(
        "ST-004",
        "",
        &[
            "2026-11-01 set size=2",
            "2026-11-03 set status=done",
            "2026-11-05 set sprint=2026-S01",
        ],
    );
    let report = p.burndown("2026-11-06");
    let day = row(&report, "2026-11-05");
    assert_eq!(
        (day["scope"].clone(), day["remaining"].clone()),
        (json!(12), json!(10))
    );
    assert_eq!(row(&report, "2026-11-04")["remaining"], json!(10));
}

#[test]
fn the_default_today_follows_the_calendar_time_zone() {
    if !std::path::Path::new("/usr/share/zoneinfo/Pacific/Kiritimati").exists() {
        return;
    }
    let as_of = |tz: &str| {
        let p = Project::new();
        fs::write(
            p.0.join(".story-time/settings.json"),
            format!(r#"{{"tracker":"files","sync":"off","team":{{"sprintDays":10,"sprintAnchor":{{"date":"2026-11-02","tz":"{tz}"}}}}}}"#),
        )
        .unwrap();
        p.json(&["burndown", "--sprint", "2026-S01", "--json"])["as_of"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_ne!(as_of("Pacific/Kiritimati"), as_of("Pacific/Pago_Pago"));
}

#[test]
fn burnup_stops_counting_a_story_done_after_it_is_reopened() {
    let p = Project::new();
    p.story(
        "ST-011",
        "epic: EP-1\n",
        &[
            "2026-11-01 set size=3",
            "2026-11-03 set status=done",
            "2026-11-05 set status=in-progress",
        ],
    );
    let report = p.json(&[
        "burnup",
        "--epic",
        "EP-1",
        "--from",
        "2026-11-02",
        "--to",
        "2026-11-06",
        "--json",
    ]);
    let done: Vec<Value> = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["done"].clone())
        .collect();
    assert_eq!(done, [json!(0), json!(3), json!(3), json!(0), json!(0)]);
}
