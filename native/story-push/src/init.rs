use crate::{validate_config, Failure};
use serde_json::{json, Map, Value};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

const HELP: &str = "Usage: story init [--from answers.json] [--force] [--config path] [--json]\n\nWrite validated project settings to .story-time/settings.json.\n\n  --from FILE    Read settings from a JSON file; defaults apply when omitted.\n  --force        Replace an existing settings file after validation.\n  --config PATH  Write settings to an explicit path (before STORY_TIME_CONFIG).\n  --json         Print the effective settings as JSON.\n  -h, --help     Show this help without reading or writing project files.\n\nGitHub settings use github.repo as owner/repo, for example acme/shop.\nGitea settings use separate gitea.baseUrl, gitea.owner, and gitea.repo fields.\nInit is native Rust and needs neither Node nor runtime configuration.\n";

#[derive(Default)]
struct Init {
    from: Option<String>,
    config: Option<String>,
    force: bool,
    json: bool,
}
impl Init {
    fn parse(args: &[String]) -> Result<Self, Failure> {
        let mut request = Self::default();
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--force" => request.force = true,
                "--json" => request.json = true,
                "--from" | "--config" => {
                    let value = args
                        .next()
                        .filter(|v| !v.starts_with("--"))
                        .ok_or_else(|| Failure {
                            code: 2,
                            message: format!("Flag {flag} needs a value."),
                        })?
                        .clone();
                    if flag == "--from" {
                        request.from = Some(value);
                    } else {
                        request.config = Some(value);
                    }
                }
                _ => {
                    return Err(Failure {
                        code: 2,
                        message: format!("Unknown init argument {flag}.\n{HELP}"),
                    })
                }
            }
        }
        Ok(request)
    }
}

fn resolved(path: &Path) -> Result<PathBuf, Failure> {
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

fn optional_string(value: Option<&Value>, name: &str) -> Result<Option<Value>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(json!(s))),
        _ => Err(format!("{name} must be a string.")),
    }
}

fn normalize(raw: Value) -> Result<Value, String> {
    let object = raw.as_object().ok_or("Config must be a JSON object.")?;
    let mut input = object.clone();
    input.remove("$schema");
    if optional_string(input.get("backlogDir"), "backlogDir")?.is_none() {
        input.remove("backlogDir");
    }
    for name in ["team", "project"] {
        if input.get(name).is_none_or(Value::is_null) {
            input.insert(name.into(), json!({}));
        }
    }
    let team = input["team"].as_object().ok_or("team must be an object.")?;
    let mut normalized_team = Map::new();
    for (key, default) in [("sprintDays", 10), ("wipLimit", 1)] {
        let mut value = team
            .get(key)
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(json!(default));
        if value.as_u64().is_none() {
            if let Some(n) = value
                .as_f64()
                .filter(|n| n.is_finite() && *n > 0.0 && n.fract() == 0.0 && *n < u64::MAX as f64)
            {
                value = json!(n as u64);
            }
        }
        normalized_team.insert(key.into(), value);
    }
    if let Some(v) = team.get("velocity") {
        normalized_team.insert("velocity".into(), v.clone());
    }
    let project = input["project"]
        .as_object()
        .ok_or("project must be an object.")?;
    let mut normalized_project = Map::new();
    normalized_project.insert(
        "stack".into(),
        project.get("stack").cloned().unwrap_or(json!([])),
    );
    for key in ["testFramework", "testCommand"] {
        if let Some(value) = optional_string(project.get(key), &format!("project.{key}"))? {
            normalized_project.insert(key.into(), value);
        }
    }
    input.insert("team".into(), Value::Object(normalized_team));
    input.insert("project".into(), Value::Object(normalized_project));
    for key in ["definitionOfDone", "definitionOfReady"] {
        input.entry(key).or_insert(json!([]));
    }
    for name in ["github", "ado", "gitea", "jira", "linear", "jev"] {
        if input.get(name).is_some_and(Value::is_null) {
            input.remove(name);
        }
    }
    if let Some(github) = input.get_mut("github").and_then(Value::as_object_mut) {
        github.retain(|key, _| key == "repo");
    }
    if let Some(ado) = input.get_mut("ado").and_then(Value::as_object_mut) {
        ado.retain(|key, _| {
            [
                "organization",
                "project",
                "process",
                "areaPath",
                "iterationPath",
            ]
            .contains(&key.as_str())
        });
        if ado.get("process").is_none_or(Value::is_null) {
            ado.insert("process".into(), json!("agile"));
        }
        for key in ["areaPath", "iterationPath"] {
            match optional_string(ado.get(key), &format!("ado.{key}"))? {
                Some(value) => {
                    ado.insert(key.into(), value);
                }
                None => {
                    ado.remove(key);
                }
            }
        }
    }
    if input
        .get("jev")
        .and_then(|jev| jev.get("model"))
        .and_then(Value::as_str)
        == Some("jev-")
    {
        return Err("jev.model must be a Jev model id with a nonempty suffix.".into());
    }
    let mut config = validate_config(Value::Object(input))?;
    if let Some(url) = config.get_mut("gitea").and_then(|v| v.get_mut("baseUrl")) {
        *url = json!(url.as_str().unwrap().trim_end_matches('/'));
    }
    Ok(config)
}

pub fn run(args: &[String]) -> Result<String, Failure> {
    if args.iter().any(|s| s == "--help" || s == "-h") {
        return Ok(HELP.into());
    }
    let request = Init::parse(args)?;
    let target = request
        .config
        .or_else(|| std::env::var("STORY_TIME_CONFIG").ok())
        .unwrap_or_else(|| ".story-time/settings.json".into());
    let path = resolved(Path::new(&target))?;
    let raw = match request.from {
        Some(from) => {
            let text =
                fs::read_to_string(&from).map_err(|e| format!("Could not read {from}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("{from}: invalid JSON: {e}"))?
        }
        None => json!({}),
    };
    let config = normalize(raw)?;
    if !request.force && path.try_exists().map_err(|e| e.to_string())? {
        return Err(format!(
            "{} already exists. Re-run with --force to replace it.",
            path.display()
        )
        .into());
    }
    let parent = path.parent().ok_or("Config has no parent.")?;
    let root = if parent.file_name().is_some_and(|p| p == ".story-time") {
        parent.parent().ok_or("Config has no project root.")?
    } else {
        parent
    };
    let metadata = root.join(".story-time");
    let ignore_path = metadata.join(".gitignore");
    let current = match fs::read_to_string(&ignore_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Could not read {}: {e}", ignore_path.display()).into()),
    };
    let settings = format!(
        "{}\n",
        serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?
    );
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    fs::create_dir_all(&metadata).map_err(|e| e.to_string())?;
    if !current.lines().any(|line| line == "*.env") {
        let ignored = if current.is_empty() {
            "# story-time: tracker tokens stay on this machine.\n*.env\n".into()
        } else {
            format!("{}\n*.env\n", current.trim_end())
        };
        fs::write(&ignore_path, ignored).map_err(|e| e.to_string())?;
    }
    fs::write(&path, &settings).map_err(|e| e.to_string())?;
    if request.json {
        Ok(settings)
    } else {
        Ok(format!(
            "Wrote {} (tracker: {}).\n",
            path.display(),
            config["tracker"].as_str().unwrap()
        ))
    }
}
