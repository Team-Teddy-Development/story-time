//! `story metrics --from YYYY-MM-DD --to YYYY-MM-DD [--json]`: read-only delivery figures from changelog status events.

use crate::{config, story, Failure};
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::{fs, path::Path};

const SOURCE: &str = "changelog `set status=` lines in the backlog story files, day resolution";
const NO_SPRINT: &str = "no sprint record is stored";
const USAGE: &str = "Usage: story metrics --from YYYY-MM-DD --to YYYY-MM-DD [--json]";

/// A story's status events in changelog order, or the reason its history cannot be trusted.
struct History {
    id: String,
    events: Vec<(NaiveDate, String)>,
    gap: Option<String>,
}

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
    let usage = |message: String| Failure {
        code: 2,
        message: format!("{message}\n{USAGE}"),
    };
    let date = |key: &str| {
        let raw = flags
            .value(key)
            .ok_or_else(|| usage(format!("--{key} is required.")))?;
        NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .ok()
            .filter(|d| d.format("%Y-%m-%d").to_string() == raw)
            .ok_or_else(|| usage(format!("--{key} must be a YYYY-MM-DD date: {raw}")))
    };
    let (from, to) = (date("from")?, date("to")?);
    if from > to {
        return Err(usage(format!("--from {from} is after --to {to}.")));
    }
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = match flags
        .value("dir")
        .map(str::to_owned)
        .or_else(|| std::env::var("STORY_TIME_DIR").ok())
    {
        Some(dir) => config::resolve(Path::new(&dir))?,
        None => config::resolve(&loaded.root.join(settings["backlogDir"].as_str().unwrap()))?,
    };
    let report = report(&histories(&dir)?, from, to);
    Ok(if flags.has("json") {
        format!("{}\n", serde_json::to_string_pretty(&report).unwrap())
    } else {
        text(&report)
    })
}

fn histories(dir: &Path) -> Result<Vec<History>, String> {
    let mut ids: Vec<String> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            let id = name.strip_suffix(".md")?;
            crate::valid_id(id).then(|| id.to_owned())
        })
        .collect();
    ids.sort();
    ids.into_iter()
        .map(|id| {
            let text =
                fs::read_to_string(dir.join(format!("{id}.md"))).map_err(|e| e.to_string())?;
            Ok(match story::parse(&text) {
                Ok(parsed) => history(id, &parsed),
                Err(e) => History {
                    id,
                    events: Vec::new(),
                    gap: Some(format!("story file does not parse: {e}")),
                },
            })
        })
        .collect()
}

/// Reads `- DATE set k=v, status=S — note` lines; a status named only in the note is not an event.
fn history(id: String, parsed: &Value) -> History {
    let log = parsed["sections"]["Changelog"].as_str().unwrap_or("");
    let events: Vec<(NaiveDate, String)> = log
        .lines()
        .filter_map(|line| {
            let (date, rest) = line.strip_prefix("- ")?.split_once(' ')?;
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            let assignments = rest.strip_prefix("set ")?.split(" \u{2014} ").next()?;
            let status = assignments
                .split(", ")
                .find_map(|pair| pair.trim().strip_prefix("status="))?;
            Some((date, status.to_owned()))
        })
        .collect();
    let status = parsed["status"].as_str().unwrap_or("");
    let consistent = match events.last() {
        Some((_, last)) => last == status,
        None => status == "draft",
    };
    History {
        id,
        events,
        gap: (!consistent).then(|| format!("status {status} has no matching changelog event")),
    }
}

