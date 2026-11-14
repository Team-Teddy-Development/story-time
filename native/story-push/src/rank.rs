//! `story rank`: Weighted Shortest Job First over the ready stories, as Node's `rankBacklog` ranks them.

use crate::{
    config::{self, Flags},
    js_number,
    lenient_story::Js,
    list, stats, table, Failure,
};
use std::cmp::Ordering;

const FINISHED: [&str; 3] = ["done", "split", "archived"];
const MISSING: &str = "missing WSJF inputs (value, time_criticality, risk_reduction, size)";

struct Ranked<'a> {
    story: &'a Js,
    cost_of_delay: f64,
    score: f64,
}

struct Excluded<'a> {
    story: &'a Js,
    reason: String,
}

fn is_status(value: &Js, statuses: &[&str]) -> bool {
    matches!(value, Js::Str(s) if statuses.contains(&s.as_str()))
}

/// SameValueZero, the key equality of a JavaScript `Map`; objects and arrays never match a copy.
fn same_value_zero(a: &Js, b: &Js) -> bool {
    match (a, b) {
        (Js::Num(x), Js::Num(y)) => x == y || (x.is_nan() && y.is_nan()),
        (Js::Arr(_) | Js::Obj(_), _) | (_, Js::Arr(_) | Js::Obj(_)) => false,
        _ => a == b,
    }
}

fn finite(value: &Js) -> Option<f64> {
    match value {
        Js::Num(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

/// `Math.round`: halves round toward +Infinity.
fn math_round(x: f64) -> f64 {
    let up = x.ceil();
    if up - 0.5 > x {
        up - 1.0
    } else {
        up
    }
}

fn wsjf(story: &Js) -> Option<(f64, f64)> {
    let parts = ["value", "time_criticality", "risk_reduction"]
        .map(|key| finite(story.get(key)))
        .into_iter()
        .collect::<Option<Vec<_>>>()?;
    let size = finite(story.get("size")).filter(|size| *size > 0.0)?;
    let cost_of_delay = parts.iter().fold(0.0, |a, b| a + b);
    Some((
        cost_of_delay,
        math_round(cost_of_delay / size * 100.0) / 100.0,
    ))
}

fn rank_backlog<'a>(
    stories: &'a [Js],
    statuses: &[&str],
) -> Result<(Vec<Ranked<'a>>, Vec<Excluded<'a>>), String> {
    let by_id = |id: &Js| {
        stories
            .iter()
            .rev()
            .find(|s| same_value_zero(s.get("id"), id))
    };
    let mut ranked = Vec::new();
    let mut excluded = Vec::new();
    for story in stories {
        let status = story.get("status");
        if is_status(status, &FINISHED) {
            continue;
        }
        if !is_status(status, statuses) {
            excluded.push(Excluded {
                story,
                reason: format!("status {}", status.string()),
            });
            continue;
        }
        let depends_on = match story.get("depends_on") {
            Js::Undefined => &[][..],
            Js::Arr(ids) => ids,
            _ => return Err("(s.depends_on ?? []).filter is not a function".into()),
        };
        let blockers: Vec<String> = depends_on
            .iter()
            .filter(|id| !by_id(id).is_some_and(|s| is_status(s.get("status"), &["done"])))
            .map(|id| match id {
                Js::Undefined => String::new(),
                id => id.string(),
            })
            .collect();
        if !blockers.is_empty() {
            excluded.push(Excluded {
                story,
                reason: format!("blocked by {}", blockers.join(", ")),
            });
            continue;
        }
        let Some((cost_of_delay, score)) = wsjf(story) else {
            excluded.push(Excluded {
                story,
                reason: MISSING.into(),
            });
            continue;
        };
        ranked.push(Ranked {
            story,
            cost_of_delay,
            score,
        });
    }
    let created = |r: &Ranked| match r.story.get("created") {
        Js::Undefined => String::new(),
        created => created.string(),
    };
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                finite(a.story.get("size"))
                    .partial_cmp(&finite(b.story.get("size")))
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| {
                list::collation_key(&created(a), false)
                    .cmp(&list::collation_key(&created(b), false))
            })
    });
    Ok((ranked, excluded))
}

/// `Number.prototype.toFixed(2)`.
fn to_fixed(x: f64) -> String {
    if !x.is_finite() || x.abs() >= 1e21 {
        return js_number::to_string(x);
    }
    if x == 0.0 {
        return "0.00".into();
    }
    format!("{x:.2}")
}

/// `Array.prototype.slice(0, Number(limit))`'s end.
fn slice_end(limit: &str, len: usize) -> usize {
    let n = stats::number(limit);
    if n.is_nan() {
        return 0;
    }
    let n = n.trunc();
    if n < 0.0 {
        (len as f64 + n).max(0.0) as usize
    } else {
        n.min(len as f64) as usize
    }
}

fn obj(entries: Vec<(&str, Js)>) -> Js {
    Js::Obj(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
}

/// `story rank [--limit N] [--include-refined] [--dir D] [--config C] [--json]`.
pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(&flags, &loaded.root, &settings)?;
    let statuses: &[&str] = if flags.has("include-refined") {
        &["ready", "refined"]
    } else {
        &["ready"]
    };
    let stories = list::stories(&dir, None)?;
    let (mut ranked, excluded) = rank_backlog(&stories, statuses)?;
    if let Some(limit) = flags.value("limit").filter(|l| !l.is_empty()) {
        ranked.truncate(slice_end(limit, ranked.len()));
    }
    if flags.has("json") {
        let ranked = ranked
            .iter()
            .enumerate()
            .map(|(i, r)| {
                obj(vec![
                    ("rank", Js::Num((i + 1) as f64)),
                    ("id", r.story.get("id").clone()),
                    ("title", r.story.get("title").clone()),
                    ("wsjf", Js::Num(r.score)),
                    ("costOfDelay", Js::Num(r.cost_of_delay)),
                    ("size", r.story.get("size").clone()),
                ])
            })
            .collect();
        let excluded = excluded
            .iter()
            .map(|e| {
                obj(vec![
                    ("id", e.story.get("id").clone()),
                    ("title", e.story.get("title").clone()),
                    ("reason", Js::Str(e.reason.clone())),
                ])
            })
            .collect();
        let json = obj(vec![
            ("ranked", Js::Arr(ranked)),
            ("excluded", Js::Arr(excluded)),
        ]);
        return Ok(format!("{}\n", json.json()));
    }
    let mut out = if ranked.is_empty() {
        "No ready, unblocked, scored stories.\n".to_owned()
    } else {
        let rows = ranked
            .iter()
            .enumerate()
            .map(|(i, r)| {
                vec![
                    (i + 1).to_string(),
                    r.story.get("id").string(),
                    to_fixed(r.score),
                    js_number::to_string(r.cost_of_delay),
                    r.story.get("size").string(),
                    r.story.get("title").string(),
                ]
            })
            .collect();
        table(&["#", "ID", "WSJF", "CoD", "Size", "Title"], rows)
    };
    if !excluded.is_empty() {
        out.push_str("\nExcluded:\n");
        for e in &excluded {
            out.push_str(&format!(
                "  {}  {}  ({})\n",
                e.story.get("id").string(),
                e.reason,
                e.story.get("title").string()
            ));
        }
    }
    Ok(out)
}
