//! `story note <id> "what changed and why"`: log a prose edit in the story's changelog.

use crate::{
    config::{self, Flags},
    lenient_story::{append_changelog, trim},
    list, Failure,
};
use chrono::Utc;

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let (id, words) = match flags.positional.split_first() {
        Some((id, words)) if !id.is_empty() && !words.is_empty() => (id, words),
        _ => {
            return Err(Failure {
                code: 2,
                message: "Usage: story note <id> \"what changed and why\"".into(),
            })
        }
    };
    let note = words.join(" ");
    let note = trim(&note);
    if note.is_empty() {
        return Err("A note is required.".into());
    }
    let story = list::get(&dir, id)?.ok_or_else(|| format!("Story {id} not found."))?;
    let next = append_changelog(&story, note, &Utc::now().format("%Y-%m-%d").to_string());
    list::save(&dir, &next)?;
    Ok(if flags.has("json") {
        format!("{}\n", next.json())
    } else {
        format!("{} noted.\n", next.get("id").string())
    })
}
