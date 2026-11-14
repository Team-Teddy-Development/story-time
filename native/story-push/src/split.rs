//! `story split <id> --from children.json`: Node's `splitStory`, which writes each child and then
//! marks the parent split as a saga, removing the children it wrote when a later step fails.

use crate::{
    config::{self, js_string, truthy, Flags},
    lenient_story::{append_changelog, Js},
    list,
    next_id::{format_id, number},
    Failure,
};
use chrono::Utc;
use serde_json::Value;
use std::{fs, io::ErrorKind, path::Path};

const UNSPLITTABLE: [&str; 3] = ["done", "split", "archived"];

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let (id, from) = match (flags.positional.first(), flags.value("from")) {
        (Some(id), Some(from)) if !id.is_empty() && !from.is_empty() => (id, from),
        _ => {
            return Err(Failure {
                code: 2,
                message: "Usage: story split <id> --from children.json".into(),
            })
        }
    };
    let text = fs::read(from).map_err(|e| match e.kind() {
        ErrorKind::NotFound => format!("ENOENT: no such file or directory, open '{from}'"),
        _ => format!("{from}: {e}"),
    })?;
    let specs: Value =
        serde_json::from_str(&String::from_utf8_lossy(&text)).map_err(|e| e.to_string())?;
    let child_ids = split(&dir, id, &specs, &Utc::now().format("%Y-%m-%d").to_string())?;
    Ok(if flags.has("json") {
        let ids = child_ids.iter().map(|c| Js::Str(c.clone())).collect();
        let result = Js::Obj(vec![
            ("ok".into(), Js::Bool(true)),
            ("parentId".into(), Js::Str(id.clone())),
            ("childIds".into(), Js::Arr(ids)),
        ]);
        format!("{}\n", result.json())
    } else {
        format!("{id} split into {}.\n", child_ids.join(", "))
    })
}

/// `splitStory` followed by the CLI's failure message: the child ids, or the error Node prints.
fn split(dir: &Path, parent_id: &str, specs: &Value, date: &str) -> Result<Vec<String>, String> {
    let failed = |message: String| format!("Split failed: {message}");
    let specs = match specs {
        Value::Array(specs) if specs.len() >= 2 => specs,
        _ => return Err(failed("A split needs at least two child stories.".into())),
    };
    let parent = list::get(dir, parent_id)?
        .ok_or_else(|| failed(format!("Story {parent_id} not found.")))?;
    if let Js::Str(status) = parent.get("status") {
        if UNSPLITTABLE.contains(&status.as_str()) {
            return Err(failed(format!("Cannot split {parent_id}: it is {status}.")));
        }
    }

    let prefix = parent_id.split('-').next().unwrap_or_default();
    let highest = list::stories(dir, None)?
        .iter()
        .filter_map(|s| number(&s.get("id").string(), prefix))
        .fold(0.0, f64::max);
    let children = specs
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            child(
                &parent,
                spec,
                format_id(prefix, highest + 1.0 + i as f64),
                date,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let child_ids: Vec<String> = children.iter().map(|c| c.get("id").string()).collect();

    let mut marked = parent.props().to_vec();
    Js::set(&mut marked, "status", Js::Str("split".into()));
    let ids = child_ids.iter().map(|c| Js::Str(c.clone())).collect();
    Js::set(&mut marked, "children", Js::Arr(ids));
    let marked = append_changelog(
        &Js::Obj(marked),
        &format!("split into {}", child_ids.join(", ")),
        date,
    );

    for (written, child) in children.iter().enumerate() {
        if let Err(error) = list::save(dir, child) {
            let step = format!("create {}", child_ids[written]);
            return Err(roll_back(dir, &step, &error, &child_ids[..written]));
        }
    }
    if let Err(error) = list::save(dir, &marked) {
        let step = format!("mark {parent_id} split");
        return Err(roll_back(dir, &step, &error, &child_ids));
    }
    Ok(child_ids)
}

/// Removes the written children, newest first, and names any that could not be removed.
fn roll_back(dir: &Path, step: &str, error: &str, written: &[String]) -> String {
    let lost: Vec<String> = written
        .iter()
        .rev()
        .filter_map(|id| match fs::remove_file(dir.join(format!("{id}.md"))) {
            Err(e) if e.kind() != ErrorKind::NotFound => Some(format!("create {id} ({e})")),
            _ => None,
        })
        .collect();
    let lost = if lost.is_empty() {
        String::new()
    } else {
        format!("\nCould not roll back: {}", lost.join("; "))
    };
    format!("Split failed at \"{step}\": {error}{lost}")
}

/// `buildChild`: a draft story from one child spec, inheriting the parent's epic, and its type
/// and persona when the spec has none.
fn child(parent: &Js, spec: &Value, id: String, date: &str) -> Result<Js, String> {
    if spec.is_null() {
        return Err("Cannot read properties of null (reading 'acceptance')".into());
    }
    let field = |key: &str| {
        spec.as_object()
            .and_then(|o| o.get(key))
            .filter(|v| !v.is_null())
    };
    let or = |key: &str, fallback: Js| field(key).map_or(fallback, frontmatter);
    let acceptance = match field("acceptance") {
        None => String::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|a| format!("- [ ] {}", js_string(a)))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(_) => return Err("(spec.acceptance ?? []).map is not a function".into()),
    };
    let mut sections = vec![(
        "Story".to_owned(),
        Js::Str(field("statement").map(js_string).unwrap_or_default()),
    )];
    if !acceptance.is_empty() {
        sections.push(("Acceptance Criteria".into(), Js::Str(acceptance)));
    }
    if truthy(field("notes")) {
        sections.push(("Notes".into(), Js::Str(js_string(&spec["notes"]))));
    }
    sections.push((
        "Changelog".into(),
        Js::Str(format!(
            "- {date} created by splitting {}",
            parent.get("id").string()
        )),
    ));
    let inherited = |key: &str, fallback: Js| match parent.get(key) {
        Js::Undefined => fallback,
        value => value.clone(),
    };
    Ok(Js::Obj(vec![
        ("id".into(), Js::Str(id)),
        ("title".into(), or("title", Js::Undefined)),
        ("status".into(), Js::Str("draft".into())),
        (
            "type".into(),
            or("type", inherited("type", Js::Str("story".into()))),
        ),
        ("epic".into(), parent.get("epic").clone()),
        (
            "persona".into(),
            or("persona", inherited("persona", Js::Undefined)),
        ),
        ("value".into(), or("value", Js::Undefined)),
        (
            "time_criticality".into(),
            or("time_criticality", Js::Undefined),
        ),
        ("risk_reduction".into(), or("risk_reduction", Js::Undefined)),
        ("size".into(), or("size", Js::Undefined)),
        ("depends_on".into(), or("depends_on", Js::Arr(Vec::new()))),
        ("parent".into(), parent.get("id").clone()),
        ("created".into(), Js::Str(date.into())),
        ("updated".into(), Js::Str(date.into())),
        ("sections".into(), Js::Obj(sections)),
    ]))
}

/// A spec value as `formatValue` prints it. A null reaches here only inside an array, where
/// `formatValue(null)` gives the quoted text "null", as the string "null" does.
fn frontmatter(value: &Value) -> Js {
    match value {
        Value::Null => Js::Str("null".into()),
        Value::Bool(b) => Js::Bool(*b),
        Value::Number(n) => Js::Num(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => Js::Str(s.clone()),
        Value::Array(items) => Js::Arr(items.iter().map(frontmatter).collect()),
        Value::Object(_) => Js::Obj(Vec::new()),
    }
}
