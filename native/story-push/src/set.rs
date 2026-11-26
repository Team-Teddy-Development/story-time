//! `story set <id> key=value... [--note N] [--force]`: Node's `updateStory`, which coerces each
//! field, gates a move to `ready` on the lint, and logs the change in the changelog.

use crate::{
    adoption_apply, blocked,
    config::{self, Flags},
    js_number,
    lenient_story::{append_changelog, ordered, trim, Js},
    lint::{self, Context, Severity},
    list,
    new_story::TYPES,
    org_index::OrgIndex,
    Failure,
};
use chrono::Utc;
use std::path::Path;

const STATUSES: [&str; 7] = [
    "draft",
    "refined",
    "ready",
    "in-progress",
    "done",
    "split",
    "archived",
];
const NUMERIC: [&str; 4] = ["value", "time_criticality", "risk_reduction", "size"];
const LIST: [&str; 4] = ["depends_on", "relates_to", "children", "labels"];
pub(crate) const IMMUTABLE: [&str; 4] = ["id", "created", "updated", "sections"];

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let (id, pairs) = match flags.positional.split_first() {
        Some((id, pairs)) if !id.is_empty() && !pairs.is_empty() => (id, pairs),
        _ => return Err(usage("Usage: story set <id> key=value...".into())),
    };
    let mut parsed = Vec::new();
    for pair in pairs {
        match pair.find('=') {
            Some(i) if i >= 1 => parsed.push((pair[..i].to_owned(), pair[i + 1..].to_owned())),
            _ => return Err(usage(format!("Expected key=value, got \"{pair}\"."))),
        }
    }
    let _lock = adoption_apply::lock_for_write(&loaded.root)?;
    let current = list::get(&dir, id)?.ok_or_else(|| format!("Story {id} not found."))?;
    let note = flags.value("note");
    let change = apply(
        &current,
        &changes(&parsed),
        note,
        flags.has("force"),
        &dir,
        &loaded.root,
        None,
    )
    .map_err(|refusal| match refusal {
        Refusal::Immutable(key) => format!("Field \"{key}\" cannot be set directly."),
        Refusal::Invalid(message) | Refusal::Other(message) => message,
        Refusal::NotReady(findings) => format!(
            "{id} is not ready:\n{}",
            findings
                .iter()
                .map(|f| format!("  {}: {}", f.rule, f.message))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    })?;
    let done = |s: &Js| s.get("status").string() == "done";
    let unblocked = if done(&change.story) && !done(&current) {
        blocked::unblocked(&list::stories(&dir, None)?, id)
    } else {
        Vec::new()
    };
    list::save(&dir, &change.story)?;
    let mut out = format!("{} updated.\n", change.story.get("id").string());
    for parent in &unblocked {
        list::save(&dir, parent)?;
        let parent = parent.get("id").string();
        out.push_str(&format!("{parent} unblocked: every dependency is done.\n"));
    }
    Ok(if flags.has("json") {
        format!("{}\n", change.story.json())
    } else {
        out
    })
}

/// Requested `key=value` pairs in `story set`'s order: `-` in a key reads as `_`, a repeated key
/// keeps its first place and its last value, and array-index keys sort first as in a JS object.
pub(crate) fn changes(pairs: &[(String, String)]) -> Vec<(String, String)> {
    let mut changes: Vec<(String, Js)> = Vec::new();
    for (key, raw) in pairs {
        let key = key.replace('-', "_");
        match changes.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = Js::Str(raw.clone()),
            None => changes.push((key, Js::Str(raw.clone()))),
        }
    }
    ordered(&changes)
        .into_iter()
        .map(|(k, v)| (k.clone(), v.string()))
        .collect()
}

pub(crate) enum Refusal {
    Immutable(String),
    Invalid(String),
    NotReady(Vec<lint::Finding>),
    Other(String),
}

pub(crate) struct Change {
    pub(crate) story: Js,
    pub(crate) line: String,
}

