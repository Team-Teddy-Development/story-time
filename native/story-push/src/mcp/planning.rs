use super::{
    fail, invalid, object, strings, Field, Kind, Repo, Server, Tool, ToolError, READ, REPOSITORY,
};
use crate::{
    burndown, forecast, metrics,
    sprint::{self, Calendar},
};
use chrono::{NaiveDate, Utc};
use serde_json::{json, Map, Value};

const FROM: Field = Field {
    name: "from",
    kind: Kind::NonEmpty(10),
};
const TO: Field = Field {
    name: "to",
    kind: Kind::NonEmpty(10),
};
const TODAY: Field = Field {
    name: "today",
    kind: Kind::NonEmpty(10),
};
const SPRINT: Field = Field {
    name: "sprint",
    kind: Kind::NonEmpty(8),
};
const EPIC: Field = Field {
    name: "epic",
    kind: Kind::NonEmpty(64),
};
const IDS: Field = Field {
    name: "ids",
    kind: Kind::List(&Kind::NonEmpty(64), 100),
};
const SEED: Field = Field {
    name: "seed",
    kind: Kind::Integer {
        min: 0,
        max: 9_007_199_254_740_991,
        default: forecast::DEFAULT_SEED as i64,
    },
};

pub(super) const VELOCITY: Tool = Tool {
    name: "story_velocity",
    description: "Completed points per closed sprint, with the last three and six sprint statistics from story metrics.",
    hints: READ,
    fields: &[REPOSITORY, FROM, TO],
    required: &["from", "to"],
    output: velocity_schema,
    run: velocity,
};
pub(super) const BURNDOWN: Tool = Tool {
    name: "story_burndown",
    description:
        "Remaining points and scope by day for one sprint. Pass today to reproduce a previous view.",
    hints: READ,
    fields: &[REPOSITORY, SPRINT, TODAY],
    required: &["sprint"],
    output: burndown_schema,
    run: burndown,
};
pub(super) const FORECAST: Tool = Tool {
    name: "story_forecast",
    description: "Completion date percentiles for exactly one sprint, epic, or list of story IDs. Uses historical velocity.",
    hints: READ,
    fields: &[REPOSITORY, SPRINT, EPIC, IDS, TODAY, SEED],
    required: &[],
    output: forecast_schema,
    run: forecast,
};

pub(super) fn is_tool(name: &str) -> bool {
    matches!(name, "story_velocity" | "story_burndown" | "story_forecast")
}

type Args = Map<String, Value>;

fn date(args: &Args, key: &str) -> Result<Option<NaiveDate>, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(|raw| {
            sprint::parse_date(raw).ok_or_else(|| {
                invalid(
                    &format!("/{key}"),
                    format!("/{key} must be a YYYY-MM-DD date."),
                )
            })
        })
        .transpose()
}

fn sprint_id(raw: &str) -> Result<&str, ToolError> {
    if sprint::valid_id(raw) {
        Ok(raw)
    } else {
        Err(invalid("/sprint", "/sprint must be YYYY-Snn.".into()))
    }
}

fn calendar(repo: &Repo) -> Result<Calendar, ToolError> {
    Calendar::from_settings(&repo.settings).ok_or_else(|| {
        fail(
            "no_sprint_calendar",
            "The repository has no sprint calendar.",
        )
    })
}

fn histories(repo: &Repo) -> Result<Vec<metrics::History>, ToolError> {
    metrics::histories_with(
        &repo.dir,
        |id| {
            repo.bytes(id)?
                .map(|bytes| {
                    String::from_utf8(bytes).map_err(|_| super::io_error("decode story history"))
                })
                .transpose()
        },
        |_| super::io_error("list story histories"),
    )
}

