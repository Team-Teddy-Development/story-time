pub mod adoption;
pub mod gateway;
pub mod init;
pub mod planning;
pub mod story;

use chrono::Utc;
use gateway::{Gateway, Remote};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const USAGE: &str = "Usage: story push <id...> | --all [--dry-run]";

#[derive(Debug)]
pub struct Failure {
    pub code: i32,
    pub message: String,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self { code: 1, message }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

#[derive(Default)]
struct Args {
    ids: Vec<String>,
    all: bool,
    dry: bool,
    json: bool,
    config: Option<String>,
    dir: Option<String>,
}

fn parse(args: &[String]) -> Result<Args, Failure> {
    let mut result = Args::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--all" => result.all = true,
            "--dry-run" => result.dry = true,
            "--json" => result.json = true,
            "--config" | "--dir" => {
                let flag = &args[index];
                index += 1;
                let value = args
                    .get(index)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| Failure {
                        code: 2,
                        message: format!("Flag {flag} needs a value."),
                    })?
                    .clone();
                if flag == "--config" {
                    result.config = Some(value)
                } else {
                    result.dir = Some(value)
                }
            }
            value if value.starts_with("--") => {
                return Err(Failure {
                    code: 2,
                    message: format!("Unknown push flag {value}."),
                })
            }
            value => result.ids.push(value.to_owned()),
        }
        index += 1;
    }
    if result.all && !result.ids.is_empty() {
        return Err(Failure {
            code: 2,
            message: "Use --all or explicit story IDs, not both.".into(),
        });
    }
    Ok(result)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}
fn absolute(path: impl AsRef<Path>) -> Result<PathBuf, String> {
    let p = path.as_ref();
    if p.is_absolute() {
        Ok(p.to_owned())
    } else {
        Ok(std::env::current_dir().map_err(|e| e.to_string())?.join(p))
    }
}

