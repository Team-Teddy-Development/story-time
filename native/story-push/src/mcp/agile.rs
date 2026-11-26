//! `story_velocity`, `story_burndown` and `story_forecast`: the read-only tools of the delivery
//! figures (ST-197). Each one runs the code of its CLI command on the repository's story files
//! and returns that command's JSON, so a tool and its command cannot disagree.

use super::{fail, invalid, strings, with_redactions, Repo, Server, ToolError};
use crate::{burndown, forecast, metrics, Failure};
use serde_json::{json, Map, Value};

type Command = fn(&[String]) -> Result<String, Failure>;

/// Runs `command` on the story files of the repository the call names, with the settings and
/// backlog the server resolved for it, and parses the JSON it prints.
fn report(repo: &Repo, command: Command, flags: Vec<(&str, String)>) -> Result<Value, ToolError> {
    let settings = [crate::config::FILE, crate::config::LEGACY_FILE]
        .map(|file| repo.root.path.join(file))
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| repo.root.path.join(crate::config::FILE));
    let mut argv = vec![
        "--config".to_owned(),
        settings.to_string_lossy().into_owned(),
        "--dir".to_owned(),
        repo.dir.to_string_lossy().into_owned(),
        "--json".to_owned(),
    ];
    for (flag, value) in flags {
        argv.push(format!("--{flag}"));
        argv.push(value);
    }
    let text = command(&argv).map_err(|failure| {
        if failure.code == 2 {
            invalid("", repo.strip(&failure.message))
        } else {
            fail("report_failed", &repo.strip(&failure.message))
        }
    })?;
    serde_json::from_str(&text).map_err(|_| fail("report_failed", "The report is not JSON."))
}

fn text<'a>(args: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn given<'a>(args: &'a Map<String, Value>, keys: &[&'a str]) -> Vec<(&'a str, String)> {
    keys.iter()
        .filter_map(|key| Some((*key, text(args, key)?.to_owned())))
        .collect()
}

pub(super) fn story_velocity(
    server: &Server,
    args: &Map<String, Value>,
) -> Result<Value, ToolError> {
    let repo = server.open(args)?;
    let all = report(&repo, metrics::run, given(args, &["from", "to"]))?;
    let velocity = all["figures"]
        .as_array()
        .and_then(|figures| figures.iter().find(|f| f["metric"] == "velocity"))
        .cloned()
        .ok_or_else(|| fail("report_failed", "The metrics have no velocity figure."))?;
    Ok(repo.at(velocity))
}

pub(super) fn story_burndown(
    server: &Server,
    args: &Map<String, Value>,
) -> Result<Value, ToolError> {
    let repo = server.open(args)?;
    Ok(repo.at(report(
        &repo,
        burndown::run_burndown,
        given(args, &["sprint", "today"]),
    )?))
}

pub(super) fn story_forecast(
    server: &Server,
    args: &Map<String, Value>,
) -> Result<Value, ToolError> {
    let repo = server.open(args)?;
    let mut flags = given(args, &["sprint", "epic", "today"]);
    if let Some(ids) = strings(args, "ids") {
        for id in &ids {
            repo.check_id(id)?;
        }
        flags.push(("ids", ids.join(",")));
    }
    if let Some(seed) = args.get("seed").and_then(Value::as_u64) {
        flags.push(("seed", seed.to_string()));
    }
    Ok(repo.at(report(&repo, forecast::run, flags)?))
}

fn nullable_number() -> Value {
    json!({"type": ["number", "null"]})
}

fn unknown() -> Value {
    json!({"type": "array", "items": {"type": "object", "required": ["id", "reason"]}})
}

pub(super) fn velocity_schema() -> Value {
    let stat = json!({"type": ["object", "string"]});
    with_redactions(json!({
        "type": "object",
        "required": [
            "metric", "value", "units", "window", "source", "definition",
            "closed_sprints", "sprints", "last_3", "last_6", "unknown"
        ],
        "properties": {
            "metric": {"type": "string", "enum": ["velocity"]},
            "value": {"type": ["number", "string"]},
            "units": {"type": "string"},
            "window": {"type": "string"},
            "source": {"type": "string"},
            "definition": {"type": "string"},
            "closed_sprints": {"type": "integer", "minimum": 0},
            "sprints": {"type": "array", "items": {
                "type": "object",
                "required": ["id", "start", "end", "completed_points"],
            }},
            "last_3": stat,
            "last_6": stat,
            "unknown": unknown(),
        }
    }))
}

pub(super) fn burndown_schema() -> Value {
    with_redactions(json!({
        "type": "object",
        "required": [
            "metric", "units", "window", "source", "definition", "sprint", "as_of", "unknown", "rows"
        ],
        "properties": {
            "metric": {"type": "string", "enum": ["burndown"]},
            "units": {"type": "string"},
            "window": {"type": "string"},
            "source": {"type": "string"},
            "definition": {"type": "string"},
            "sprint": {"type": "object", "required": ["id", "start", "end"]},
            "as_of": {"type": "string"},
            "unknown": unknown(),
            "rows": {"type": "array", "items": {
                "type": "object",
                "required": ["day", "scope", "remaining", "ideal", "weekend", "today"],
                "properties": {
                    "day": {"type": "string"},
                    "scope": {"type": "number"},
                    "remaining": nullable_number(),
                    "ideal": {"type": "number"},
                    "weekend": {"type": "boolean"},
                    "today": {"type": "boolean"},
                }
            }},
        }
    }))
}

pub(super) fn forecast_schema() -> Value {
    with_redactions(json!({
        "type": "object",
        "required": [
            "metric", "units", "window", "source", "definition", "target", "today",
            "remaining_points", "sample_source", "sample_count", "runs", "seed", "rows", "unknown"
        ],
        "properties": {
            "metric": {"type": "string", "enum": ["forecast"]},
            "units": {"type": "string"},
            "window": {"type": "string"},
            "source": {"type": "string"},
            "definition": {"type": "string"},
            "target": {"type": "object"},
            "today": {"type": "string"},
            "remaining_points": {"type": "number"},
            "sample_source": {"type": "string"},
            "sample_count": {"type": "integer", "minimum": 0},
            "runs": {"type": "integer", "minimum": 1},
            "seed": {"type": "integer", "minimum": 0},
            "rows": {"type": "array", "items": {
                "type": "object",
                "required": ["percentile", "date"],
                "properties": {
                    "percentile": {"type": "integer"},
                    "date": {"type": "string"},
                    "reason": {"type": "string"},
                }
            }},
            "unknown": unknown(),
        }
    }))
}
