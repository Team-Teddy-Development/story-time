//! `story show <id> [--json]`, reading a story the way the Node CLI's lenient `parseStory` does.

use crate::{
    config, js_space,
    lenient_story::{self, ordered, Js},
    story::numeric_literal,
    Failure,
};
use std::{fs, io::ErrorKind, path::Path};

const KEY_ORDER: [&str; 18] = [
    "id",
    "title",
    "status",
    "type",
    "epic",
    "persona",
    "value",
    "time_criticality",
    "risk_reduction",
    "size",
    "timebox",
    "depends_on",
    "parent",
    "children",
    "remote",
    "remote_url",
    "created",
    "updated",
];

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
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
    if !crate::valid_id(id) {
        return Err(format!("Invalid story id: {id}").into());
    }
    let bytes = match fs::read(dir.join(format!("{id}.md"))) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(format!("Story {id} not found.").into())
        }
        Err(e) => return Err(e.to_string().into()),
    };
    let story = lenient_story::parse(&String::from_utf8_lossy(&bytes))?;
    Ok(if flags.has("json") {
        format!("{}\n", story.json())
    } else {
        format!("{}\n", serialize(&story).trim_end_matches(js_space))
    })
}

fn format_value(v: &Js) -> String {
    let s = match v {
        Js::Arr(items) => {
            return format!(
                "[{}]",
                items
                    .iter()
                    .map(format_value)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Js::Num(_) | Js::Bool(_) => return v.string(),
        _ => v.string(),
    };
    let needs_quotes = s.is_empty()
        || s.contains([':', '#', '[', ']', ',', '"', '\''])
        || s.starts_with(js_space)
        || s.ends_with(js_space)
        || matches!(s.as_str(), "true" | "false" | "null" | "~")
        || numeric_literal(&s);
    if needs_quotes {
        serde_json::to_string(&s).unwrap()
    } else {
        s
    }
}

/// `serializeStory` from the Node domain module.
fn serialize(story: &Js) -> String {
    let meta = story.props();
    let get = |k: &str| meta.iter().find(|(key, _)| key == k);
    let mut extras: Vec<&(String, Js)> = ordered(meta)
        .into_iter()
        .filter(|(k, _)| !KEY_ORDER.contains(&k.as_str()) && k != "sections")
        .collect();
    extras.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    let fm = KEY_ORDER
        .iter()
        .filter_map(|k| get(k))
        .chain(extras)
        .filter(|(_, v)| !matches!(v, Js::Undefined))
        .map(|(k, v)| format!("{k}: {}", format_value(v)))
        .collect::<Vec<_>>()
        .join("\n");
    let body = ordered(story.get("sections").props())
        .into_iter()
        .map(|(name, content)| {
            format!("## {name}\n{}", content.string())
                .trim_end_matches(js_space)
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let body = if body.is_empty() {
        String::new()
    } else {
        format!("\n{body}\n")
    };
    format!("---\n{fm}\n---\n{body}")
}