fn velocity(server: &Server, args: &Args) -> Result<Value, ToolError> {
    let (from, to) = (date(args, "from")?.unwrap(), date(args, "to")?.unwrap());
    if from > to {
        return Err(invalid("/from", "/from must be on or before /to.".into()));
    }
    let repo = server.open(args)?;
    let figure = metrics::velocity(
        &histories(&repo)?,
        Calendar::from_settings(&repo.settings).as_ref(),
        from,
        to,
    );
    Ok(repo.at(figure))
}

fn burndown(server: &Server, args: &Args) -> Result<Value, ToolError> {
    let id = sprint_id(args["sprint"].as_str().unwrap())?;
    let given = date(args, "today")?;
    let repo = server.open(args)?;
    let calendar = calendar(&repo)?;
    let today = given.unwrap_or_else(|| burndown::today_in(calendar.tz()));
    let report = burndown::report(&histories(&repo)?, &calendar, id, today)
        .map_err(|message| invalid("/sprint", message))?;
    Ok(repo.at(report))
}

fn forecast(server: &Server, args: &Args) -> Result<Value, ToolError> {
    let target = match (
        args.get("sprint").and_then(Value::as_str),
        args.get("epic").and_then(Value::as_str),
        strings(args, "ids"),
    ) {
        (Some(id), None, None) => forecast::Target::Sprint(sprint_id(id)?.into()),
        (None, Some(epic), None) => forecast::Target::Epic(epic.into()),
        (None, None, Some(ids)) if !ids.is_empty() && ids.iter().all(|id| !id.contains(',')) => {
            let mut seen = std::collections::BTreeSet::new();
            forecast::Target::Ids(
                ids.into_iter()
                    .filter(|id| seen.insert(id.clone()))
                    .collect(),
            )
        }
        _ => {
            return Err(invalid(
                "",
                "Give exactly one of /sprint, /epic, or a nonempty /ids list.".into(),
            ))
        }
    };
    let today = date(args, "today")?.unwrap_or_else(|| Utc::now().date_naive());
    let seed = args
        .get("seed")
        .and_then(Value::as_u64)
        .unwrap_or(forecast::DEFAULT_SEED);
    let repo = server.open(args)?;
    let report = forecast::report(&histories(&repo)?, &calendar(&repo)?, &target, today, seed)
        .map_err(|error| invalid("", error.message))?;
    Ok(repo.at(report))
}

fn figure_schema(metric: &str, required: &[&str], fields: Value) -> Value {
    let mut properties = json!({
        "metric": {"const": metric}, "units": {"type": "string"}, "window": {"type": "string"},
        "source": {"type": "string"}, "definition": {"type": "string"}, "unknown": {"type": "array"},
        "redactions": {"type": "integer", "minimum": 1},
    });
    properties
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let mut keys = vec![
        "metric",
        "units",
        "window",
        "source",
        "definition",
        "unknown",
    ];
    keys.extend_from_slice(required);
    object(&keys, properties)
}

fn velocity_schema() -> Value {
    figure_schema(
        "velocity",
        &["value", "closed_sprints", "sprints", "last_3", "last_6"],
        json!({
            "value": {"type": ["number", "string"]}, "closed_sprints": {"type": "integer"},
            "sprints": {"type": "array"}, "last_3": {"type": ["object", "string"]}, "last_6": {"type": ["object", "string"]}
        }),
    )
}

fn burndown_schema() -> Value {
    figure_schema(
        "burndown",
        &["sprint", "as_of", "rows"],
        json!({
            "sprint": {"type": "object"}, "as_of": {"type": "string"}, "rows": {"type": "array"}
        }),
    )
}

fn forecast_schema() -> Value {
    figure_schema(
        "forecast",
        &[
            "target",
            "today",
            "remaining_points",
            "sample_source",
            "sample_count",
            "runs",
            "seed",
            "rows",
        ],
        json!({
            "target": {"type": "object"}, "today": {"type": "string"}, "remaining_points": {"type": "number"},
            "sample_source": {"type": "string"}, "sample_count": {"type": "integer"}, "runs": {"type": "integer"},
            "seed": {"type": "integer"}, "rows": {"type": "array"}
        }),
    )
}
