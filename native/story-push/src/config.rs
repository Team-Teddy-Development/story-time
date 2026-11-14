use crate::Failure;
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
};

pub const FILE: &str = ".story-time/settings.json";
pub const LEGACY_FILE: &str = ".story-time.json";
const TRACKERS: [&str; 6] = ["files", "github", "ado", "gitea", "jira", "linear"];
const PUSHABLE: [&str; 3] = ["github", "ado", "gitea"];
const SYNC_MODES: [&str; 3] = ["off", "ask", "auto"];
const ADO_PROCESSES: [&str; 3] = ["agile", "scrum", "basic"];
const KNOWN_KEYS: [&str; 15] = [
    "$schema",
    "tracker",
    "sync",
    "backlogDir",
    "prefix",
    "github",
    "ado",
    "gitea",
    "jira",
    "linear",
    "team",
    "project",
    "definitionOfDone",
    "definitionOfReady",
    "jev",
];

/// The settings file a command reads, before any command-specific validation.
pub struct Loaded {
    pub path: PathBuf,
    pub found: bool,
    pub raw: Value,
    pub root: PathBuf,
}

/// Makes `path` absolute against the working directory and removes `.` and `..`, like Node's `path.resolve`.
pub fn resolve(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let mut result = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => (),
            Component::ParentDir => {
                result.pop();
            }
            _ => result.push(part.as_os_str()),
        }
    }
    Ok(result)
}

/// --config, else $STORY_TIME_CONFIG, else ./.story-time/settings.json, else ./.story-time.json.
pub fn locate(flag: Option<&str>) -> Result<PathBuf, String> {
    let explicit = match flag {
        Some(flag) => flag.to_owned(),
        None => std::env::var("STORY_TIME_CONFIG").unwrap_or_default(),
    };
    if !explicit.is_empty() {
        return resolve(Path::new(&explicit));
    }
    let file = resolve(Path::new(FILE))?;
    if file.exists() {
        return Ok(file);
    }
    let legacy = resolve(Path::new(LEGACY_FILE))?;
    Ok(if legacy.exists() { legacy } else { file })
}

pub fn load(flag: Option<&str>) -> Result<Loaded, String> {
    let path = locate(flag)?;
    let (raw, found) = match fs::read_to_string(&path) {
        Ok(text) => (
            crate::js_number::parse(&text)
                .map_err(|_| format!("{}: invalid JSON configuration.", path.display()))?,
            true,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (json!({}), false),
        Err(e) => return Err(format!("Could not read configuration: {e}")),
    };
    let parent = path.parent().ok_or("Config has no parent.")?;
    let root = if parent.file_name().is_some_and(|p| p == ".story-time") {
        parent.parent().ok_or("Config has no project root.")?
    } else {
        parent
    }
    .to_owned();
    Ok(Loaded {
        path,
        found,
        raw,
        root,
    })
}

/// Arguments parsed the way the Node CLI parses them: a boolean flag takes no value, and every other `--flag` takes the next argument.
pub struct Flags {
    pub positional: Vec<String>,
    named: HashMap<String, Vec<Option<String>>>,
}
impl Flags {
    pub fn parse(args: &[String]) -> Result<Self, Failure> {
        const BOOLEAN: [&str; 6] = [
            "json",
            "force",
            "include-refined",
            "command",
            "all",
            "dry-run",
        ];
        let mut positional = Vec::new();
        let mut named = HashMap::new();
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let Some(key) = arg.strip_prefix("--") else {
                positional.push(arg.clone());
                continue;
            };
            if BOOLEAN.contains(&key) {
                named.insert(key.to_owned(), vec![None]);
                continue;
            }
            let value = args.next().ok_or_else(|| Failure {
                code: 2,
                message: format!("Flag --{key} needs a value."),
            })?;
            named
                .entry(key.to_owned())
                .or_insert_with(Vec::new)
                .push(Some(value.clone()));
        }
        Ok(Self { positional, named })
    }
    pub fn has(&self, key: &str) -> bool {
        self.named.contains_key(key)
    }
    /// The last value given for `key`, as Node keeps for a non-repeatable flag.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.named.get(key)?.last()?.as_deref()
    }
    /// Every value given for `key`, in order, as Node collects a repeatable flag such as `--ac`.
    pub fn values(&self, key: &str) -> Option<Vec<&str>> {
        self.named
            .get(key)
            .map(|v| v.iter().filter_map(Option::as_deref).collect())
    }
}

