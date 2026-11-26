//! `story metrics --from YYYY-MM-DD --to YYYY-MM-DD [--json]`: read-only delivery figures from changelog status events.

use crate::sprint::{self, Calendar, StoryLog};
use crate::{config, lenient_story, list, Failure};
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, path::Path};

const SOURCE: &str =
    "changelog `set status=` and `split into` lines in the backlog story files, day resolution";
const SPRINT_SOURCE: &str = "changelog `set sprint=`, `set size=` and `set status=` lines in the backlog story files, day resolution, with sprint dates from settings team.sprintAnchor and team.sprints";
const NO_CALENDAR: &str = "no sprint calendar in settings (team.sprintAnchor or team.sprints)";
const USAGE: &str = "Usage: story metrics --from YYYY-MM-DD --to YYYY-MM-DD [--json]";

/// A story's changelog events in file order, or the reason its history cannot be trusted.
pub(crate) struct History {
    pub(crate) id: String,
    pub(crate) log: StoryLog,
    pub(crate) gap: Option<String>,
    pub(crate) status: String,
    pub(crate) size: Option<f64>,
    pub(crate) epic: Option<String>,
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
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let calendar = Calendar::from_settings(&settings);
    let report = report(&histories(&dir)?, calendar.as_ref(), from, to);
    Ok(if flags.has("json") {
        format!("{}\n", serde_json::to_string_pretty(&report).unwrap())
    } else {
        text(&report)
    })
}

pub(crate) fn histories(dir: &Path) -> Result<Vec<History>, String> {
    histories_with(
        dir,
        |id| {
            fs::read_to_string(dir.join(format!("{id}.md")))
                .map(Some)
                .map_err(|e| e.to_string())
        },
        |e| format!("{}: {e}", dir.display()),
    )
}

pub(crate) fn histories_with<E>(
    dir: &Path,
    mut read: impl FnMut(&str) -> Result<Option<String>, E>,
    list_error: impl FnOnce(std::io::Error) -> E,
) -> Result<Vec<History>, E> {
    let mut ids: Vec<String> = fs::read_dir(dir)
        .map_err(list_error)?
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            crate::story_file_id(&name).map(str::to_owned)
        })
        .collect();
    ids.sort();
    let mut histories = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(text) = read(&id)? else { continue };
        histories.push(match lenient_story::parse(&text) {
            Ok(parsed) => history(id, &parsed.to_value()),
            Err(e) => History {
                id,
                log: StoryLog::default(),
                gap: Some(format!("story file does not parse: {e}")),
                status: String::new(),
                size: None,
                epic: None,
            },
        });
    }
    Ok(histories)
}

/// Reads `- DATE set k=v, status=S — note` and `- DATE split into ...` lines. A status named only
/// in the note is not an event. `sprint=` and `size=` are read from the same assignments.
fn history(id: String, parsed: &Value) -> History {
    let text = parsed["sections"]["Changelog"].as_str().unwrap_or("");
    let mut log = StoryLog::default();
    for line in text.lines() {
        let Some((date, rest)) = line.strip_prefix("- ").and_then(|l| l.split_once(' ')) else {
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
            continue;
        };
        if rest.starts_with("split into ") {
            log.status.push((date, "split".to_owned()));
            continue;
        }
        let Some(assignments) = rest
            .strip_prefix("set ")
            .and_then(|r| r.split(" \u{2014} ").next())
        else {
            continue;
        };
        for pair in assignments.split(", ") {
            let pair = pair.trim();
            if let Some(status) = pair.strip_prefix("status=") {
                log.status.push((date, status.to_owned()));
            } else if let Some(sprint) = pair.strip_prefix("sprint=") {
                log.sprint.push((
                    date,
                    Some(sprint).filter(|s| !s.is_empty()).map(str::to_owned),
                ));
            } else if let Some(size) = pair.strip_prefix("size=") {
                log.size
                    .push((date, size.parse::<f64>().ok().filter(|n| n.is_finite())));
            }
        }
    }
    let status = parsed["status"].as_str().unwrap_or("");
    let consistent = match log.status.last() {
        Some((_, last)) => last == status,
        None => status == "draft",
    };
    History {
        id,
        log,
        gap: (!consistent).then(|| format!("status {status} has no matching changelog event")),
        status: status.to_owned(),
        size: parsed["size"].as_f64().filter(|n| n.is_finite()),
        epic: parsed["epic"].as_str().map(str::to_owned),
    }
}

