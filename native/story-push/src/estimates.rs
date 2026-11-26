//! `story estimates [--json]`: each done story's estimate set against how long it took, and
//! calibration groups by size and builder. Read-only. The estimate is the last `set size=` on or
//! before the start event; the actual is the calendar days from the first `status=in-progress`
//! (else the first `status=ready`) to the first `status=done`.

use crate::metrics::{self, number, History};
use crate::{config, list, Failure};
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const SOURCE: &str = "changelog `set size=` and `set status=` lines in the backlog story files, day resolution; `built_by` from the story front matter; the PR from the note `delivered: PR #N (sha)` on the first done line";
const DEFINITION: &str = "rows: one per done story; estimate: the last size set on or before the start, else the front-matter size when no size was ever set; start: the first in-progress event, else the first ready event (start_event says which); actual_days: calendar days from start to the first done event; groups: for each size and builder, level size_builder when that pair has at least 10 rows, else size when the size has at least 10 rows, else none with no figures; median_days and p85_days (nearest rank) over the level's rows";
/// Rows a group needs before its figures stand for it. Below 7, the nearest-rank 85th
/// percentile is the group's maximum.
const MIN_GROUP: usize = 10;

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let report = report(&metrics::histories(&dir)?);
    Ok(if flags.has("json") {
        format!("{}\n", serde_json::to_string_pretty(&report).unwrap())
    } else {
        text(&report)
    })
}

struct Row<'a> {
    id: &'a str,
    estimate: Option<f64>,
    start: Option<(NaiveDate, &'static str)>,
    done: NaiveDate,
    built_by: &'a str,
    pr: Option<u64>,
}

impl Row<'_> {
    fn actual_days(&self) -> Option<i64> {
        self.start.map(|(start, _)| (self.done - start).num_days())
    }

    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "estimate": self.estimate.map(number),
            "start": self.start.map(|(d, _)| d.to_string()),
            "start_event": self.start.map(|(_, e)| e),
            "done": self.done.to_string(),
            "actual_days": self.actual_days(),
            "built_by": self.built_by,
            "pr": self.pr,
        })
    }
}

fn first(story: &History, status: &str) -> Option<NaiveDate> {
    story
        .log
        .status
        .iter()
        .filter(|(_, s)| s == status)
        .map(|(d, _)| *d)
        .min()
}

fn row(story: &History) -> Option<Row<'_>> {
    if story.gap.is_some() || story.status != "done" {
        return None;
    }
    let done = first(story, "done")?;
    let start = first(story, "in-progress")
        .map(|d| (d, "in-progress"))
        .or_else(|| first(story, "ready").map(|d| (d, "ready")));
    let estimate = if story.log.size.is_empty() {
        story.size
    } else {
        let by = start.map_or(done, |(d, _)| d);
        story
            .log
            .size
            .iter()
            .rfind(|(d, _)| *d <= by)
            .and_then(|(_, n)| *n)
    };
    Some(Row {
        id: &story.id,
        estimate,
        start,
        done,
        built_by: story.built_by.as_deref().unwrap_or("unknown"),
        pr: story.pr,
    })
}

fn median(sorted: &[i64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2] as f64
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0
    }
}

/// The nearest-rank percentile: the value at rank ceil(p * n).
fn nearest_rank(sorted: &[i64], p: f64) -> f64 {
    let rank = (p * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1] as f64
}

/// Sizes as keys that sort numerically and compare exactly.
fn key(size: f64) -> (i64, u64) {
    (size.floor() as i64, size.to_bits())
}

fn groups(rows: &[Row]) -> Vec<Value> {
    let measured: Vec<(f64, &str, i64)> = rows
        .iter()
        .filter_map(|r| Some((r.estimate?, r.built_by, r.actual_days()?)))
        .collect();
    let mut pairs: BTreeSet<((i64, u64), &str)> = BTreeSet::new();
    let mut sizes: BTreeMap<(i64, u64), f64> = BTreeMap::new();
    for (size, builder, _) in &measured {
        pairs.insert((key(*size), builder));
        sizes.insert(key(*size), *size);
    }
    let days = |keep: &dyn Fn(f64, &str) -> bool| -> Vec<i64> {
        let mut days: Vec<i64> = measured
            .iter()
            .filter(|(s, b, _)| keep(*s, b))
            .map(|(_, _, d)| *d)
            .collect();
        days.sort_unstable();
        days
    };
    pairs
        .into_iter()
        .map(|(k, builder)| {
            let size = sizes[&k];
            let own = days(&|s, b| key(s) == k && b == builder);
            let all = days(&|s, _| key(s) == k);
            let (level, sample) = if own.len() >= MIN_GROUP {
                ("size_builder", &own)
            } else if all.len() >= MIN_GROUP {
                ("size", &all)
            } else {
                ("none", &own)
            };
            let figure = |f: &dyn Fn(&[i64]) -> f64| (level != "none").then(|| number(f(sample)));
            json!({
                "size": number(size),
                "built_by": builder,
                "level": level,
                "n": sample.len(),
                "median_days": figure(&median),
                "p85_days": figure(&|s| nearest_rank(s, 0.85)),
            })
        })
        .collect()
}

fn report(stories: &[History]) -> Value {
    let rows: Vec<Row> = stories.iter().filter_map(row).collect();
    let unknown: Vec<Value> = stories
        .iter()
        .filter(|s| s.status == "done")
        .filter_map(|s| Some(json!({"id": s.id, "reason": s.gap.as_ref()?})))
        .collect();
    json!({
        "metric": "estimates",
        "units": "days",
        "source": SOURCE,
        "definition": DEFINITION,
        "min_group": MIN_GROUP,
        "unknown": unknown,
        "rows": rows.iter().map(Row::json).collect::<Vec<_>>(),
        "groups": groups(&rows),
    })
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => "-".to_owned(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

fn table(head: &str, columns: &[&str], rows: &[Value], show: fn(&str, &Value) -> String) -> String {
    let cells: Vec<Vec<String>> =
        std::iter::once(columns.iter().map(|c| (*c).to_owned()).collect())
            .chain(
                rows.iter()
                    .map(|r| columns.iter().map(|c| show(c, &r[*c])).collect()),
            )
            .collect();
    let widths: Vec<usize> = (0..columns.len())
        .map(|i| cells.iter().map(|r| r[i].len()).max().unwrap_or(0))
        .collect();
    let mut out = format!("{head}\n");
    for row in cells {
        let line: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(c, w)| format!("{c:<w$}"))
            .collect();
        out += &format!("  {}\n", line.join("  ").trim_end());
    }
    out
}

fn text(report: &Value) -> String {
    let rows = report["rows"].as_array().unwrap();
    let groups = report["groups"].as_array().unwrap();
    let mut out = table(
        &format!(
            "Estimates: {} done stories, actual in calendar days",
            rows.len()
        ),
        &[
            "id",
            "estimate",
            "start",
            "start_event",
            "done",
            "actual_days",
            "built_by",
            "pr",
        ],
        rows,
        |column, v| match (column, v) {
            ("pr", Value::Number(n)) => format!("#{n}"),
            (_, v) => cell(v),
        },
    );
    out += "\n";
    out += &table(
        &format!(
            "Calibration groups (at least {} stories per level)",
            report["min_group"]
        ),
        &["size", "built_by", "level", "n", "median_days", "p85_days"],
        groups,
        |_, v| cell(v),
    );
    for gap in report["unknown"].as_array().unwrap() {
        out += &format!("unknown {}: {}\n", cell(&gap["id"]), cell(&gap["reason"]));
    }
    out
}