fn report(stories: &[History], from: NaiveDate, to: NaiveDate) -> Value {
    let window = format!("{from}..{to}");
    let in_window = |d: &NaiveDate| (from..=to).contains(d);
    let gaps: Vec<Value> = stories
        .iter()
        .filter_map(|s| Some(json!({"id": s.id, "reason": s.gap.as_ref()?})))
        .collect();
    let trusted: Vec<&History> = stories.iter().filter(|s| s.gap.is_none()).collect();

    let done: Vec<(&History, usize)> = trusted
        .iter()
        .filter_map(|s| {
            let at = s
                .events
                .iter()
                .position(|(d, st)| st == "done" && in_window(d))?;
            Some((*s, at))
        })
        .collect();
    let wip: Vec<&str> = trusted
        .iter()
        .filter(|s| {
            s.events
                .iter()
                .rev()
                .find(|(d, _)| *d <= to)
                .is_some_and(|(_, st)| st == "in-progress")
        })
        .map(|s| s.id.as_str())
        .collect();

    let mut cycle_unknown = gaps.clone();
    let mut known = Vec::new();
    let per_story: Vec<Value> = done
        .iter()
        .map(|(s, at)| {
            let (finished, _) = s.events[*at];
            let started = s.events[..*at].iter().find(|(_, st)| st == "in-progress");
            match started {
                Some((started, _)) => {
                    let days = (finished - *started).num_days();
                    known.push(days);
                    json!({"id": s.id, "days": days})
                }
                None => {
                    cycle_unknown.push(
                        json!({"id": s.id, "reason": "done with no earlier in-progress event"}),
                    );
                    json!({"id": s.id, "days": "unknown"})
                }
            }
        })
        .collect();

    let figure = |metric: &str, value: Value, units: &str, definition: &str| {
        json!({
            "metric": metric,
            "value": value,
            "units": units,
            "window": window,
            "source": SOURCE,
            "definition": definition,
        })
    };
    let mut throughput = figure(
        "throughput",
        json!(done.len()),
        "stories",
        "stories with a status=done event dated in the window",
    );
    throughput["stories"] = json!(done.iter().map(|(s, _)| &s.id).collect::<Vec<_>>());
    throughput["unknown"] = json!(gaps);
    let mut in_progress = figure(
        "wip",
        json!(wip.len()),
        "stories",
        "stories whose last status event on or before the window end is in-progress",
    );
    in_progress["stories"] = json!(wip);
    in_progress["unknown"] = json!(gaps);
    let mut cycle = figure(
        "cycle_time_median",
        median(&mut known),
        "days",
        "median days from the first in-progress event to the done event, over stories done in the window with both events recorded",
    );
    cycle["per_story"] = json!(per_story);
    cycle["unknown"] = json!(cycle_unknown);
    let mut velocity = figure(
        "velocity",
        json!("unknown"),
        "points per sprint",
        "completed points per sprint",
    );
    velocity["source"] = json!(NO_SPRINT);
    velocity["unknown"] = json!([{"id": null, "reason": NO_SPRINT}]);

    json!({
        "window": window,
        "source": SOURCE,
        "figures": [throughput, in_progress, cycle, velocity],
    })
}

fn median(days: &mut [i64]) -> Value {
    days.sort_unstable();
    let n = days.len();
    match n {
        0 => json!("unknown"),
        _ if n % 2 == 1 => json!(days[n / 2]),
        _ => {
            let sum = days[n / 2 - 1] + days[n / 2];
            if sum % 2 == 0 {
                json!(sum / 2)
            } else {
                json!(sum as f64 / 2.0)
            }
        }
    }
}

fn text(report: &Value) -> String {
    let plain = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_owned);
    let mut out = format!(
        "Agile metrics, window {}, source {}\n",
        plain(&report["window"]),
        plain(&report["source"])
    );
    for f in report["figures"].as_array().unwrap() {
        out += &format!(
            "{}: {} {}, window {}, source {}\n  definition: {}\n",
            plain(&f["metric"]),
            plain(&f["value"]),
            plain(&f["units"]),
            plain(&f["window"]),
            plain(&f["source"]),
            plain(&f["definition"]),
        );
        if let Some(ids) = f["stories"].as_array().filter(|ids| !ids.is_empty()) {
            let ids: Vec<String> = ids.iter().map(plain).collect();
            out += &format!("  stories: {}\n", ids.join(", "));
        }
        for row in f["per_story"].as_array().into_iter().flatten() {
            if row["days"].is_number() {
                out += &format!("  {} {} days\n", plain(&row["id"]), row["days"]);
            }
        }
        for gap in f["unknown"].as_array().into_iter().flatten() {
            let who = gap["id"]
                .as_str()
                .map_or(String::new(), |id| format!("{id} "));
            out += &format!("  {who}unknown: {}\n", plain(&gap["reason"]));
        }
    }
    out
}
