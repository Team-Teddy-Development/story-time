//! Node's `serializeStory`: the one writer of the story file format, shared by `show` and `new`.

use crate::{
    js_space,
    lenient_story::{ordered, Js},
    story::numeric_literal,
};

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
pub(crate) fn serialize(story: &Js) -> String {
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
