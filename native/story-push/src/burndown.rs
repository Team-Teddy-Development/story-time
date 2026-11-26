//! `story burndown --sprint ID [--today D] [--json]` and `story burnup (--epic ID | --ids A,B)
//! --from D --to D [--json]`: remaining points per day for a sprint, and cumulative points done
//! against scope for a set of stories. Read-only. Scope, adds, drops and points come from
//! `sprint::derive`, so these figures cannot disagree with `story sprint show`.

use crate::metrics::{self, number, History};
use crate::sprint::{self, Calendar, Move};
use crate::{config, list, Failure};
use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const BURNDOWN_USAGE: &str =
    "Usage: story burndown --sprint <YYYY-Snn> [--today YYYY-MM-DD] [--json]";
const BURNUP_USAGE: &str =
    "Usage: story burnup (--epic ID | --ids A,B) --from YYYY-MM-DD --to YYYY-MM-DD [--json]";
const SPRINT_SOURCE: &str = "changelog `set sprint=`, `set size=` and `set status=` lines in the backlog story files, day resolution, with sprint dates from settings team.sprintAnchor and team.sprints";
const BURNUP_SOURCE: &str = "changelog `set size=` and `set status=` lines in the backlog story files, day resolution; `epic` from the story front matter";

fn usage(message: impl std::fmt::Display, text: &str) -> Failure {
    Failure {
        code: 2,
        message: format!("{message}\n{text}"),
    }
}

fn date_flag(flags: &config::Flags, key: &str, text: &str) -> Result<Option<NaiveDate>, Failure> {
    flags
        .value(key)
        .map(|raw| {
            sprint::parse_date(raw)
                .ok_or_else(|| usage(format!("--{key} must be a YYYY-MM-DD date: {raw}"), text))
        })
        .transpose()
}

fn days(from: NaiveDate, to: NaiveDate) -> impl Iterator<Item = NaiveDate> {
    (0..=(to - from).num_days()).map(move |i| from + Duration::days(i))
}

fn render(report: &Value, flags: &config::Flags, text: fn(&Value) -> String) -> String {
    if flags.has("json") {
        format!("{}\n", serde_json::to_string_pretty(report).unwrap())
    } else {
        text(report)
    }
}

pub fn run_burndown(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
    let id = flags
        .value("sprint")
        .ok_or_else(|| usage("--sprint is required.", BURNDOWN_USAGE))?;
    if !sprint::valid_id(id) {
        return Err(usage(
            format!("sprint must be YYYY-Snn, got \"{id}\"."),
            BURNDOWN_USAGE,
        ));
    }
    let given = date_flag(&flags, "today", BURNDOWN_USAGE)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let calendar = Calendar::from_settings(&settings)
        .ok_or("no sprint calendar in settings (team.sprintAnchor or team.sprints).")?;
    let today = given.unwrap_or_else(|| today_in(calendar.tz()));
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let stories = metrics::histories(&dir)?;
    let report = report(&stories, &calendar, id, today)?;
    Ok(render(&report, &flags, burndown_text))
}

pub(crate) fn report(
    stories: &[History],
    calendar: &Calendar,
    id: &str,
    today: NaiveDate,
) -> Result<Value, String> {
    let sprint = calendar.resolve(id)?;
    if sprint.end < sprint.start {
        return Err(format!(
            "{id} ends ({}) before it starts ({}).",
            sprint.end, sprint.start
        ));
    }
    Ok(burndown(stories, calendar, &sprint, today))
}

pub(crate) fn today_in(tz: Option<&str>) -> NaiveDate {
    match tz {
        Some(tz) => chrono::Utc::now()
            .with_timezone(&tz.parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::UTC))
            .date_naive(),
        None => chrono::Local::now().date_naive(),
    }
}