fn report(
    stories: &[History],
    calendar: Option<&Calendar>,
    from: NaiveDate,
    to: NaiveDate,
) -> Value {
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
                .log
                .status
                .iter()
                .position(|(d, st)| st == "done" && in_window(d))?;
            Some((*s, at))
        })
        .collect();
    let wip: Vec<&str> = trusted
        .iter()
        .filter(|s| {
            s.log
                .status
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
            let (finished, _) = s.log.status[*at];
            let started = s.log.status[..*at]
                .iter()
                .find(|(_, st)| st == "in-progress");
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
    let velocity = velocity(stories, calendar, from, to);

    json!({
        "window": window,
        "source": SOURCE,
        "figures": [throughput, in_progress, cycle, velocity],
    })
}

/// Completed points per closed sprint. A sprint is closed when its end is on or before `to`, and
/// the window start does not apply: the figure looks back over every closed sprint. Each sprint
/// counts the points its stories had when they entered it, so a re-estimate never rewrites it.
pub(crate) fn velocity(
    stories: &[History],
    calendar: Option<&Calendar>,
    from: NaiveDate,
    to: NaiveDate,
) -> Value {
    let mut figure = json!({
        "metric": "velocity",
        "value": "unknown",
        "units": "points per sprint",
        "window": format!("{from}..{to}"),
        "source": SPRINT_SOURCE,
        "definition": "mean completed points per closed sprint over the last 3 closed sprints (closed: sprint end on or before the window end; points: size when the story entered the sprint; done: a status=done event inside the sprint dates), with min and max, and the same over the last 6",
    });
    let (known, sprint_unknown) = closed_velocities(stories, calendar, to);
    let count = known.len();
    let points: Vec<f64> = known.iter().map(|(_, p)| *p).collect();
    let stat = |n: usize| match sprint::stat(&points, n) {
        Some(s) => {
            json!({"sprints": s.sprints, "mean": number(s.mean), "min": number(s.min), "max": number(s.max)})
        }
        None => json!("unknown"),
    };
    figure["closed_sprints"] = json!(count);
    figure["sprints"] = json!(known
        .iter()
        .map(|(s, p)| json!({"id": s.id, "start": s.start.to_string(), "end": s.end.to_string(), "completed_points": number(*p)}))
        .collect::<Vec<_>>());
    figure["last_3"] = stat(3);
    figure["last_6"] = stat(6);
    if figure["last_3"] != json!("unknown") {
        figure["value"] = figure["last_3"]["mean"].clone();
    }
    if let Some((first, _)) = known.first() {
        figure["window"] = json!(format!("{}..{to}", first.start));
    }
    let mut unknown = Vec::new();
    if calendar.is_none() {
        unknown.push(json!({"id": null, "reason": NO_CALENDAR}));
    } else if count < 3 {
        let plural = if count == 1 { "" } else { "s" };
        unknown.push(json!({"id": null, "reason": format!("{count} closed sprint{plural} with known points; velocity needs at least 3")}));
    } else if count < 6 {
        unknown.push(json!({"id": null, "reason": format!("last_6 needs 6 closed sprints with known points, found {count}")}));
    }
    unknown.extend(sprint_unknown);
    figure["unknown"] = json!(unknown);
    figure
}

/// The closed sprints with known completed points, oldest first, and the sprints left out with
/// the reason. A sprint is closed when its end is on or before `to`.
pub(crate) fn closed_velocities(
    stories: &[History],
    calendar: Option<&Calendar>,
    to: NaiveDate,
) -> (Vec<(sprint::Sprint, f64)>, Vec<Value>) {
    let mut sprint_unknown: Vec<Value> = Vec::new();
    let mut known: Vec<(sprint::Sprint, f64)> = Vec::new();
    let trusted: Vec<&History> = stories.iter().filter(|s| s.gap.is_none()).collect();
    if let Some(calendar) = calendar {
        let ids: BTreeSet<&str> = trusted
            .iter()
            .flat_map(|s| s.log.sprint.iter().filter_map(|(_, id)| id.as_deref()))
            .collect();
        let mut closed = Vec::new();
        for id in ids {
            match calendar.resolve(id) {
                Ok(sprint) if sprint.end <= to => closed.push(sprint),
                Ok(_) => {}
                Err(reason) => sprint_unknown.push(json!({"id": id, "reason": reason})),
            }
        }
        closed.sort_by(|a, b| (a.start, &a.id).cmp(&(b.start, &b.id)));
        for sprint in closed {
            let derived = sprint::derive(&sprint, trusted.iter().map(|s| (s.id.as_str(), &s.log)));
            let reason = if !derived.had_members() {
                "no story was in the sprint by its end".to_owned()
            } else {
                match derived.completed() {
                    Ok(points) => {
                        known.push((sprint, points));
                        continue;
                    }
                    Err(ids) => format!(
                        "completed stories have no size at commitment: {}",
                        ids.join(", ")
                    ),
                }
            };
            sprint_unknown.push(json!({"id": sprint.id, "reason": reason}));
        }
    }
    (known, sprint_unknown)
}

/// A whole number as an integer, anything else rounded to two places.
pub(crate) fn number(n: f64) -> Value {
    let n = (n * 100.0).round() / 100.0;
    if n.fract() == 0.0 {
        json!(n as i64)
    } else {
        json!(n)
    }
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
        for row in f["sprints"].as_array().into_iter().flatten() {
            out += &format!(
                "  {} {} to {}: {} points\n",
                plain(&row["id"]),
                plain(&row["start"]),
                plain(&row["end"]),
                plain(&row["completed_points"]),
            );
        }
        for key in ["last_3", "last_6"] {
            if f[key].is_object() {
                out += &format!(
                    "  {key}: mean {}, min {}, max {} over {} sprints\n",
                    plain(&f[key]["mean"]),
                    plain(&f[key]["min"]),
                    plain(&f[key]["max"]),
                    plain(&f[key]["sprints"]),
                );
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
