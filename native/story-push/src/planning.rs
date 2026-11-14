use chrono::{DateTime, NaiveDate, Utc};
use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
};

const USAGE: &str = "Usage: story plan --from request.json [--output docs/planning/name.json]\n       story plan --snapshot\n\nNative, local-only planning. --snapshot emits a request template with current source hashes.\nNo output option writes JSON to stdout only. Output writes JSON and its Markdown counterpart.\nExisting outputs require reviewed SHA-256 preimages for BOTH paths in output_preimages.\n";
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FileImage {
    path: String,
    sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity {
    root: String,
    head: Option<String>,
    origin: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    repository: Identity,
    captured_at: String,
    config: Option<FileImage>,
    backlog_dir: String,
    files: Vec<FileImage>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Calendar {
    start: Option<String>,
    end: Option<String>,
    time_zone: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capacity {
    budget: f64,
    reserve: f64,
    unit: String,
    source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EpicEvidence {
    disposition: EpicDisposition,
    epic: Option<String>,
    evidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum EpicDisposition {
    Mapped,
    JustifiedStandalone,
    UnresolvedEpic,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Forecast {
    label: String,
    ids: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanRequest {
    schema_version: u32,
    plan_id: String,
    source: Source,
    candidate_order: Vec<String>,
    goal: Option<String>,
    calendar: Option<Calendar>,
    capacity: Option<Capacity>,
    #[serde(default)]
    epic_evidence: BTreeMap<String, EpicEvidence>,
    #[serde(default)]
    forecasts: Vec<Forecast>,
    #[serde(default)]
    exclusions: BTreeMap<String, String>,
    #[serde(default)]
    output_preimages: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Debug)]
struct Story {
    id: String,
    status: Status,
    size: Option<f64>,
    dependencies: Vec<String>,
    epic: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Draft,
    Refined,
    Ready,
    InProgress,
    Done,
    Archived,
    Split,
}
impl Status {
    fn open(self) -> bool {
        !matches!(self, Self::Done | Self::Archived | Self::Split)
    }
}
struct Snapshot {
    source: Source,
    stories: BTreeMap<String, Story>,
    config: Value,
}
#[derive(Serialize)]
struct Missing {
    field: String,
    reason: String,
}
#[derive(Serialize)]
struct Disposition {
    id: String,
    selection: String,
    reasons: Vec<String>,
    size: Option<f64>,
    dependencies: Vec<Value>,
    epic: EpicEvidence,
}
#[derive(Serialize)]
struct PlanArtifact {
    schema_version: u32,
    plan_id: String,
    source: Source,
    goal: Option<String>,
    calendar: Value,
    capacity: Value,
    dispositions: Vec<Disposition>,
    selected: Vec<String>,
    dependency_order: Vec<String>,
    assessment: String,
    missing_inputs: Vec<Missing>,
    forecasts: Vec<Value>,
    publication: Value,
    wip_policy: Value,
}

fn hash(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
const POINT_SCALE: u64 = 1_000_000;
fn point_units(value: f64) -> Result<u64> {
    let invalid = || {
        "point_precision: use nonnegative points <= 1000000000 with at most six decimal places"
            .to_owned()
    };
    if !value.is_finite() || !(0.0..=1_000_000_000.0).contains(&value) {
        return Err(invalid());
    }
    let decimal = value.to_string();
    let (whole, fraction) = decimal.split_once('.').unwrap_or((&decimal, ""));
    if fraction.len() > 6 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    let whole = whole.parse::<u64>().map_err(|_| invalid())?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().map_err(|_| invalid())? * 10_u64.pow(6 - fraction.len() as u32)
    };
    Ok(whole * POINT_SCALE + fraction)
}
fn points(units: u64) -> f64 {
    units as f64 / POINT_SCALE as f64
}
fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}
fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty()
        || Path::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!(
            "unsafe_path: expected repository-relative path: {relative}"
        ));
    }
    let mut path = root.to_owned();
    for component in Path::new(relative).components() {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_symlink() => {
                return Err(format!("symlink_path: {relative}"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("path_io: {relative}: {e}")),
        }
    }
    Ok(path)
}
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let result = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).trim().to_owned())
}
fn identity(root: &Path) -> Identity {
    Identity {
        root: root.to_string_lossy().into_owned(),
        head: git(root, &["rev-parse", "HEAD"]),
        origin: git(root, &["remote", "get-url", "origin"])
            .map(|url| format!("sha256:{}", hash(url.as_bytes()))),
    }
}
fn image(root: &Path, path: &str) -> Result<(FileImage, Vec<u8>)> {
    let dest = safe_path(root, path)?;
    let bytes = fs::read(dest).map_err(|e| format!("source_io: {path}: {e}"))?;
    Ok((
        FileImage {
            path: path.into(),
            sha256: hash(&bytes),
        },
        bytes,
    ))
}
fn optional_text(value: &Value, name: &str) -> Result<Option<String>> {
    match value.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if nonempty(s) => Ok(Some(s.clone())),
        _ => Err(format!(
            "malformed_data: {name} must be a nonempty string or null"
        )),
    }
}
fn parse_story(bytes: &[u8], path: &str, prefix: &str) -> Result<Story> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| format!("malformed_story: {path}: invalid UTF-8"))?;
    let value = crate::lenient_story::parse(text)
        .map_err(|e| format!("malformed_story: {path}: {e}"))?
        .to_value();
    let id = optional_text(&value, "id")?
        .ok_or_else(|| format!("malformed_story: {path}: missing id"))?;
    let suffix = id.strip_prefix(&format!("{prefix}-"));
    if !suffix.is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())) {
        return Err(format!(
            "malformed_story: {path}: id does not match configured prefix {prefix}"
        ));
    }
    let status = match value["status"].as_str() {
        Some("draft") => Status::Draft,
        Some("refined") => Status::Refined,
        Some("ready") => Status::Ready,
        Some("in-progress") => Status::InProgress,
        Some("done") => Status::Done,
        Some("archived") => Status::Archived,
        Some("split") => Status::Split,
        _ => {
            return Err(format!(
                "malformed_story: {path}: unknown or missing status"
            ))
        }
    };
    let size = match value.get("size") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_f64()
                .filter(|n| n.is_finite() && *n > 0.0)
                .ok_or_else(|| {
                    format!("unknown_size: {id}: size must be positive points or null")
                })?,
        ),
    };
    if let Some(size) = size {
        point_units(size).map_err(|cause| format!("unknown_size: {id}: {cause}"))?;
    }
    let dependencies = match value.get("depends_on") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .filter(|s| nonempty(s))
                    .map(str::to_owned)
                    .ok_or_else(|| format!("malformed_story: {id}: depends_on must contain IDs"))
            })
            .collect::<Result<Vec<_>>>()?,
        _ => {
            return Err(format!(
                "malformed_story: {id}: depends_on must be an array"
            ))
        }
    };
    if dependencies.iter().collect::<BTreeSet<_>>().len() != dependencies.len() {
        return Err(format!("duplicate_dependency: {id}"));
    }
    Ok(Story {
        id,
        status,
        size,
        dependencies,
        epic: optional_text(&value, "epic")?,
    })
}
fn snapshot(root: &Path) -> Result<Snapshot> {
    let mut configs = Vec::new();
    for name in [".story-time/settings.json", ".story-time.json"] {
        let path = safe_path(root, name)?;
        if path.exists() {
            configs.push(name);
        }
    }
    if configs.len() > 1 {
        return Err("config_conflict: both current and legacy settings exist".into());
    }
    let (config_image, config) = if let Some(name) = configs.first() {
        let (file, bytes) = image(root, name)?;
        let config: Value = crate::js_number::parse(&String::from_utf8_lossy(&bytes))
            .map_err(|e| format!("malformed_config: {e}"))?;
        (Some(file), config)
    } else {
        (None, json!({}))
    };
    let config = crate::config::parse(&config).map_err(|e| format!("malformed_config: {e}"))?;
    let backlog_dir = crate::config::js_string(&config["backlogDir"]);
    let prefix = crate::config::js_string(&config["prefix"]);
    let dir = safe_path(root, &backlog_dir)?;
    let mut paths = Vec::new();
    match fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| format!("source_io: {e}"))?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "unsafe_path: non-UTF8 backlog filename")?;
                let relative = format!("{backlog_dir}/{name}");
                safe_path(root, &relative)?;
                if name.ends_with(".md") {
                    if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                        return Err(format!("malformed_story: not a regular file: {relative}"));
                    }
                    paths.push(relative);
                } else if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    return Err(format!(
                        "unsupported_layout: nested backlog directory: {relative}"
                    ));
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("source_io: {e}")),
    }
    paths.sort();
    let mut files = Vec::new();
    let mut stories = BTreeMap::new();
    for path in paths {
        let (file, bytes) = image(root, &path)?;
        if !path.ends_with(".map.md") {
            let story = parse_story(&bytes, &path, &prefix)?;
            if stories.insert(story.id.clone(), story).is_some() {
                return Err(format!("duplicate_id: {path}"));
            }
        }
        files.push(file);
    }
    fn visit(
        id: &str,
        stories: &BTreeMap<String, Story>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.into()) {
            return Err(format!("dependency_cycle: {id}"));
        }
        for dep in &stories[id].dependencies {
            if stories.contains_key(dep) {
                visit(dep, stories, active, done)?;
            }
        }
        active.remove(id);
        done.insert(id.into());
        Ok(())
    }
    let mut done = BTreeSet::new();
    for id in stories.keys() {
        visit(id, &stories, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(Snapshot {
        source: Source {
            repository: identity(root),
            captured_at: Utc::now().to_rfc3339(),
            config: config_image,
            backlog_dir,
            files,
        },
        stories,
        config,
    })
}
fn fresh(expected: &Source, current: &Source) -> Result<()> {
    if expected.repository != current.repository
        || expected.config != current.config
        || expected.backlog_dir != current.backlog_dir
        || expected.files != current.files
    {
        return Err("stale_source: repository, configuration, backlog membership, or file hashes changed; capture and review again".into());
    }
    DateTime::parse_from_rfc3339(&expected.captured_at)
        .map_err(|_| "malformed_data: source.captured_at requires RFC3339")?;
    Ok(())
}
fn validate(request: &PlanRequest, snapshot: &Snapshot) -> Result<()> {
    if request.schema_version != 1 {
        return Err("unsupported_schema: schema_version must be 1".into());
    }
    if !nonempty(&request.plan_id) {
        return Err("malformed_data: plan_id must be nonempty".into());
    }
    fresh(&request.source, &snapshot.source)?;
    if request
        .candidate_order
        .iter()
        .collect::<BTreeSet<_>>()
        .len()
        != request.candidate_order.len()
    {
        return Err("duplicate_candidate: candidate_order must contain unique IDs".into());
    }
    for id in &request.candidate_order {
        if !snapshot.stories.get(id).is_some_and(|s| s.status.open()) {
            return Err(format!(
                "unknown_candidate: {id}: expected open source story"
            ));
        }
    }
    if let Some(capacity) = &request.capacity {
        if !capacity.budget.is_finite()
            || !capacity.reserve.is_finite()
            || capacity.budget < 0.0
            || capacity.reserve < 0.0
            || capacity.reserve > capacity.budget
        {
            return Err(
                "invalid_capacity: require 0 <= reserve <= budget with finite numbers".into(),
            );
        }
        point_units(capacity.budget).map_err(|cause| format!("invalid_capacity: {cause}"))?;
        point_units(capacity.reserve).map_err(|cause| format!("invalid_capacity: {cause}"))?;
        if capacity.unit != "points" || !nonempty(&capacity.source) {
            return Err("invalid_capacity: unit must be points and source must be nonempty".into());
        }
    }
    if let Some(calendar) = &request.calendar {
        let parse_date = |value: &Option<String>| -> Result<Option<NaiveDate>> {
            value
                .as_ref()
                .map(|s| {
                    NaiveDate::parse_from_str(s, "%Y-%m-%d")
                        .map_err(|_| "invalid_calendar: use YYYY-MM-DD".into())
                })
                .transpose()
        };
        let start = parse_date(&calendar.start)?;
        let end = parse_date(&calendar.end)?;
        if start.zip(end).is_some_and(|(s, e)| s > e) {
            return Err("invalid_calendar: end precedes start".into());
        }
        if calendar.time_zone.as_ref().is_some_and(|s| !nonempty(s)) {
            return Err("invalid_calendar: time_zone must be nonempty or null".into());
        }
    }
    for (id, evidence) in &request.epic_evidence {
        let story = snapshot
            .stories
            .get(id)
            .filter(|s| s.status.open())
            .ok_or_else(|| format!("unknown_epic_story: {id}"))?;
        if !nonempty(&evidence.evidence) {
            return Err(format!("missing_epic_evidence: {id}"));
        }
        match evidence.disposition {
            EpicDisposition::Mapped if evidence.epic.as_ref().is_none_or(|s| !nonempty(s)) => {
                return Err(format!("missing_epic: {id}"))
            }
            EpicDisposition::JustifiedStandalone
                if evidence.epic.is_some() || story.epic.is_some() =>
            {
                return Err(format!(
                    "epic_conflict: {id}: standalone conflicts with epic"
                ))
            }
            _ => {}
        }
        if story.epic.is_some() && evidence.epic.is_some() && story.epic != evidence.epic {
            return Err(format!("epic_conflict: {id}"));
        }
    }
    for (id, reason) in &request.exclusions {
        if !snapshot.stories.get(id).is_some_and(|s| s.status.open()) || !nonempty(reason) {
            return Err(format!(
                "invalid_exclusion: {id}: expected open ID and evidence reason"
            ));
        }
    }
    let mut forecast_ids = BTreeSet::new();
    for forecast in &request.forecasts {
        if !nonempty(&forecast.label) {
            return Err("malformed_forecast: label is required".into());
        }
        for id in &forecast.ids {
            if !snapshot.stories.get(id).is_some_and(|s| s.status.open())
                || !forecast_ids.insert(id)
            {
                return Err(format!(
                    "malformed_forecast: unknown or repeated open ID {id}"
                ));
            }
        }
    }
    Ok(())
}
fn plan(request: &PlanRequest, snapshot: &Snapshot) -> PlanArtifact {
    let mut missing = Vec::new();
    let mut need = |field: &str, reason: &str| {
        missing.push(Missing {
            field: field.into(),
            reason: reason.into(),
        })
    };
    if request.goal.as_ref().is_none_or(|s| !nonempty(s)) {
        need(
            "goal",
            "No sprint goal supplied; story titles do not establish a goal",
        );
    }
    if request.capacity.is_none() {
        need(
            "capacity",
            "No point budget and reserve supplied; velocity is not availability",
        );
    }
    for (field, value) in [
        (
            "calendar.start",
            request.calendar.as_ref().and_then(|c| c.start.as_ref()),
        ),
        (
            "calendar.end",
            request.calendar.as_ref().and_then(|c| c.end.as_ref()),
        ),
        (
            "calendar.time_zone",
            request.calendar.as_ref().and_then(|c| c.time_zone.as_ref()),
        ),
    ] {
        if value.is_none() {
            need(
                field,
                "Not supplied; sprintDays alone cannot establish dates or time zone",
            );
        }
    }
    if snapshot.source.config.is_none() {
        need("source.config", "No configuration; tracker, prefix and backlog settings use local compatibility defaults");
    }
    let mut selected = Vec::new();
    let mut total = 0_u64;
    let mut dispositions = Vec::new();
    let mut order = request.candidate_order.clone();
    order.extend(
        snapshot
            .stories
            .values()
            .filter(|s| s.status.open() && !request.candidate_order.contains(&s.id))
            .map(|s| s.id.clone()),
    );
    for id in order {
        let story = &snapshot.stories[&id];
        let dependencies: Vec<Value> = story.dependencies.iter().map(|dep| {
            json!({"id":dep,"done":snapshot.stories.get(dep).is_some_and(|s| s.status == Status::Done),"source":"current backlog snapshot"})
        }).collect();
        let blocked: Vec<_> = dependencies
            .iter()
            .filter(|d| d["done"] == false)
            .map(|d| d["id"].as_str().unwrap())
            .collect();
        let (selection, reasons) = if let Some(reason) = request.exclusions.get(&id) {
            need(&format!("eligibility.{id}"), reason);
            ("blocked", vec![reason.clone()])
        } else if !blocked.is_empty() {
            (
                "blocked",
                vec![format!(
                    "Dependencies must already be done: {}",
                    blocked.join(", ")
                )],
            )
        } else if story.status != Status::Ready {
            (
                "needs-refinement",
                vec!["Source status is not ready; only story-ready establishes readiness".into()],
            )
        } else if let Some(size) = story.size {
            if !request.candidate_order.contains(&id) {
                ("deferred", vec!["Not in reviewed candidate order".into()])
            } else if let Some(capacity) = &request.capacity {
                let size_units = point_units(size).expect("validated source size");
                let available = point_units(capacity.budget).expect("validated budget")
                    - point_units(capacity.reserve).expect("validated reserve")
                    - total;
                if size_units <= available {
                    total += size_units;
                    selected.push(id.clone());
                    (
                        "selected",
                        vec![
                            "Ready in source, dependencies done, fits reviewed point budget".into(),
                        ],
                    )
                } else {
                    (
                        "deferred",
                        vec!["Does not fit remaining capacity after reserve".into()],
                    )
                }
            } else {
                ("deferred", vec!["No capacity; no commitment".into()])
            }
        } else {
            need(
                &format!("size.{id}"),
                "Ready story has unknown size and cannot be selected",
            );
            (
                "needs-refinement",
                vec!["Unknown size excludes selection".into()],
            )
        };
        let epic = request
            .epic_evidence
            .get(&id)
            .cloned()
            .unwrap_or_else(|| EpicEvidence {
                disposition: EpicDisposition::UnresolvedEpic,
                epic: story.epic.clone(),
                evidence: if story.epic.is_some() {
                    "Source epic link exists; confirm its definition with story-map".into()
                } else {
                    "Which epic outcome owns this story, or what justifies standalone work?".into()
                },
            });
        dispositions.push(Disposition {
            id,
            selection: selection.into(),
            reasons,
            size: story.size,
            dependencies,
            epic,
        });
    }
    let tracker = snapshot.config["tracker"].as_str().unwrap_or("files");
    if tracker != "files" {
        need("publication.tracker_sprint_fields", "Configured tracker adapter has no sprint-field publication contract; proposal stays local and provisional");
    }
    let publication = json!({"mode":"local-only","tracker":tracker,"writes":[],"sprint_fields_supported":false,"unsupported_operations":["tracker sprint assignment","sprint start or close","tracker epic writes"],"reason":"Issue mirroring is not a sprint service; no tracker calls are made"});
    let capacity = request.capacity.as_ref().map_or(Value::Null, |c| json!({"budget":c.budget,"reserve":c.reserve,"unit":c.unit,"source":c.source,"selected_points":points(total),"remaining":points(point_units(c.budget).expect("validated budget")-point_units(c.reserve).expect("validated reserve")-total)}));
    let calendar = json!({"sprint_days":snapshot.config["team"]["sprintDays"],"start":request.calendar.as_ref().and_then(|c| c.start.as_ref()),"end":request.calendar.as_ref().and_then(|c| c.end.as_ref()),"time_zone":request.calendar.as_ref().and_then(|c| c.time_zone.as_ref())});
    let forecasts = request.forecasts.iter().map(|f| json!({"kind":"forecast","label":f.label,"ids":f.ids,"commitment":false,"dates":null})).collect();
    PlanArtifact {
        schema_version: 1,
        plan_id: request.plan_id.clone(),
        source: request.source.clone(),
        goal: request.goal.clone(),
        calendar,
        capacity,
        dispositions,
        dependency_order: selected.clone(),
        selected,
        assessment: if missing.is_empty() {
            "planned"
        } else {
            "provisional"
        }
        .into(),
        missing_inputs: missing,
        forecasts,
        publication,
        wip_policy: json!({"limit_per_person":snapshot.config["team"]["wipLimit"],"meaning":"Pull policy, not a limit on planned story count","people":null,"availability":null}),
    }
}
fn markdown(artifact: &PlanArtifact) -> String {
    let mut text = format!("# Plan {}\n\nAssessment: {}. Local proposal only; no sprint starts or tracker assignments.\n\nGoal: {}\n\n## Capacity and calendar\n\n```json\n{}\n{}\n```\n\n## Dispositions\n\n", artifact.plan_id, artifact.assessment, artifact.goal.as_deref().unwrap_or("Unknown; see missing inputs"), artifact.capacity, artifact.calendar);
    for row in &artifact.dispositions {
        text.push_str(&format!(
            "- {}: {}. {} Epic: {}\n",
            row.id,
            row.selection,
            row.reasons.join("; "),
            serde_json::to_string(&row.epic).unwrap()
        ));
    }
    text.push_str("\n## Missing inputs\n\n");
    for item in &artifact.missing_inputs {
        text.push_str(&format!("- {}: {}\n", item.field, item.reason));
    }
    text.push_str(&format!("\n## Forecasts\n\n{}\n\n## Publication and WIP\n\n{}\n\n{}\n\n## Source\n\n```json\n{}\n```\n",json!(artifact.forecasts),artifact.publication,artifact.wip_policy,serde_json::to_string_pretty(&artifact.source).unwrap()));
    text
}
fn output_paths(root: &Path, output: &str) -> Result<[PathBuf; 2]> {
    let relative = Path::new(output);
    if !relative.starts_with("docs/planning")
        || relative.extension().is_none_or(|s| s != "json")
        || relative.file_name().is_none()
    {
        return Err("unsafe_output: use docs/planning/<name>.json".into());
    }
    let json = safe_path(root, output)?;
    let md = relative.with_extension("md");
    Ok([
        json,
        safe_path(root, md.to_str().ok_or("unsafe_output: invalid UTF-8")?)?,
    ])
}
fn protect_sources(root: &Path, source: &Source, paths: &[PathBuf; 2]) -> Result<()> {
    let backlog = safe_path(root, &source.backlog_dir)?;
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "unsafe_output: outside root")?;
        if path.starts_with(&backlog)
            || source
                .config
                .as_ref()
                .is_some_and(|file| relative == Path::new(&file.path))
            || source
                .files
                .iter()
                .any(|file| relative == Path::new(&file.path))
        {
            return Err(format!("source_output_overlap: {} is an authoritative input; choose a planning output outside the configured backlog", relative.display()));
        }
    }
    Ok(())
}
fn output_preimages(
    root: &Path,
    paths: &[PathBuf; 2],
    reviewed: &BTreeMap<String, Option<String>>,
) -> Result<[Option<Vec<u8>>; 2]> {
    let names: Vec<_> = paths
        .iter()
        .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
        .collect();
    if reviewed.keys().any(|key| !names.contains(key)) {
        return Err(
            "unexpected_output_preimage: only the JSON and Markdown target are allowed".into(),
        );
    }
    let mut originals = [None, None];
    let mut any_existing = false;
    for (i, path) in paths.iter().enumerate() {
        safe_path(root, &names[i])?;
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_file() => {
                originals[i] = Some(fs::read(path).map_err(|e| format!("output_io: {e}"))?);
                any_existing = true;
            }
            Ok(_) => return Err(format!("output_conflict: not a regular file: {}", names[i])),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("output_io: {e}")),
        }
    }
    if any_existing || !reviewed.is_empty() {
        for i in 0..2 {
            let current = originals[i].as_ref().map(|bytes| hash(bytes));
            if reviewed.get(&names[i]) != Some(&current) {
                return Err(format!(
                    "output_conflict: exact reviewed preimage required for BOTH outputs: {}",
                    names[i]
                ));
            }
        }
    }
    Ok(originals)
}
fn publish(
    root: &Path,
    request: &PlanRequest,
    artifact: &PlanArtifact,
    output: &str,
) -> Result<()> {
    let paths = output_paths(root, output)?;
    protect_sources(root, &request.source, &paths)?;
    let originals = output_preimages(root, &paths, &request.output_preimages)?;
    let content = [
        format!("{}\n", serde_json::to_string_pretty(artifact).unwrap()).into_bytes(),
        markdown(artifact).into_bytes(),
    ];
    let parent = paths[0].parent().unwrap();
    let existed = parent.exists();
    fs::create_dir_all(parent).map_err(|e| format!("output_io: {e}"))?;
    let lock = parent.join(".story-plan.lock");
    let mut lock_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|e| {
            format!("output_lock: {e}; remove a stale lock only after checking its owner")
        })?;
    let staged = paths.each_ref().map(|p| {
        p.with_file_name(format!(
            ".{}.{}.tmp",
            p.file_name().unwrap().to_string_lossy(),
            std::process::id()
        ))
    });
    let mut owned_staged = Vec::new();
    let result = (|| {
        writeln!(lock_file, "pid={}", std::process::id())
            .and_then(|()| lock_file.sync_all())
            .map_err(|e| format!("output_lock: {e}"))?;
        let current = snapshot(root)?;
        fresh(&request.source, &current.source)?;
        protect_sources(root, &current.source, &paths)?;
        output_preimages(root, &paths, &request.output_preimages)?;
        for i in 0..2 {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged[i])
                .map_err(|e| format!("output_stage: {e}"))?;
            owned_staged.push(staged[i].clone());
            file.write_all(&content[i])
                .and_then(|()| file.sync_all())
                .map_err(|e| format!("output_stage: {e}"))?;
        }
        for i in 0..2 {
            if let Err(e) = fs::rename(&staged[i], &paths[i]) {
                let mut recovery = Vec::new();
                for j in (0..i).rev() {
                    if let Err(error) = match &originals[j] {
                        Some(bytes) => fs::write(&paths[j], bytes),
                        None => fs::remove_file(&paths[j]),
                    } {
                        recovery.push(format!("{}: {error}", paths[j].display()));
                    }
                }
                return Err(format!(
                    "output_publish: {e}; rollback failures: {recovery:?}"
                ));
            }
        }
        Ok(())
    })();
    let mut cleanup_errors = Vec::new();
    for path in &owned_staged {
        if let Err(e) = fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                cleanup_errors.push(format!("{}: {e}", path.display()));
            }
        }
    }
    drop(lock_file);
    let cleanup = fs::remove_file(lock).map_err(|e| format!("output_cleanup: {e}"));
    if result.is_err() && !existed {
        let _ = fs::remove_dir(parent);
    }
    if !cleanup_errors.is_empty() {
        return Err(format!(
            "output_cleanup: {cleanup_errors:?}; publication result: {result:?}"
        ));
    }
    result.and(cleanup)
}