fn burndown(
    stories: &[History],
    calendar: &Calendar,
    sprint: &sprint::Sprint,
    today: NaiveDate,
) -> Value {
    let trusted: Vec<&History> = stories.iter().filter(|s| s.gap.is_none()).collect();
    let derived = sprint::derive(sprint, trusted.iter().map(|s| (s.id.as_str(), &s.log)));

    let mut unknown: BTreeMap<&str, String> = stories
        .iter()
        .filter(|s| {
            s.log
                .sprint
                .iter()
                .any(|(_, v)| v.as_deref() == Some(&sprint.id))
        })
        .filter_map(|s| Some((s.id.as_str(), s.gap.clone()?)))
        .collect();
    let no_size = "no size when it entered the sprint";

    let mut entered: BTreeMap<&str, NaiveDate> = BTreeMap::new();
    let mut scope_steps: Vec<(NaiveDate, f64)> = Vec::new();
    for member in &derived.committed {
        entered.insert(&member.id, sprint.start);
        match member.points {
            Some(p) => scope_steps.push((sprint.start, p)),
            None => {
                unknown.insert(&member.id, no_size.to_owned());
            }
        }
    }
    for change in &derived.changes {
        let sign = match change.kind {
            Move::Add => {
                entered.insert(&change.id, change.date);
                1.0
            }
            Move::Drop { .. } => -1.0,
        };
        match change.points {
            Some(p) => scope_steps.push((change.date, sign * p)),
            None => {
                unknown.insert(&change.id, no_size.to_owned());
            }
        }
    }
    let done_steps: Vec<(NaiveDate, f64)> = derived
        .done
        .iter()
        .filter_map(|d| Some((d.date.max(*entered.get(d.id.as_str())?), d.points?)))
        .collect();

    let sum_to = |steps: &[(NaiveDate, f64)], day: NaiveDate| -> f64 {
        steps
            .iter()
            .filter(|(d, _)| *d <= day)
            .map(|(_, p)| p)
            .sum()
    };
    let span: Vec<NaiveDate> = days(sprint.start, sprint.end).collect();
    let opening = sum_to(&scope_steps, sprint.start);
    let last = (span.len() - 1) as f64;
    let rows: Vec<Value> = span
        .iter()
        .enumerate()
        .map(|(i, day)| {
            let scope = sum_to(&scope_steps, *day);
            let ideal = if last == 0.0 { 0.0 } else { opening * (last - i as f64) / last };
            json!({
                "day": day.to_string(),
                "scope": number(scope),
                "remaining": if *day <= today { number(scope - sum_to(&done_steps, *day)) } else { Value::Null },
                "ideal": number(ideal),
                "weekend": sprint::is_weekend(*day),
                "today": *day == today,
            })
        })
        .collect();

    json!({
        "metric": "burndown",
        "units": "points",
        "window": format!("{}..{}", sprint.start, sprint.end),
        "source": SPRINT_SOURCE,
        "definition": "scope: points of the stories in the sprint on that day, each as sized when it entered, with adds and drops applied on their dated day; remaining: scope minus the points of stories done on or before that day, null after today; ideal: a straight line from the opening scope to 0 on the last day; a story with no size when it entered is unknown and counts as nothing",
        "sprint": {
            "id": sprint.id,
            "start": sprint.start.to_string(),
            "end": sprint.end.to_string(),
            "goal": sprint.goal,
            "tz": calendar.tz(),
        },
        "as_of": today.to_string(),
        "unknown": unknown.iter().map(|(id, reason)| json!({"id": id, "reason": reason})).collect::<Vec<_>>(),
        "rows": rows,
    })
}

fn plain(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
}

fn table(report: &Value, head: &str, columns: &[&str]) -> String {
    let mut out = format!("{head}\n  {}\n", columns.join("  "));
    for row in report["rows"].as_array().unwrap() {
        let cells: Vec<String> = columns
            .iter()
            .map(|c| match &row[*c] {
                Value::Bool(true) => (*c).to_owned(),
                Value::Bool(false) | Value::Null => "-".to_owned(),
                v => plain(v),
            })
            .collect();
        out += &format!("  {}\n", cells.join("  "));
    }
    for gap in report["unknown"].as_array().unwrap() {
        out += &format!("unknown {}: {}\n", plain(&gap["id"]), plain(&gap["reason"]));
    }
    out
}

