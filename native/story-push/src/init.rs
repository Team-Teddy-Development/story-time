use crate::Failure;
use serde_json::json;
use std::{fs, path::Path};

const HELP: &str = "Usage: story init [--from answers.json] [--force] [--config path] [--json]\n\nWrite validated project settings to .story-time/settings.json.\n\n  --from FILE    Read settings from a JSON file; defaults apply when omitted.\n  --force        Replace an existing settings file after validation.\n  --config PATH  Write settings to an explicit path (before STORY_TIME_CONFIG).\n  --json         Print the effective settings as JSON.\n  -h, --help     Show this help without reading or writing project files.\n\nGitHub settings use github.repo as owner/repo, for example acme/shop.\nGitea settings use separate gitea.baseUrl, gitea.owner, and gitea.repo fields.\nInit is native Rust and needs neither Node nor runtime configuration.\n";

pub fn run(args: &[String]) -> Result<String, Failure> {
    if let [flag] = args {
        if flag == "--help" || flag == "-h" {
            return Ok(HELP.into());
        }
    }
    let flags = crate::config::Flags::parse(args)?;
    let target = flags
        .value("config")
        .map(str::to_owned)
        .or_else(|| std::env::var("STORY_TIME_CONFIG").ok())
        .unwrap_or_else(|| ".story-time/settings.json".into());
    let path = crate::config::resolve(Path::new(&target))?;
    let raw = match flags.value("from") {
        Some(from) => {
            let text =
                fs::read_to_string(from).map_err(|e| format!("Could not read {from}: {e}"))?;
            crate::js_number::parse(&text).map_err(|e| format!("{from}: invalid JSON: {e}"))?
        }
        None => json!({}),
    };
    let config = crate::config::parse(&raw)?;
    if !flags.has("force") && path.try_exists().map_err(|e| e.to_string())? {
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
    let settings = format!("{}\n", crate::js_number::stringify(&config, true));
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
    if flags.has("json") {
        Ok(settings)
    } else {
        Ok(format!(
            "Wrote {} (tracker: {}).\n",
            path.display(),
            config["tracker"].as_str().unwrap()
        ))
    }
}