/// `story config [--config PATH] [--json]`: print the effective project config.
pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = load(flags.value("config"))?;
    let config = parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let text = crate::js_number::stringify(&config, true);
    Ok(if flags.has("json") {
        format!("{text}\n")
    } else if loaded.found {
        format!("# {}\n{text}\n", loaded.path.display())
    } else {
        format!("# no config file; defaults\n{text}\n")
    })
}

/// The one settings validator for config, push, and init: Node's `parseConfig` plus the tracker URL checks that keep secrets and path segments out of requests.
pub fn parse(raw: &Value) -> Result<Value, String> {
    let raw = js_value(raw);
    let object = raw.as_object().ok_or("Config must be a JSON object.")?;
    if let Some(key) = object.keys().find(|k| !KNOWN_KEYS.contains(&k.as_str())) {
        return Err(format!("Unknown config key \"{key}\"."));
    }
    let given = |key| present(object.get(key));

    let tracker = given("tracker").cloned().unwrap_or(json!("files"));
    let tracker = tracker
        .as_str()
        .filter(|t| TRACKERS.contains(t))
        .ok_or_else(|| {
            format!(
                "Unknown tracker \"{}\". Use one of: {}.",
                js_string(&tracker),
                TRACKERS.join(", ")
            )
        })?;

    let prefix = given("prefix").cloned().unwrap_or(json!("ST"));
    let prefix_text = text(&prefix, "prefix")?;
    if !is_prefix(prefix_text) {
        return Err(format!(
            "prefix must be uppercase letters/digits, e.g. \"ST\" or \"PAY\"; got \"{prefix_text}\"."
        ));
    }

    let sync = given("sync").cloned().unwrap_or(json!("off"));
    let sync = sync
        .as_str()
        .filter(|s| SYNC_MODES.contains(s))
        .ok_or_else(|| {
            format!(
                "sync must be one of: {}; got \"{}\".",
                SYNC_MODES.join(", "),
                js_string(&sync)
            )
        })?;
    if sync != "off" && !PUSHABLE.contains(&tracker) {
        return Err(format!("sync \"{sync}\" needs tracker github, ado, or gitea; tracker is \"{tracker}\". Use sync \"off\" and story export cards."));
    }

    let mut config = Map::new();
    config.insert("tracker".into(), json!(tracker));
    config.insert("sync".into(), json!(sync));
    config.insert(
        "backlogDir".into(),
        string(object.get("backlogDir"), "backlogDir")?.unwrap_or(json!("backlog")),
    );
    config.insert("prefix".into(), prefix);
    config.insert("team".into(), team(given("team").unwrap_or(&json!({})))?);
    config.insert(
        "project".into(),
        project(given("project").unwrap_or(&json!({})))?,
    );
    for key in ["definitionOfDone", "definitionOfReady"] {
        config.insert(key.into(), string_list(object.get(key), key)?);
    }
    let empty = json!({});
    if truthy(object.get("github")) || tracker == "github" {
        config.insert("github".into(), github(given("github").unwrap_or(&empty))?);
    }
    if truthy(object.get("ado")) || tracker == "ado" {
        config.insert("ado".into(), ado(given("ado").unwrap_or(&empty))?);
    }
    if truthy(object.get("gitea")) || tracker == "gitea" {
        config.insert("gitea".into(), gitea(given("gitea").unwrap_or(&empty))?);
    }
    for key in ["jira", "linear"] {
        if truthy(object.get(key)) {
            config.insert(key.into(), spread(&object[key]));
        }
    }
    if truthy(object.get("jev")) {
        config.insert("jev".into(), jev(&object["jev"])?);
    }
    Ok(Value::Object(config))
}