/// `updateStory`: the story with each change coerced, a move to `ready` gated on the lint
/// unless forced, and one changelog line dated today. The caller saves it.
pub(crate) fn apply(
    current: &Js,
    changes: &[(String, String)],
    note: Option<&str>,
    force: bool,
    dir: &Path,
    root: &Path,
    org: Option<&OrgIndex>,
) -> Result<Change, Refusal> {
    let mut next = current.props().to_vec();
    for (key, raw) in changes {
        let key = key.as_str();
        if IMMUTABLE.contains(&key) {
            return Err(Refusal::Immutable(key.into()));
        }
        match coerce(key, raw).map_err(Refusal::Invalid)? {
            Some(value) => Js::set(&mut next, key, value),
            None => next.retain(|(k, _)| k != key),
        }
    }
    let next = Js::Obj(next);

    let ready = |s: &Js| matches!(s.get("status"), Js::Str(st) if st == "ready");
    if ready(&next) && !ready(current) && !force {
        let stories = list::stories(dir, None).map_err(Refusal::Other)?;
        let ctx = Context {
            stories: Some(&stories),
            root: Some(root),
            org,
            ..Context::default()
        };
        let errors: Vec<lint::Finding> = lint::lint_story(&next, &ctx)
            .map_err(Refusal::Other)?
            .into_iter()
            .filter(|f| f.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(Refusal::NotReady(errors));
        }
    }

    let summary = changes
        .iter()
        .map(|(key, _)| format!("{key}={}", display(&next, key)))
        .collect::<Vec<_>>()
        .join(", ");
    let note = match note {
        Some(note) if !note.is_empty() => format!(" — {note}"),
        _ => String::new(),
    };
    let line = format!("set {summary}{note}");
    let date = Utc::now().format("%Y-%m-%d").to_string();
    Ok(Change {
        story: append_changelog(&next, &line, &date),
        line,
    })
}

fn usage(message: String) -> Failure {
    Failure { code: 2, message }
}

pub(crate) fn coerce(key: &str, raw: &str) -> Result<Option<Js>, String> {
    if raw.is_empty() {
        return Ok(None);
    }
    if NUMERIC.contains(&key) {
        let n = js_number::string_to_number(raw);
        if !n.is_finite() {
            return Err(format!("{key} must be a number, got \"{raw}\"."));
        }
        return Ok(Some(Js::Num(n)));
    }
    if LIST.contains(&key) {
        let text = trim(raw);
        let items = text
            .strip_prefix('[')
            .and_then(|t| t.strip_suffix(']'))
            .unwrap_or(text);
        return Ok(Some(Js::Arr(
            items
                .split(',')
                .map(trim)
                .filter(|s| !s.is_empty())
                .map(|s| Js::Str(s.into()))
                .collect(),
        )));
    }
    if key == "status" && !STATUSES.contains(&raw) {
        return Err(format!(
            "Unknown status \"{raw}\". Use one of: {}.",
            STATUSES.join(", ")
        ));
    }
    if key == "sprint" && !crate::sprint::valid_id(raw) {
        return Err(format!(
            "sprint must be YYYY-Snn (for example 2026-S23), got \"{raw}\"."
        ));
    }
    if key == "type" && !TYPES.contains(&raw) {
        return Err(format!(
            "Unknown type \"{raw}\". Use one of: {}.",
            TYPES.join(", ")
        ));
    }
    Ok(Some(Js::Str(raw.into())))
}

/// `String(next[key])`, or empty when undefined. A key the story lacks still reads through the
/// object prototype in Node, so `__proto__` and the `Object.prototype` methods print as it does.
fn display(next: &Js, key: &str) -> String {
    const METHODS: [&str; 10] = [
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toLocaleString",
        "toString",
        "valueOf",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
    ];
    match next.get(key) {
        _ if key == "__proto__" => "[object Object]".into(),
        Js::Undefined if key == "constructor" => "function Object() { [native code] }".into(),
        Js::Undefined if METHODS.contains(&key) => format!("function {key}() {{ [native code] }}"),
        Js::Undefined => String::new(),
        v => v.string(),
    }
}