pub fn validate_config(value: Value) -> Result<Value, String> {
    let mut object = value
        .as_object()
        .cloned()
        .ok_or("Config must be a JSON object.")?;
    for key in object.keys() {
        if ![
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
        ]
        .contains(&key.as_str())
        {
            return Err(format!("Unknown config key \"{key}\"."));
        }
    }
    let tracker = object
        .get("tracker")
        .unwrap_or(&Value::Null)
        .as_str()
        .unwrap_or("files")
        .to_string();
    if !["files", "github", "ado", "gitea", "jira", "linear"].contains(&tracker.as_str()) {
        return Err(format!("Unknown tracker \"{tracker}\"."));
    }
    let sync = object
        .get("sync")
        .unwrap_or(&Value::Null)
        .as_str()
        .unwrap_or("off")
        .to_string();
    if !["off", "ask", "auto"].contains(&sync.as_str()) {
        return Err("sync must be one of: off, ask, auto.".into());
    }
    for (key, default) in [
        ("tracker", tracker.as_str()),
        ("sync", sync.as_str()),
        ("backlogDir", "backlog"),
        ("prefix", "ST"),
    ] {
        match object.get(key) {
            Some(Value::String(s)) if !s.is_empty() => {}
            None | Some(Value::Null) => {
                object.insert(key.into(), json!(default));
            }
            _ => return Err(format!("{key} must be a nonempty string.")),
        }
    }
    let prefix = object["prefix"].as_str().unwrap();
    if !prefix
        .bytes()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
        || !prefix
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err("prefix must be uppercase letters/digits.".into());
    }
    if sync != "off" && !["github", "ado", "gitea"].contains(&tracker.as_str()) {
        return Err(format!("sync \"{sync}\" needs tracker github, ado, or gitea; tracker is \"{tracker}\". Use sync \"off\" and story export cards."));
    }
    for key in ["definitionOfDone", "definitionOfReady"] {
        if let Some(v) = object.get(key) {
            if !v.as_array().is_some_and(|a| a.iter().all(Value::is_string)) {
                return Err(format!("{key} must be a list of strings."));
            }
        }
    }
    if let Some(team) = object.get("team") {
        if !team.is_object() {
            return Err("team must be an object.".into());
        }
        for key in ["sprintDays", "wipLimit"] {
            if let Some(v) = team.get(key) {
                if v.as_u64().filter(|n| *n > 0).is_none() {
                    return Err(format!("team.{key} must be a positive integer."));
                }
            }
        }
        if let Some(v) = team.get("velocity") {
            if !v.as_f64().is_some_and(|n| n > 0.0) {
                return Err("team.velocity must be a positive number of points per sprint.".into());
            }
        }
    }
    if let Some(project) = object.get("project") {
        if !project.is_object() {
            return Err("project must be an object.".into());
        }
        if let Some(stack) = project.get("stack") {
            if !stack
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
            {
                return Err("project.stack must be a list of strings.".into());
            }
        }
        for key in ["testFramework", "testCommand"] {
            if let Some(v) = project.get(key) {
                if !v.is_null() && !v.is_string() {
                    return Err(format!("project.{key} must be a string."));
                }
            }
        }
    }
    for name in ["github", "gitea", "ado"] {
        if tracker != name && !object.contains_key(name) {
            continue;
        }
        let cfg = object
            .get(name)
            .and_then(Value::as_object)
            .ok_or_else(|| format!("tracker \"{name}\" needs {name} settings."))?;
        let keys: &[&str] = match name {
            "github" => &["repo"],
            "gitea" => &["baseUrl", "owner", "repo"],
            _ => &["organization", "project"],
        };
        for key in keys {
            if cfg
                .get(*key)
                .and_then(Value::as_str)
                .is_none_or(|s| s.is_empty())
            {
                return Err(format!("tracker \"{name}\" needs {name}.{key}."));
            }
        }
        if name == "gitea" {
            for key in cfg.keys() {
                if !["baseUrl", "owner", "repo"].contains(&key.as_str()) {
                    return Err(format!("Unknown gitea key \"{key}\". Tokens never go in settings; store one with: story secret set gitea"));
                }
            }
            for key in ["owner", "repo"] {
                if !segment(cfg[key].as_str().unwrap()) {
                    return Err(format!("gitea.{key} must be a single name."));
                }
            }
            let url = reqwest::Url::parse(cfg["baseUrl"].as_str().unwrap())
                .map_err(|_| "gitea.baseUrl must be an http(s) URL.")?;
            if !["http", "https"].contains(&url.scheme())
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(
                    "gitea.baseUrl must be an http(s) URL without credentials, query, or fragment."
                        .into(),
                );
            }
        }
        if name == "github" {
            let pieces: Vec<_> = cfg["repo"].as_str().unwrap().split('/').collect();
            if pieces.len() != 2 || !pieces.iter().all(|p| segment(p)) {
                return Err("github.repo must be owner/repo.".into());
            }
        }
        if name == "ado" {
            if let Some(p) = cfg.get("process") {
                if !p
                    .as_str()
                    .is_some_and(|s| ["agile", "scrum", "basic"].contains(&s))
                {
                    return Err("ado.process must be one of: agile, scrum, basic.".into());
                }
            }
        }
    }
    if let Some(j) = object.get("jev") {
        let j = j.as_object().ok_or("jev must be an object.")?;
        for key in j.keys() {
            if key != "model" {
                return Err(format!("Unknown jev key \"{key}\". Keys never go in settings; store one with: story secret set jev"));
            }
        }
        if let Some(m) = j.get("model") {
            if !m.as_str().is_some_and(|s| {
                s.starts_with("jev-")
                    && s[4..]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
            }) {
                return Err("jev.model must be a Jev model id.".into());
            }
        }
    }
    Ok(Value::Object(object))
}
fn segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}
fn valid_id(s: &str) -> bool {
    let Some((prefix, n)) = s.rsplit_once('-') else {
        return false;
    };
    !prefix.is_empty()
        && prefix.bytes().next().unwrap().is_ascii_uppercase()
        && prefix
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !n.is_empty()
        && n.bytes().all(|c| c.is_ascii_digit())
}