fn team(t: &Value) -> Result<Value, String> {
    let sprint_days = present(prop(t, "sprintDays")).cloned().unwrap_or(json!(10));
    let wip_limit = present(prop(t, "wipLimit")).cloned().unwrap_or(json!(1));
    if !positive_integer(&sprint_days) {
        return Err("team.sprintDays must be a positive integer.".into());
    }
    if !positive_integer(&wip_limit) {
        return Err("team.wipLimit must be a positive integer.".into());
    }
    let mut team = Map::new();
    team.insert("sprintDays".into(), sprint_days);
    if let Some(velocity) = prop(t, "velocity") {
        if !velocity
            .as_number()
            .is_some_and(|n| crate::js_number::value(n) > 0.0)
        {
            return Err("team.velocity must be a positive number of points per sprint.".into());
        }
        team.insert("velocity".into(), velocity.clone());
    }
    team.insert("wipLimit".into(), wip_limit);
    Ok(Value::Object(team))
}

fn project(p: &Value) -> Result<Value, String> {
    let mut project = Map::new();
    project.insert(
        "stack".into(),
        string_list(prop(p, "stack"), "project.stack")?,
    );
    for key in ["testFramework", "testCommand"] {
        if let Some(value) = string(prop(p, key), &format!("project.{key}"))? {
            project.insert(key.into(), value);
        }
    }
    Ok(Value::Object(project))
}

fn github(g: &Value) -> Result<Value, String> {
    let repo = prop(g, "repo");
    if !truthy(repo) {
        return Err("tracker \"github\" needs github.repo, e.g. \"acme/shop\".".into());
    }
    let text = text(repo.unwrap(), "github.repo")?;
    if !text
        .split_once('/')
        .is_some_and(|(owner, name)| is_segment(owner) && is_segment(name))
    {
        return Err(format!("github.repo must be owner/repo; got \"{text}\"."));
    }
    Ok(json!({ "repo": text }))
}

fn ado(a: &Value) -> Result<Value, String> {
    if !truthy(prop(a, "organization")) {
        return Err(
            "tracker \"ado\" needs ado.organization, e.g. \"https://dev.azure.com/acme\".".into(),
        );
    }
    if !truthy(prop(a, "project")) {
        return Err("tracker \"ado\" needs ado.project.".into());
    }
    let organization = text(&a["organization"], "ado.organization")?;
    let project = text(&a["project"], "ado.project")?;
    let process = present(prop(a, "process"))
        .cloned()
        .unwrap_or(json!("agile"));
    if !process.as_str().is_some_and(|p| ADO_PROCESSES.contains(&p)) {
        return Err(format!(
            "ado.process must be one of: {}; got \"{}\".",
            ADO_PROCESSES.join(", "),
            js_string(&process)
        ));
    }
    let mut ado = Map::new();
    ado.insert("organization".into(), json!(organization));
    ado.insert("project".into(), json!(project));
    ado.insert("process".into(), process);
    for key in ["areaPath", "iterationPath"] {
        if let Some(value) = string(prop(a, key), &format!("ado.{key}"))? {
            ado.insert(key.into(), value);
        }
    }
    Ok(Value::Object(ado))
}

fn gitea(g: &Value) -> Result<Value, String> {
    if let Some(key) = js_keys(g)
        .into_iter()
        .find(|k| !["baseUrl", "owner", "repo"].contains(&k.as_str()))
    {
        return Err(format!("Unknown gitea key \"{key}\". Tokens never go in settings; store one with: story secret set gitea"));
    }
    let base_url = prop(g, "baseUrl");
    if !truthy(base_url) {
        return Err(
            "tracker \"gitea\" needs gitea.baseUrl, e.g. \"https://gitea.example.com\".".into(),
        );
    }
    let base_url = text(base_url.unwrap(), "gitea.baseUrl")?;
    let url = reqwest::Url::parse(base_url)
        .ok()
        .filter(|u| ["http", "https"].contains(&u.scheme()));
    let Some(url) = url else {
        return Err(format!("gitea.baseUrl must be an http(s) URL such as \"https://gitea.example.com\"; got \"{base_url}\"."));
    };
    // Stricter than Node: the URL can carry a secret, so the reason never echoes it.
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "gitea.baseUrl must not contain credentials, a query, or a fragment. Store the token with: story secret set gitea".into(),
        );
    }
    if !truthy(prop(g, "owner")) {
        return Err("tracker \"gitea\" needs gitea.owner (the user or organization).".into());
    }
    if !truthy(prop(g, "repo")) {
        return Err("tracker \"gitea\" needs gitea.repo.".into());
    }
    let segment = |key| {
        let name = text(&g[key], &format!("gitea.{key}"))?;
        if is_segment(name) {
            Ok(name)
        } else {
            Err(format!(
                "gitea.{key} must be a single name (letters, digits, _ . -); got \"{name}\"."
            ))
        }
    };
    Ok(json!({
        "baseUrl": base_url.trim_end_matches('/'),
        "owner": segment("owner")?,
        "repo": segment("repo")?,
    }))
}

