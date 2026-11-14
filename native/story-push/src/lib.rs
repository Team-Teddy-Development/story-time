pub mod adoption;
mod adoption_apply;
pub mod config;
pub mod export;
pub mod gateway;
pub mod init;
mod lenient_story;
pub mod list;
pub mod machine;
pub mod metrics;
mod models;
pub mod new_story;
mod next_id;
pub mod note;
pub mod planning;
pub mod secret;
pub mod show;
pub mod stats;
pub mod story;
pub mod table;
mod toml_subset;

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

pub type Native = fn(&[String]) -> Result<(String, u8), Failure>;

pub const NATIVE_COMMANDS: &[(&str, Native)] = &[
    ("init", |args| init::run(args).map(|out| (out, 0))),
    ("push", |args| push(args).map(|out| (out, 0))),
    ("adopt", |args| Ok(adoption::run(args))),
    ("plan", |args| Ok(planning::run(args))),
    ("config", |args| config::run(args).map(|out| (out, 0))),
    ("show", |args| show::run(args).map(|out| (out, 0))),
    ("issues", |args| issues(args).map(|out| (out, 0))),
    ("list", |args| list::run(args).map(|out| (out, 0))),
    ("stats", |args| stats::run(args).map(|out| (out, 0))),
    ("secret", |args| secret::run(args).map(|out| (out, 0))),
    ("machine", machine::run),
    ("table", table::run),
    ("metrics", |args| metrics::run(args).map(|out| (out, 0))),
    ("export", |args| export::run(args).map(|out| (out, 0))),
    ("note", |args| note::run(args).map(|out| (out, 0))),
    ("new", |args| new_story::run(args).map(|out| (out, 0))),
];

pub fn legacy_required_message() -> String {
    let names: Vec<&str> = NATIVE_COMMANDS.iter().map(|(name, _)| *name).collect();
    let list = match names.as_slice() {
        [] => String::new(),
        [only] => only.to_string(),
        [first, second] => format!("{first} and {second}"),
        [init @ .., last] => format!("{}, and {last}", init.join(", ")),
    };
    let mut capitalized = list.chars();
    let capitalized = capitalized
        .next()
        .map(|first| first.to_uppercase().chain(capitalized).collect::<String>())
        .unwrap_or_default();
    format!("Commands other than {list} require STORY_TIME_LEGACY_CLI or ~/.config/story-time/runtime.json with legacyCli. {capitalized} are fully native and need neither Node nor this setting.")
}

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

pub(crate) fn valid_id(s: &str) -> bool {
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
    let loaded = config::load(args.config.as_deref())?;
    let config =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let root = loaded.root;
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
    /// Why linking this story would rewrite its file with less than it holds.
    loss: Option<String>,
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
        let (parsed, dropped) = lenient_story::parse_noting_drops(text)?;
        let story = parsed.to_value();
        let loss = rewrite_loss(&id, &parsed, &story, dropped);
        if story["id"].as_str() != Some(&id) {
            return Err(format!("Story {id} has mismatched frontmatter id.").into());
        }
        let remote = story
            .get("remote")
            .filter(|v| !v.is_null() && v.as_str() != Some(""));
        let closed = story["status"]
            .as_str()
            .is_some_and(|s| ["done", "split", "archived"].contains(&s));
        let pending = story.get(PENDING_CREATE).is_some_and(|v| !v.is_null());
        let action = if remote.is_some() {
            "update"
        } else if closed && pending {
            "reconcile"
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
            loss,
        });
    }
    Ok(result)
}

