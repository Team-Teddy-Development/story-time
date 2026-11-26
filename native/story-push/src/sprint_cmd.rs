//! `story sprint show <id> [--dir D] [--config C] [--json]`: what one sprint committed to, how its
//! scope changed, and what it finished, from changelog events. Read-only.

use crate::metrics::{self, number};
use crate::sprint::{self, Calendar, Derived, Move};
use crate::{config, list, Failure};
use serde_json::{json, Value};

const USAGE: &str = "Usage: story sprint show <YYYY-Snn> [--json]";
const SOURCE: &str = "changelog `set sprint=`, `set size=` and `set status=` lines in the backlog story files, day resolution";

pub fn run(args: &[String]) -> Result<String, Failure> {
    let usage = |message: String| Failure {
        code: 2,
        message: format!("{message}\n{USAGE}"),
    };
    let flags = config::Flags::parse(args)?;
    let id = match flags.positional.as_slice() {
        [sub, id] if sub == "show" => id.as_str(),
        [sub] if sub == "show" => return Err(usage("A sprint id is required.".into())),
        [sub, ..] if sub != "show" => return Err(usage(format!("Unknown sprint command: {sub}"))),
        _ => return Err(usage("Expected `show <id>`.".into())),
    };
    if !sprint::valid_id(id) {
        return Err(usage(format!("sprint must be YYYY-Snn, got \"{id}\".")));
    }
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let calendar = Calendar::from_settings(&settings)
        .ok_or("no sprint calendar in settings (team.sprintAnchor or team.sprints).".to_owned())?;
    let sprint = calendar.resolve(id)?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let stories = metrics::histories(&dir)?;
    let trusted: Vec<_> = stories.iter().filter(|s| s.gap.is_none()).collect();
    let derived = sprint::derive(&sprint, trusted.iter().map(|s| (s.id.as_str(), &s.log)));
    let mut unknown: Vec<Value> = stories
        .iter()
        .filter(|s| s.log.sprint.iter().any(|(_, v)| v.as_deref() == Some(id)))
        .filter_map(|s| Some(json!({"id": s.id, "reason": s.gap.as_ref()?})))
        .collect();
    let total = |rows: &[Option<f64>], label: &str, unknown: &mut Vec<Value>, ids: Vec<&str>| {
        let missing: Vec<&&str> = ids
            .iter()
            .zip(rows)
            .filter(|(_, p)| p.is_none())
            .map(|(id, _)| id)
            .collect();
        for id in &missing {
            unknown.push(json!({"id": id, "reason": format!("no size at commitment, so the {label} points are unknown")}));
        }
        if missing.is_empty() {
            number(rows.iter().flatten().sum())
        } else {
            json!("unknown")
        }
    };
    let points = |p: Option<f64>| p.map_or(json!("unknown"), number);
    let committed_total = total(
        &derived
            .committed
            .iter()
            .map(|m| m.points)
            .collect::<Vec<_>>(),
        "committed",
        &mut unknown,
        derived.committed.iter().map(|m| m.id.as_str()).collect(),
    );
    let done_total = total(
        &derived.done.iter().map(|d| d.points).collect::<Vec<_>>(),
        "done",
        &mut unknown,
        derived.done.iter().map(|d| d.id.as_str()).collect(),
    );
    let report = json!({
        "sprint": {
            "id": sprint.id,
            "start": sprint.start.to_string(),
            "end": sprint.end.to_string(),
            "goal": sprint.goal,
            "tz": calendar.tz(),
            "source": if sprint.overridden { "team.sprints" } else { "team.sprintAnchor" },
        },
        "source": SOURCE,
        "committed": {
            "stories": derived.committed.iter().map(|m| json!({"id": m.id, "points": points(m.points)})).collect::<Vec<_>>(),
            "points": committed_total,
        },
        "changes": derived.changes.iter().map(|c| {
            let mut row = json!({"date": c.date.to_string(), "id": c.id, "change": match c.kind { Move::Add => "add", Move::Drop { .. } => "drop" }});
            if let Move::Drop { to: Some(to) } = &c.kind {
                row["to"] = json!(to);
            }
            row["points"] = points(c.points);
            row
        }).collect::<Vec<_>>(),
        "done": {
            "stories": derived.done.iter().map(|d| json!({"id": d.id, "date": d.date.to_string(), "points": points(d.points)})).collect::<Vec<_>>(),
            "points": done_total,
        },
        "unknown": unknown,
    });
    Ok(if flags.has("json") {
        format!("{}\n", serde_json::to_string_pretty(&report).unwrap())
    } else {
        text(&report, &derived)
    })
}

fn text(report: &Value, derived: &Derived) -> String {
    let plain = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_owned);
    let s = &report["sprint"];
    let mut out = format!(
        "Sprint {}, {} to {}, source {}\n",
        plain(&s["id"]),
        plain(&s["start"]),
        plain(&s["end"]),
        plain(&s["source"]),
    );
    if let Some(goal) = s["goal"].as_str() {
        out += &format!("Goal: {goal}\n");
    }
    out += &format!(
        "Committed: {} points\n",
        plain(&report["committed"]["points"])
    );
    for row in report["committed"]["stories"].as_array().unwrap() {
        out += &format!("  {} {}\n", plain(&row["id"]), plain(&row["points"]));
    }
    out += "Changes:\n";
    for row in report["changes"].as_array().unwrap() {
        let to = row["to"]
            .as_str()
            .map_or(String::new(), |to| format!(" to {to}"));
        out += &format!(
            "  {} {} {}{to} ({} points)\n",
            plain(&row["date"]),
            plain(&row["change"]),
            plain(&row["id"]),
            plain(&row["points"]),
        );
    }
    if derived.changes.is_empty() {
        out += "  none\n";
    }
    out += &format!("Done: {} points\n", plain(&report["done"]["points"]));
    for row in report["done"]["stories"].as_array().unwrap() {
        out += &format!(
            "  {} {} ({} points)\n",
            plain(&row["date"]),
            plain(&row["id"]),
            plain(&row["points"]),
        );
    }
    for gap in report["unknown"].as_array().unwrap() {
        out += &format!("unknown {}: {}\n", plain(&gap["id"]), plain(&gap["reason"]));
    }
    out
}
