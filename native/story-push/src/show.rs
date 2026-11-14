//! `story show <id> [--json]`, reading a story the way the Node CLI's lenient `parseStory` does.

use crate::{config, js_space, lenient_story, lenient_story::Js, list, Failure};
use serde_json::Value;
use std::path::Path;

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
    let (_, story) = read(&flags)?;
    Ok(if flags.has("json") {
        format!("{}\n", story.json())
    } else {
        format!(
            "{}\n",
            lenient_story::serialize(&story).trim_end_matches(js_space)
        )
    })
}

/// The settings and the story named by the first positional argument, read the way Node's
/// `requireStory` reads it.
pub(crate) fn read(flags: &config::Flags) -> Result<(Value, Js), Failure> {
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = match flags
        .value("dir")
        .map(str::to_owned)
        .or_else(|| std::env::var("STORY_TIME_DIR").ok())
    {
        Some(dir) => config::resolve(Path::new(&dir))?,
        None => config::resolve(&loaded.root.join(settings["backlogDir"].as_str().unwrap()))?,
    };
    let id = flags.positional.first().ok_or(Failure {
        code: 2,
        message: "A story id is required.".into(),
    })?;
    let story = list::get(&dir, id)?.ok_or_else(|| format!("Story {id} not found."))?;
    Ok((settings, story))
}

/// `requireStory` for each id, failing the way Node's `Promise.all` of them settles: an empty id
/// first, then an invalid one, then the first that cannot be read.
pub(crate) fn require_all(dir: &Path, ids: &[String]) -> Result<Vec<Js>, Failure> {
    if ids.iter().any(String::is_empty) {
        return Err(Failure {
            code: 2,
            message: "A story id is required.".into(),
        });
    }
    if let Some(id) = ids.iter().find(|id| !crate::valid_id(id)) {
        return Err(format!("Invalid story id: {id}").into());
    }
    ids.iter()
        .map(|id| Ok(list::get(dir, id)?.ok_or_else(|| format!("Story {id} not found."))?))
        .collect()
}
