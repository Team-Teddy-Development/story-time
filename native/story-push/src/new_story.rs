//! `story new`: Node's `createStory` writing a draft story file with the next free id. Like
//! Node, it never contacts a tracker; `story push` publishes the story afterwards.

use crate::{
    config::{self, Flags},
    js_number,
    lenient_story::{trim, Js},
    list::{self, backlog_dir},
    next_id::next_id,
    Failure,
};
use chrono::Utc;
use std::path::Path;

pub(crate) const TYPES: [&str; 4] = ["story", "spike", "bug", "chore"];
const STATEMENT_TODO: &str = "TODO: As a <persona>, I want <goal>, so that <benefit>.";

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let prefix = match flags.value("prefix") {
        Some(prefix) => prefix.to_owned(),
        None => std::env::var("STORY_TIME_PREFIX")
            .unwrap_or_else(|_| settings["prefix"].as_str().unwrap_or("ST").to_owned()),
    };
    let story = create(
        &flags,
        &dir,
        &prefix,
        &Utc::now().format("%Y-%m-%d").to_string(),
    )?;
    let id = story.get("id").string();
    Ok(if flags.has("json") {
        format!("{}\n", story.json())
    } else {
        format!("{id} created: {}\n", dir.join(format!("{id}.md")).display())
    })
}

/// `createStory` followed by `FileSystemStoryRepository.save`.
fn create(flags: &Flags, dir: &Path, prefix: &str, date: &str) -> Result<Js, String> {
    let title = flags
        .value("title")
        .map(trim)
        .filter(|t| !t.is_empty())
        .ok_or("A story needs a title.")?;
    let kind = flags.value("type").unwrap_or("story");
    if !TYPES.contains(&kind) {
        return Err(format!(
            "Unknown type \"{kind}\". Use one of: {}.",
            TYPES.join(", ")
        ));
    }
    let text = |key: &str| Js::Str(key.into());
    let bullets = flags
        .values("ac")
        .unwrap_or_default()
        .iter()
        .map(|a| format!("- [ ] {a}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut sections = Vec::new();
    if kind == "spike" {
        let question = flags
            .value("question")
            .unwrap_or("TODO: the one question this spike answers.");
        sections.push(("Question".into(), text(question)));
        let exit = if bullets.is_empty() {
            "- [ ] TODO: decision recorded with evidence"
        } else {
            &bullets
        };
        sections.push(("Exit Criteria".into(), text(exit)));
    } else {
        let statement = flags.value("statement").unwrap_or(STATEMENT_TODO);
        sections.push(("Story".into(), text(statement)));
        if !bullets.is_empty() {
            sections.push(("Acceptance Criteria".into(), text(&bullets)));
        }
    }
    sections.push(("Changelog".into(), text(&format!("- {date} created"))));

    let id = next_id(dir, prefix)?;
    let mut fields = vec![
        ("id".to_owned(), text(&id)),
        ("title".into(), text(title)),
        ("status".into(), text("draft")),
        ("type".into(), text(kind)),
    ];
    for key in ["epic", "persona"] {
        if let Some(value) = flags.value(key) {
            fields.push((key.into(), text(value)));
        }
    }
    for (field, flag) in [
        ("value", "value"),
        ("time_criticality", "time-criticality"),
        ("risk_reduction", "risk-reduction"),
        ("size", "size"),
    ] {
        if let Some(value) = flags.value(flag) {
            fields.push((field.into(), Js::Num(js_number::string_to_number(value))));
        }
    }
    if let Some(timebox) = flags.value("timebox") {
        fields.push(("timebox".into(), text(timebox)));
    }
    fields.extend([
        ("depends_on".into(), Js::Arr(Vec::new())),
        ("created".into(), text(date)),
        ("updated".into(), text(date)),
        ("sections".into(), Js::Obj(sections)),
    ]);
    let story = Js::Obj(fields);
    list::save(dir, &story)?;
    Ok(story)
}