fn jev(j: &Value) -> Result<Value, String> {
    if let Some(key) = js_keys(j).into_iter().find(|k| k != "model") {
        return Err(format!("Unknown jev key \"{key}\". Keys never go in settings; store one with: story secret set jev"));
    }
    let model = prop(j, "model");
    if let Some(model) = model {
        let text = js_string(model);
        if !text.strip_prefix("jev-").is_some_and(is_name) {
            return Err(format!(
                "jev.model must be a Jev model id such as \"jev-1.13.0\"; got \"{text}\"."
            ));
        }
    }
    Ok(if truthy(model) {
        json!({ "model": model })
    } else {
        json!({})
    })
}

/// A field native readers use as a string; stricter than Node, which accepts any value `String(value)` turns into valid text.
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value
        .as_str()
        .ok_or_else(|| format!("{name} must be a string."))
}

fn string(value: Option<&Value>, name: &str) -> Result<Option<Value>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(json!(s))),
        _ => Err(format!("{name} must be a string.")),
    }
}

fn string_list(value: Option<&Value>, name: &str) -> Result<Value, String> {
    match value {
        None => Ok(json!([])),
        Some(Value::Array(items)) if items.iter().all(Value::is_string) => {
            Ok(Value::Array(items.clone()))
        }
        _ => Err(format!("{name} must be a list of strings.")),
    }
}

fn positive_integer(value: &Value) -> bool {
    value
        .as_number()
        .map(crate::js_number::value)
        .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n >= 1.0)
}

fn is_prefix(s: &str) -> bool {
    s.bytes().next().is_some_and(|c| c.is_ascii_uppercase())
        && s.bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Matches `^[\w.-]+$`.
fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}

/// A name that is safe as one tracker URL path segment; stricter than Node, which accepts `.` and `..`.
fn is_segment(s: &str) -> bool {
    is_name(s) && s != "." && s != ".."
}

/// `value.key` in JavaScript: only objects carry the settings keys.
fn prop<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object().and_then(|o| o.get(key))
}

/// `value ?? default` treats null like a missing key.
fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !v.is_null())
}

pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `String(value)` in JavaScript.
pub(crate) fn js_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                if v.is_null() {
                    String::new()
                } else {
                    js_string(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
        Value::Number(n) => crate::js_number::to_string(crate::js_number::value(n)),
        other => other.to_string(),
    }
}

/// `Object.keys(value)` in JavaScript.
fn js_keys(value: &Value) -> Vec<String> {
    match value {
        Value::Object(o) => o.keys().cloned().collect(),
        Value::Array(a) => (0..a.len()).map(|i| i.to_string()).collect(),
        Value::String(s) => (0..s.encode_utf16().count())
            .map(|i| i.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// `{ ...value }` in JavaScript.
fn spread(value: &Value) -> Value {
    match value {
        Value::Object(o) => Value::Object(o.clone()),
        Value::Array(a) => Value::Object(
            a.iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
        ),
        Value::String(s) => Value::Object(
            s.chars()
                .enumerate()
                .map(|(i, c)| (i.to_string(), json!(c.to_string())))
                .collect(),
        ),
        _ => json!({}),
    }
}

/// The value `JSON.parse` would build: array-index keys first in ascending order, and each number the Number JavaScript holds.
pub(crate) fn js_value(value: &Value) -> Value {
    match value {
        Value::Object(o) => {
            let mut entries: Vec<_> = o.iter().map(|(k, v)| (k.clone(), js_value(v))).collect();
            entries.sort_by_key(|(k, _)| array_index(k).map_or((1, 0), |i| (0, i)));
            Value::Object(entries.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(js_value).collect()),
        Value::Number(n) => crate::js_number::number(crate::js_number::value(n)),
        _ => value.clone(),
    }
}

pub(crate) fn array_index(key: &str) -> Option<u64> {
    let n: u64 = key.parse().ok()?;
    (n.to_string() == key && n < u32::MAX as u64).then_some(n)
}