fn burndown_text(report: &Value) -> String {
    table(
        report,
        &format!(
            "Burndown {}, {}, points, as of {}",
            plain(&report["sprint"]["id"]),
            plain(&report["window"]),
            plain(&report["as_of"]),
        ),
        &["day", "scope", "remaining", "ideal", "weekend", "today"],
    )
}

pub fn run_burnup(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
    let need = |key: &str| {
        date_flag(&flags, key, BURNUP_USAGE)?
            .ok_or_else(|| usage(format!("--{key} is required."), BURNUP_USAGE))
    };
    let (epic, ids) = (flags.value("epic"), flags.value("ids"));
    if epic.is_some() == ids.is_some() {
        return Err(usage("Give exactly one of --epic and --ids.", BURNUP_USAGE));
    }
    let (from, to) = (need("from")?, need("to")?);
    if from > to {
        return Err(usage(
            format!("--from {from} is after --to {to}."),
            BURNUP_USAGE,
        ));
    }
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let stories = metrics::histories(&dir)?;
    let members: Vec<&History> = match (epic, ids) {
        (Some(epic), _) => {
            let found: Vec<_> = stories
                .iter()
                .filter(|s| s.epic.as_deref() == Some(epic))
                .collect();
            if found.is_empty() {
                return Err(format!("no story has epic {epic}.").into());
            }
            found
        }
        (_, Some(ids)) => ids
            .split(',')
            .map(str::trim)
            .map(|id| {
                stories
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| Failure::from(format!("no story {id}.")))
            })
            .collect::<Result<_, _>>()?,
        _ => unreachable!(),
    };
    let label = epic.map_or_else(
        || format!("ids {}", ids.unwrap_or_default()),
        |e| format!("epic {e}"),
    );
    let report = burnup(&members, &label, from, to);
    Ok(render(&report, &flags, burnup_text))
}

fn points_on(story: &History, day: NaiveDate) -> Option<f64> {
    if story.log.size.is_empty() {
        story.size
    } else {
        sprint::points_at(&story.log, day)
    }
}

fn done_on(story: &History, day: NaiveDate) -> bool {
    let mut events: Vec<_> = story.log.status.iter().filter(|(d, _)| *d <= day).collect();
    events.sort_by_key(|(d, _)| *d);
    events.last().is_some_and(|(_, s)| s == "done")
}

fn burnup(members: &[&History], label: &str, from: NaiveDate, to: NaiveDate) -> Value {
    let mut unknown: BTreeMap<&str, String> = BTreeMap::new();
    let mut counted: Vec<&History> = Vec::new();
    for story in members {
        if let Some(gap) = &story.gap {
            unknown.insert(&story.id, gap.clone());
        } else if days(from, to).all(|d| points_on(story, d).is_none()) {
            unknown.insert(&story.id, "no size".to_owned());
        } else {
            counted.push(story);
        }
    }
    let rows: Vec<Value> = days(from, to)
        .map(|day| {
            let sized: Vec<(&&History, f64)> = counted
                .iter()
                .filter_map(|s| Some((s, points_on(s, day)?)))
                .collect();
            let scope: f64 = sized.iter().map(|(_, p)| p).sum();
            let done: f64 = sized
                .iter()
                .filter(|(s, _)| done_on(s, day))
                .map(|(_, p)| p)
                .sum();
            json!({"day": day.to_string(), "scope": number(scope), "done": number(done)})
        })
        .collect();
    let stories: BTreeSet<&str> = members.iter().map(|s| s.id.as_str()).collect();
    json!({
        "metric": "burnup",
        "units": "points",
        "window": format!("{from}..{to}"),
        "source": BURNUP_SOURCE,
        "definition": "scope: points of the stories in the set on that day, each as last sized on or before it; done: the same points for stories whose last status event on or before that day is done; a story with no size is unknown and counts as nothing",
        "set": label,
        "stories": stories,
        "unknown": unknown.iter().map(|(id, reason)| json!({"id": id, "reason": reason})).collect::<Vec<_>>(),
        "rows": rows,
    })
}

fn burnup_text(report: &Value) -> String {
    table(
        report,
        &format!(
            "Burnup {}, {}, points",
            plain(&report["set"]),
            plain(&report["window"]),
        ),
        &["day", "scope", "done"],
    )
}