struct Context {
    config: Value,
    root: PathBuf,
    dir: PathBuf,
}
fn context(args: &Args) -> Result<Context, String> {
    let config_path = if let Some(p) = args.config.clone().or_else(|| env("STORY_TIME_CONFIG")) {
        absolute(p)?
    } else if Path::new(".story-time/settings.json").exists() {
        absolute(".story-time/settings.json")?
    } else if Path::new(".story-time.json").exists() {
        absolute(".story-time.json")?
    } else {
        absolute(".story-time/settings.json")?
    };
    let config = match fs::read_to_string(&config_path) {
        Ok(text) => {
            let raw = serde_json::from_str(&text)
                .map_err(|_| format!("{}: invalid JSON configuration.", config_path.display()))?;
            validate_config(raw).map_err(|e| format!("{}: {e}", config_path.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => validate_config(json!({}))?,
        Err(e) => return Err(format!("Could not read configuration: {e}")),
    };
    let parent = config_path.parent().ok_or("Config has no parent.")?;
    let root = if parent.file_name().is_some_and(|p| p == ".story-time") {
        parent
            .parent()
            .ok_or("Config has no project root.")?
            .to_owned()
    } else {
        parent.to_owned()
    };
    let dir = if let Some(p) = args.dir.clone().or_else(|| env("STORY_TIME_DIR")) {
        absolute(p)?
    } else {
        root.join(config["backlogDir"].as_str().unwrap())
    };
    Ok(Context { config, root, dir })
}

pub trait Tracker {
    fn find(&mut self, story: &Value) -> Result<Option<Remote>, String>;
    fn create(&mut self, story: &Value) -> Result<Remote, String>;
    fn update(&mut self, story: &Value) -> Result<Remote, String>;
    fn remove(&mut self, id: &str) -> Result<(), String>;
}
impl Tracker for Gateway {
    fn find(&mut self, s: &Value) -> Result<Option<Remote>, String> {
        Gateway::find(self, s)
    }
    fn create(&mut self, s: &Value) -> Result<Remote, String> {
        Gateway::create(self, s)
    }
    fn update(&mut self, s: &Value) -> Result<Remote, String> {
        Gateway::update(self, s)
    }
    fn remove(&mut self, id: &str) -> Result<(), String> {
        Gateway::remove(self, id)
    }
}

#[derive(Clone)]
struct Planned {
    id: String,
    path: PathBuf,
    original: Vec<u8>,
    story: Value,
    action: &'static str,
}
fn plans(ctx: &Context, args: &Args) -> Result<Vec<Planned>, Failure> {
    let ids = if args.all {
        let mut ids = Vec::new();
        match fs::read_dir(&ctx.dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "md") {
                        let id = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .ok_or("Invalid story filename.")?;
                        if valid_id(id) {
                            ids.push(id.to_string());
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string().into()),
        };
        ids.sort();
        ids
    } else {
        args.ids.clone()
    };
    if ids.is_empty() && !args.all {
        return Err(Failure {
            code: 2,
            message: USAGE.into(),
        });
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        if !valid_id(&id) {
            return Err(format!("Invalid story id: {id}").into());
        }
        if !seen.insert(id.clone()) {
            return Err(format!("Duplicate story id: {id}").into());
        }
        let path = ctx.dir.join(format!("{id}.md"));
        let meta = fs::symlink_metadata(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("Story {id} not found.")
            } else {
                format!("Could not read {id} metadata.")
            }
        })?;
        if !meta.file_type().is_file() {
            return Err(format!("Story {id} must be a regular file.").into());
        }
        let original = fs::read(&path).map_err(|_| format!("Could not read story {id}."))?;
        let text =
            std::str::from_utf8(&original).map_err(|_| format!("Story {id} is not UTF-8."))?;
        let story = story::parse(text)?;
        if story["id"].as_str() != Some(&id) {
            return Err(format!("Story {id} has mismatched frontmatter id.").into());
        }
        let remote = story
            .get("remote")
            .filter(|v| !v.is_null() && v.as_str() != Some(""));
        let closed = story["status"]
            .as_str()
            .is_some_and(|s| ["done", "split", "archived"].contains(&s));
        let action = if remote.is_some() {
            "update"
        } else if closed {
            "skip"
        } else {
            "create"
        };
        result.push(Planned {
            id,
            path,
            original,
            story,
            action,
        });
    }
    Ok(result)
}
fn summary(plan: &[Planned]) -> Value {
    Value::Array(
        plan.iter()
            .map(|p| {
                let mut v = json!({"id":p.id,"action":p.action});
                if let Some(r) = p
                    .story
                    .get("remote")
                    .filter(|r| !r.is_null() && r.as_str() != Some(""))
                {
                    v["remoteId"] = json!(r
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| r.to_string()));
                }
                v
            })
            .collect(),
    )
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(format!("md.{}.tmp", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mode = fs::metadata(path)
            .map_err(|_| "Could not read story permissions.")?
            .permissions()
            .mode();
        options.mode(mode);
    }
    let result = (|| {
        let mut file = options
            .open(&tmp)
            .map_err(|_| "Could not create story temporary file.")?;
        file.write_all(bytes)
            .map_err(|_| "Could not write story temporary file.")?;
        file.sync_all()
            .map_err(|_| "Could not sync story temporary file.")?;
        fs::rename(&tmp, path).map_err(|_| "Could not replace story file.")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

const PENDING_CREATE: &str = "pending_create";

fn step(
    p: &Planned,
    gateway: &mut dyn Tracker,
    date: &str,
) -> Result<(&'static str, Remote, Vec<u8>), String> {
    if p.action == "update" {
        return Ok(("update", gateway.update(&p.story)?, p.original.clone()));
    }
    if p.story.get(PENDING_CREATE).is_some_and(|v| !v.is_null()) {
        if let Some(remote) = gateway.find(&p.story)? {
            return Ok(("link", remote, p.original.clone()));
        }
        return Ok(("create", gateway.create(&p.story)?, p.original.clone()));
    }
    unchanged(p, &p.original)?;
    let mut marked = p.story.clone();
    marked[PENDING_CREATE] = json!(date);
    let bytes = story::serialize(&marked)?.into_bytes();
    atomic_write(&p.path, &bytes)?;
    Ok(("create", gateway.create(&p.story)?, bytes))
}
fn unchanged(p: &Planned, expected: &[u8]) -> Result<(), String> {
    if fs::read(&p.path).map_err(|_| "Could not re-read story.")? != expected {
        return Err(format!(
            "Story {} changed during push; refusing to overwrite it.",
            p.id
        ));
    }
    Ok(())
}

fn execute(plan: &[Planned], gateway: &mut dyn Tracker, date: &str) -> Result<Value, String> {
    let mut remote_results: Vec<(&Planned, &'static str, Remote, Vec<u8>)> = Vec::new();
    let mut linked = 0;
    let operation = (|| -> Result<(), String> {
        for p in plan.iter().filter(|p| p.action != "skip") {
            let (action, r, expected) =
                step(p, gateway, date).map_err(|e| format!("{}: {e}", p.id))?;
            remote_results.push((p, action, r, expected));
        }
        for (p, action, r, expected) in &remote_results {
            unchanged(p, expected)?;
            let value = story::link(&p.story, &r.remote_id, &r.url, date, action)?;
            let bytes = story::serialize(&value)?;
            atomic_write(&p.path, bytes.as_bytes())?;
            linked += 1;
        }
        Ok(())
    })();
    if let Err(error) = operation {
        let mut undone = Vec::new();
        let mut stuck = Vec::new();
        for (p, _, _, expected) in remote_results[..linked].iter().rev() {
            match atomic_write(&p.path, expected) {
                Ok(()) => undone.push(format!("link {}", p.id)),
                Err(_) => stuck.push(format!("link {}", p.id)),
            }
        }
        for (p, _, r, expected) in remote_results
            .iter()
            .rev()
            .filter(|(_, action, _, _)| *action == "create")
        {
            match gateway.remove(&r.remote_id) {
                Ok(()) => {
                    if unchanged(p, expected).is_ok() {
                        let _ = atomic_write(&p.path, &p.original);
                    }
                    undone.push(format!("create {}", p.id))
                }
                Err(_) => stuck.push(format!("create {}", p.id)),
            }
        }
        let mut message = format!("Push failed: {error}");
        if !undone.is_empty() {
            message.push_str(&format!("\nRolled back: {}.", undone.join(", ")));
        }
        if !stuck.is_empty() {
            message.push_str(&format!(
                "\nCould not roll back (fix by hand): {}.",
                stuck.join(", ")
            ));
        }
        return Err(message);
    }
    Ok(
        json!({"ok":true,"pushed":remote_results.iter().map(|(p,action,r,_)|json!({"id":p.id,"action":action,"remoteId":r.remote_id,"url":r.url})).collect::<Vec<_>>(),"skipped":plan.iter().filter(|p|p.action=="skip").map(|p|p.id.clone()).collect::<Vec<_>>()}),
    )
}

pub fn push(args: &[String]) -> Result<String, Failure> {
    let args = parse(args)?;
    let ctx = context(&args)?;
    if ctx.config["sync"] == "off" {
        return Err("Pushing is off for this project (sync: \"off\" in .story-time/settings.json). Use story export for a copyable card, or run the story-setup skill to set sync to \"ask\" or \"auto\".".into());
    }
    if !["github", "ado", "gitea"].contains(&ctx.config["tracker"].as_str().unwrap_or("")) {
        return Err("story push supports github, ado, or gitea.".into());
    }
    let plan = plans(&ctx, &args)?;
    let result = if args.dry {
        json!({"ok":true,"plan":summary(&plan)})
    } else if plan.iter().all(|p| p.action == "skip") {
        json!({"ok":true,"pushed":[],"skipped":plan.iter().map(|p|p.id.clone()).collect::<Vec<_>>()})
    } else {
        let mut gateway = Gateway::new(&ctx.config, &ctx.root)?;
        execute(
            &plan,
            &mut gateway,
            &Utc::now().format("%Y-%m-%d").to_string(),
        )?
    };
    if args.json {
        return Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
        ));
    }
    if args.dry {
        let rows = result["plan"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                vec![
                    p["action"].as_str().unwrap().to_owned(),
                    p["id"].as_str().unwrap().to_owned(),
                    p["remoteId"].as_str().unwrap_or("-").to_owned(),
                ]
            })
            .collect();
        return Ok(table(&["Action", "ID", "Remote"], rows));
    }
    let pushed = result["pushed"].as_array().unwrap();
    let mut output = if pushed.is_empty() {
        "Nothing to push.\n".into()
    } else {
        table(
            &["Action", "ID", "URL"],
            pushed
                .iter()
                .map(|p| {
                    ["action", "id", "url"]
                        .iter()
                        .map(|k| p[k].as_str().unwrap().to_owned())
                        .collect()
                })
                .collect(),
        )
    };
    let skipped = result["skipped"].as_array().unwrap();
    if !skipped.is_empty() {
        output.push_str(&format!(
            "Skipped (closed, never pushed): {}\n",
            skipped
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(output)
}
fn table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    let widths: Vec<_> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            rows.iter()
                .map(|r| r[i].chars().count())
                .chain([h.chars().count()])
                .max()
                .unwrap()
        })
        .collect();
    let render = |row: Vec<String>| -> String {
        row.iter()
            .enumerate()
            .map(|(i, c)| format!("{c}{}", " ".repeat(widths[i] - c.chars().count())))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_owned()
    };
    let mut output = render(headers.iter().map(|s| s.to_string()).collect());
    for row in rows {
        output.push('\n');
        output.push_str(&render(row));
    }
    output.push('\n');
    output
}

#[cfg(test)]
mod saga_tests;
