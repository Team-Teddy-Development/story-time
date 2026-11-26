use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const SPRINTS: [(&str, &str, &str); 7] = [
    ("2026-S01", "2026-09-07", "2026-09-20"),
    ("2026-S02", "2026-09-21", "2026-10-04"),
    ("2026-S03", "2026-10-05", "2026-10-18"),
    ("2026-S04", "2026-10-19", "2026-11-01"),
    ("2026-S05", "2026-11-02", "2026-11-15"),
    ("2026-S06", "2026-11-16", "2026-11-29"),
    ("2026-S07", "2026-11-30", "2026-12-13"),
];

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        Self::with_anchor(true)
    }

    /// The seven listed sprints and nothing after them.
    fn bounded() -> Self {
        Self::with_anchor(false)
    }

    fn with_anchor(anchored: bool) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-forecast-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::create_dir_all(path.join(".story-time")).unwrap();
        let sprints: Vec<Value> = SPRINTS
            .iter()
            .map(|(id, start, end)| json!({"id": id, "start": start, "end": end}))
            .collect();
        let mut team = json!({"sprintDays": 10, "wipLimit": 1, "sprints": sprints});
        if anchored {
            team["sprintAnchor"] = json!({"date": "2026-09-07", "tz": "UTC"});
        }
        fs::write(
            path.join(".story-time/settings.json"),
            json!({"tracker": "files", "sync": "off", "team": team}).to_string(),
        )
        .unwrap();
        Self(path)
    }

    fn write(&self, id: &str, status: &str, front: &str, changelog: &[String]) -> &Self {
        let log: String = changelog.iter().map(|l| format!("- {l}\n")).collect();
        fs::write(
            self.0.join("backlog").join(format!("{id}.md")),
            format!("---\nid: {id}\ntitle: {id}\nstatus: {status}\ntype: story\n{front}created: 2026-01-02\n---\n\n## Story\nAs a user, I want {id}.\n\n## Changelog\n- 2026-01-02 created\n{log}"),
        )
        .unwrap();
        self
    }

    /// One story per sprint, sized `points` when it enters the sprint and done inside it.
    fn closed(&self, velocities: &[u32]) -> &Self {
        for (n, points) in velocities.iter().enumerate() {
            let (id, start, done) = SPRINTS[n];
            self.write(
                &format!("ST-1{n:02}"),
                "done",
                &format!("size: {points}\n"),
                &[
                    format!("2026-01-03 set size={points}, sprint={id}"),
                    format!("{start} set status=in-progress"),
                    format!("{done} set status=done"),
                ],
            );
        }
        self
    }

    fn open(&self, id: &str, size: Option<u32>, extra: &str) -> &Self {
        let front = size.map_or(String::new(), |s| format!("size: {s}\n"));
        self.write(
            id,
            "ready",
            &format!("{front}{extra}"),
            &["2026-01-04 set status=ready".to_owned()],
        )
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

    fn forecast(&self, args: &[&str]) -> Value {
        let mut all = vec!["forecast"];
        all.extend(args);
        all.push("--json");
        let out = self.run(&all);
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

fn dates(report: &Value) -> Vec<String> {
    report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["date"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn constant_velocity_gives_the_same_date_at_both_percentiles() {
    let p = Project::new();
    p.closed(&[10; 5])
        .open("ST-201", Some(10), "")
        .open("ST-202", Some(10), "");
    let report = p.forecast(&["--ids", "ST-201,ST-202", "--today", "2026-11-15"]);
    assert_eq!(
        report["rows"],
        json!([
            {"percentile": 50, "date": "2026-12-13"},
            {"percentile": 85, "date": "2026-12-13"},
        ])
    );
    assert_eq!(report["metric"], "forecast");
    assert_eq!(report["units"], "date");
    assert_eq!(report["remaining_points"], 20);
    assert_eq!(report["runs"], 10000);
    assert_eq!(report["seed"], 1);
    assert_eq!(report["sample_count"], 5);
}

fn mixed() -> Project {
    let p = Project::new();
    p.closed(&[8, 10, 12, 9, 11])
        .open("ST-201", Some(12), "")
        .open("ST-202", Some(8), "");
    p
}

#[test]
fn the_same_seed_gives_byte_identical_output() {
    let p = mixed();
    let args = [
        "forecast",
        "--ids",
        "ST-201,ST-202",
        "--today",
        "2026-11-15",
        "--seed",
        "7",
        "--json",
    ];
    let (first, second) = (p.run(&args), p.run(&args));
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let report: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["seed"], 7);
}

#[test]
fn a_fixed_seed_pins_the_dates_and_the_85_date_is_never_earlier() {
    let p = mixed();
    let report = p.forecast(&["--ids", "ST-201,ST-202", "--today", "2026-11-15"]);
    assert_eq!(report["seed"], 1);
    assert_eq!(dates(&report), PINNED_DEFAULT_SEED);
    for seed in ["1", "2", "7", "99", "12345"] {
        let r = p.forecast(&[
            "--ids",
            "ST-201,ST-202",
            "--today",
            "2026-11-15",
            "--seed",
            seed,
        ]);
        let d = dates(&r);
        assert!(d[0] <= d[1], "seed {seed}: {d:?}");
    }
}

/// Computed by an independent Python re-implementation of the sampler, not by this binary.
const PINNED_DEFAULT_SEED: [&str; 2] = ["2026-12-13", "2026-12-25"];

#[test]
fn unsized_stories_are_listed_unknown_and_left_out_of_remaining_points() {
    let p = Project::new();
    p.closed(&[10; 5])
        .open("ST-201", Some(5), "")
        .open("ST-202", None, "");
    let report = p.forecast(&["--ids", "ST-201,ST-202", "--today", "2026-11-15"]);
    assert_eq!(report["remaining_points"], 5);
    assert_eq!(
        report["unknown"],
        json!([{"id": "ST-202", "reason": "not sized"}])
    );
}

#[test]
fn a_sprint_target_is_committed_plus_adds_minus_drops_minus_done() {
    let p = Project::new();
    p.closed(&[10; 5]);
    let log = |lines: &[&str]| lines.iter().map(|l| (*l).to_owned()).collect::<Vec<_>>();
    p.write(
        "ST-301",
        "done",
        "size: 4\n",
        &log(&[
            "2026-11-10 set size=4, sprint=2026-S06",
            "2026-11-17 set status=in-progress",
            "2026-11-18 set status=done",
        ]),
    );
    p.write(
        "ST-302",
        "ready",
        "size: 2\nsprint: 2026-S07\n",
        &log(&[
            "2026-11-10 set size=2, sprint=2026-S06",
            "2026-11-12 set status=ready",
            "2026-11-19 set sprint=2026-S07",
        ]),
    );
    for (id, size) in [("ST-303", 3), ("ST-304", 3)] {
        p.write(
            id,
            "ready",
            &format!("size: {size}\nsprint: 2026-S06\n"),
            &log(&[
                &format!("2026-11-10 set size={size}, sprint=2026-S06"),
                "2026-11-12 set status=ready",
            ]),
        );
    }
    let report = p.forecast(&["--sprint", "2026-S06", "--today", "2026-11-20"]);
    assert_eq!(report["remaining_points"], 6);
    assert_eq!(report["unknown"], json!([]));
}

#[test]
fn a_sprint_forecast_ignores_membership_and_status_events_after_today() {
    let p = Project::new();
    p.closed(&[10; 5]);
    for (id, status, later) in [
        ("ST-301", "done", "2026-11-22 set sprint=2026-S07"),
        ("ST-302", "in-progress", "2026-11-22 set status=in-progress"),
    ] {
        p.write(
            id,
            status,
            "size: 4\n",
            &[
                "2026-11-10 set size=4, sprint=2026-S06".to_owned(),
                "2026-11-18 set status=done".to_owned(),
                later.to_owned(),
            ],
        );
    }
    let report = p.forecast(&["--sprint", "2026-S06", "--today", "2026-11-20"]);
    assert_eq!(report["remaining_points"], 0);
    assert_eq!(dates(&report), ["2026-11-20", "2026-11-20"]);
}

#[test]
fn fewer_than_three_closed_sprints_give_unknown_dates_with_the_count() {
    let p = Project::new();
    p.closed(&[10, 10]).open("ST-201", Some(5), "");
    let report = p.forecast(&["--ids", "ST-201", "--today", "2026-11-15"]);
    assert_eq!(report["sample_count"], 2);
    assert_eq!(report["remaining_points"], 5);
    let rows = report["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for (row, percentile) in rows.iter().zip([50, 85]) {
        assert_eq!(row["percentile"], percentile);
        assert_eq!(row["date"], "unknown");
        assert!(
            row["reason"].as_str().unwrap().contains("2 closed sprints"),
            "{row}"
        );
    }
}

#[test]
fn an_epic_target_is_its_open_stories_and_done_stories_do_not_count() {
    let p = Project::new();
    p.closed(&[10; 5])
        .open("ST-201", Some(3), "epic: ST-900\n")
        .open("ST-202", Some(4), "epic: ST-900\n")
        .open("ST-203", Some(50), "epic: ST-901\n");
    p.write(
        "ST-204",
        "done",
        "size: 8\nepic: ST-900\n",
        &["2026-11-01 set status=done".to_owned()],
    );
    let report = p.forecast(&["--epic", "ST-900", "--today", "2026-11-15"]);
    assert_eq!(report["remaining_points"], 7);
}

#[test]
fn a_run_that_cannot_finish_before_the_calendar_ends_is_unknown() {
    let p = Project::bounded();
    p.closed(&[1; 5]).open("ST-201", Some(500), "");
    let report = p.forecast(&["--ids", "ST-201", "--today", "2026-11-15"]);
    assert_eq!(report["rows"][0]["date"], "unknown");
    assert!(report["rows"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("calendar"));
}

#[test]
fn exactly_one_target_and_valid_flags_are_required() {
    let p = Project::new();
    p.closed(&[10; 5]).open("ST-201", Some(5), "");
    for args in [
        vec!["forecast", "--today", "2026-11-15"],
        vec![
            "forecast",
            "--ids",
            "ST-201",
            "--epic",
            "E",
            "--today",
            "2026-11-15",
        ],
        vec!["forecast", "--ids", "ST-201", "--today", "soon"],
        vec![
            "forecast",
            "--ids",
            "ST-201",
            "--today",
            "2026-11-15",
            "--seed",
            "-1",
        ],
        vec![
            "forecast",
            "--ids",
            "ST-201",
            "--today",
            "2026-11-15",
            "--seed",
            "x",
        ],
        vec!["forecast", "--ids", "ST-999", "--today", "2026-11-15"],
        vec!["forecast", "--sprint", "soon", "--today", "2026-11-15"],
    ] {
        let out = p.run(&args);
        assert_eq!(
            out.status.code(),
            Some(if args.contains(&"ST-999") { 1 } else { 2 }),
            "{args:?}"
        );
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(!out.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn text_output_names_both_dates() {
    let p = Project::new();
    p.closed(&[10; 5]).open("ST-201", Some(20), "");
    let out = p.run(&["forecast", "--ids", "ST-201", "--today", "2026-11-15"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("50%"), "{text}");
    assert!(text.contains("85%"), "{text}");
    assert!(text.contains("2026-12-13"), "{text}");
}
