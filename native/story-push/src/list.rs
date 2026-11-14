//! `story list`: the read side of the backlog that later read commands share.

use crate::{
    config::{self, Flags},
    lenient_story::Js,
    table, Failure,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn story_id(name: &str) -> bool {
    let Some((prefix, number)) = name.rsplit_once('-') else {
        return false;
    };
    prefix.starts_with(|c: char| c.is_ascii_uppercase())
        && prefix
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !number.is_empty()
        && number.bytes().all(|c| c.is_ascii_digit())
}

/// `FileSystemStoryRepository.list({ status })`: every `<ID>.md` in `dir`, parsed, filtered, and
/// sorted by id the way `localeCompare(…, 'en', { numeric: true })` sorts.
pub(crate) fn stories(dir: &Path, status: Option<&str>) -> Result<Vec<Js>, String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .into_iter()
            .filter_map(|name| name.into_string().ok())
            .filter(|name| name.strip_suffix(".md").is_some_and(story_id))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    names.sort();
    let mut stories = Vec::new();
    for name in names {
        let path = dir.join(&name);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        stories.push(crate::lenient_story::parse(&String::from_utf8_lossy(
            &bytes,
        ))?);
    }
    stories.retain(|s| status.is_none_or(|status| *s.get("status") == Js::Str(status.into())));
    // V8 sorts these short arrays by binary insertion, so every story but the first is the
    // receiver of `a.id.localeCompare(b.id)` at least once.
    if stories.len() > 1 {
        if let Some(id) = stories[1..]
            .iter()
            .map(|s| s.get("id"))
            .find(|id| !matches!(id, Js::Str(_)))
        {
            return Err(if *id == Js::Undefined {
                "Cannot read properties of undefined (reading 'localeCompare')".into()
            } else {
                "a.id.localeCompare is not a function".into()
            });
        }
    }
    stories.sort_by_cached_key(|s| collation_key(&s.get("id").string()));
    Ok(stories)
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Primary {
    Symbol(usize),
    Digits(usize, String),
    Letter(u8),
    Other(char),
}

/// The order ICU's root collation gives ASCII whitespace, punctuation, and symbols.
const SYMBOLS: &str = "\t\n\u{b}\u{c}\r _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

/// Primary weights, then case (lowercase first), with each digit run compared as a number.
fn collation_key(id: &str) -> (Vec<Primary>, Vec<bool>) {
    let mut primary = Vec::new();
    let mut tertiary = Vec::new();
    let mut chars = id.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut run = c.to_string();
            while let Some(d) = chars.next_if(char::is_ascii_digit) {
                run.push(d);
            }
            let run = run.trim_start_matches('0').to_owned();
            primary.push(Primary::Digits(run.len(), run));
            tertiary.push(false);
        } else if let Some(rank) = SYMBOLS.find(c) {
            primary.push(Primary::Symbol(rank));
            tertiary.push(false);
        } else if c.is_ascii_alphabetic() {
            primary.push(Primary::Letter(c.to_ascii_lowercase() as u8));
            tertiary.push(c.is_ascii_uppercase());
        } else if !c.is_ascii_control() {
            primary.push(Primary::Other(c));
            tertiary.push(false);
        }
    }
    (primary, tertiary)
}

/// `story list [--status S] [--dir D] [--config C] [--json]`.
pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = backlog_dir(&flags, &loaded.root, &settings)?;
    let status = flags.value("status").filter(|s| !s.is_empty());
    let stories = stories(&dir, status)?;
    if flags.has("json") {
        return Ok(format!("{}\n", Js::Arr(stories).json()));
    }
    let rows = stories
        .iter()
        .map(|s| {
            let size = match s.get("size") {
                Js::Undefined => "-".into(),
                size => size.string(),
            };
            vec![
                s.get("id").string(),
                s.get("status").string(),
                size,
                s.get("title").string(),
            ]
        })
        .collect();
    Ok(table(&["ID", "Status", "Size", "Title"], rows))
}

/// --dir, else $STORY_TIME_DIR (even when empty), else the config's backlogDir under the project root.
pub(crate) fn backlog_dir(
    flags: &Flags,
    root: &Path,
    settings: &serde_json::Value,
) -> Result<PathBuf, String> {
    let chosen = match flags.value("dir") {
        Some(dir) => PathBuf::from(dir),
        None => match std::env::var_os("STORY_TIME_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => root.join(settings["backlogDir"].as_str().unwrap_or("backlog")),
        },
    };
    config::resolve(&chosen)
}