pub fn run(args: &[String]) -> (String, u8) {
    if args == ["--help"] || args == ["-h"] {
        return (USAGE.into(), 0);
    }
    let result = (|| -> Result<String> {
        let root = std::env::current_dir()
            .and_then(fs::canonicalize)
            .map_err(|e| format!("source_io: {e}"))?;
        if args == ["--snapshot"] {
            let snapshot = snapshot(&root)?;
            let request = PlanRequest {
                schema_version: 1,
                plan_id: "review-required".into(),
                candidate_order: snapshot
                    .stories
                    .values()
                    .filter(|s| s.status.open())
                    .map(|s| s.id.clone())
                    .collect(),
                source: snapshot.source,
                goal: None,
                calendar: None,
                capacity: None,
                epic_evidence: BTreeMap::new(),
                forecasts: vec![],
                exclusions: BTreeMap::new(),
                output_preimages: BTreeMap::new(),
            };
            return Ok(format!(
                "{}\n",
                serde_json::to_string_pretty(&request).unwrap()
            ));
        }
        let (from, output) = match args {
            [flag, from] if flag == "--from" => (from, None),
            [flag, from, out, output] if flag == "--from" && out == "--output" => {
                (from, Some(output))
            }
            _ => return Err(format!("invalid_arguments: {USAGE}")),
        };
        let bytes = fs::read(from).map_err(|e| format!("request_io: {e}"))?;
        let request: PlanRequest =
            serde_json::from_slice(&bytes).map_err(|e| format!("malformed_request: {e}"))?;
        let snapshot = snapshot(&root)?;
        validate(&request, &snapshot)?;
        let artifact = plan(&request, &snapshot);
        for forecast in &request.forecasts {
            if forecast.ids.iter().any(|id| artifact.selected.contains(id)) {
                return Err(
                    "malformed_forecast: later forecasts must not repeat selected IDs".into(),
                );
            }
        }
        if let Some(output) = output {
            publish(&root, &request, &artifact, output)?;
        } else if !request.output_preimages.is_empty() {
            return Err("unexpected_output_preimage: no --output target".into());
        }
        Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&artifact).unwrap()
        ))
    })();
    match result {
        Ok(text) => (text, 0),
        Err(cause) => (
            format!(
                "{}\n",
                json!({"status":"conflict","diagnostic":cause,"backlog_config_tracker_writes":[],"output_state":"Unchanged for input conflicts; inspect named targets after output I/O or cleanup failures","recovery":"Correct the named input or capture and review fresh preimages, then rerun the same native command"})
            ),
            2,
        ),
    }
}