fn rewrite_loss(
    id: &str,
    parsed: &lenient_story::Js,
    story: &Value,
    dropped: Vec<String>,
) -> Option<String> {
    if !dropped.is_empty() {
        return Some(format!(
            "Story {id} has {}, which push would drop when it links the story; move or remove that text, then push again.",
            dropped.join(", ")
        ));
    }
    match story::serialize(story).and_then(|text| lenient_story::parse(&text)) {
        Err(e) => Some(format!("Story {id} cannot be rewritten: {e}")),
        Ok(reread) if !reread.same(parsed) => Some(format!(
            "Story {id} has frontmatter that push cannot rewrite without changing it."
        )),
        Ok(_) => None,
    }
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

type Pushed = (&'static str, Remote, Vec<u8>);

fn step(p: &Planned, gateway: &mut dyn Tracker, date: &str) -> Result<Option<Pushed>, String> {
    if p.action == "update" {
        return Ok(Some((
            "update",
            gateway.update(&p.story)?,
            p.original.clone(),
        )));
    }
    if p.action == "reconcile" {
        let Some(remote) = gateway.find(&p.story)? else {
            return Ok(None);
        };
        let mut linked = p.story.clone();
        linked["remote"] = json!(remote.remote_id);
        gateway.update(&linked)?;
        return Ok(Some(("link", remote, p.original.clone())));
    }
    if p.story.get(PENDING_CREATE).is_some_and(|v| !v.is_null()) {
        if let Some(remote) = gateway.find(&p.story)? {
            return Ok(Some(("link", remote, p.original.clone())));
        }
        return Ok(Some((
            "create",
            gateway.create(&p.story)?,
            p.original.clone(),
        )));
    }
    unchanged(p, &p.original)?;
    let mut marked = p.story.clone();
    marked[PENDING_CREATE] = json!(date);
    let bytes = story::serialize(&marked)?.into_bytes();
    atomic_write(&p.path, &bytes)?;
    Ok(Some(("create", gateway.create(&p.story)?, bytes)))
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
    let mut skipped: Vec<String> = plan
        .iter()
        .filter(|p| p.action == "skip")
        .map(|p| p.id.clone())
        .collect();
    if let Some(loss) = plan
        .iter()
        .filter(|p| p.action != "skip")
        .find_map(|p| p.loss.clone())
    {
        return Err(loss);
    }
    let mut linked = 0;
    let operation = (|| -> Result<(), String> {
        for p in plan.iter().filter(|p| p.action != "skip") {
            match step(p, gateway, date).map_err(|e| format!("{}: {e}", p.id))? {
                Some((action, r, expected)) => remote_results.push((p, action, r, expected)),
                None => skipped.push(p.id.clone()),
            }
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
        json!({"ok":true,"pushed":remote_results.iter().map(|(p,action,r,_)|json!({"id":p.id,"action":action,"remoteId":r.remote_id,"url":r.url})).collect::<Vec<_>>(),"skipped":skipped}),
    )
}

pub fn issues(args: &[String]) -> Result<String, Failure> {
    let usage = |message: String| Failure { code: 2, message };
    let (mut json, mut limit, mut rest) = (false, 200, Vec::new());
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--state" => {
                index += 1;
                if args.get(index).map(String::as_str) != Some("open") {
                    return Err(usage("story issues lists only --state open.".into()));
                }
            }
            "--limit" => {
                index += 1;
                limit = args
                    .get(index)
                    .and_then(|n| n.parse::<usize>().ok())
                    .filter(|n| *n > 0)
                    .ok_or_else(|| usage("--limit needs a positive number.".into()))?;
            }
            _ => rest.push(args[index].clone()),
        }
        index += 1;
    }
    let args = parse(&rest)?;
    if !args.ids.is_empty() || args.all || args.dry {
        return Err(usage(
            "Usage: story issues --state open [--limit N] [--json]".into(),
        ));
    }
    let ctx = context(&args)?;
    if ctx.config["tracker"] != "gitea" {
        return Err("story issues lists Gitea issues only. For a github tracker, run gh issue list --state open.".into());
    }
    let issues = Gateway::new(&ctx.config, &ctx.root)?.open_issues(limit)?;
    if json {
        return Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&issues).map_err(|e| e.to_string())?
        ));
    }
    Ok(table(
        &["Number", "Title", "Labels"],
        issues
            .iter()
            .map(|i| {
                vec![
                    i["number"].to_string(),
                    i["title"].as_str().unwrap_or_default().to_owned(),
                    i["labels"]
                        .as_array()
                        .map(|l| {
                            l.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_default(),
                ]
            })
            .collect(),
    ))
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
/// `\s` in a JavaScript regular expression, which `trim` also removes.
pub(crate) fn js_space(c: char) -> bool {
    (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
}

/// Node's `table`: columns padded to their widest cell in UTF-16 units, rows end-trimmed.
pub(crate) fn table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    let width = |s: &str| s.encode_utf16().count();
    let widths: Vec<_> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            rows.iter()
                .map(|r| width(&r[i]))
                .chain([width(h)])
                .max()
                .unwrap()
        })
        .collect();
    let render = |row: Vec<String>| -> String {
        row.iter()
            .enumerate()
            .map(|(i, c)| format!("{c}{}", " ".repeat(widths[i] - width(c))))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end_matches(js_space)
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
