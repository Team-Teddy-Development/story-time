//! `story set <id> key=value... [--note N] [--force]`: Node's `updateStory`, which coerces each
//! field, gates a move to `ready` on the lint, and logs the change in the changelog.

use crate::{
    config::{self, Flags},
    lenient_story::{append_changelog, ordered, trim, Js},
    lint::{self, Context, Severity},
    list,
    new_story::TYPES,
    stats, Failure,
};
use chrono::Utc;

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
const LIST: [&str; 2] = ["depends_on", "children"];
const IMMUTABLE: [&str; 4] = ["id", "created", "updated", "sections"];

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(&flags, &loaded.root, &settings)?;
    let (id, pairs) = match flags.positional.split_first() {
        Some((id, pairs)) if !id.is_empty() && !pairs.is_empty() => (id, pairs),
        _ => return Err(usage("Usage: story set <id> key=value...".into())),
    };
    let mut changes: Vec<(String, Js)> = Vec::new();
    for pair in pairs {
        let (key, raw) = match pair.find('=') {
            Some(i) if i >= 1 => (pair[..i].replace('-', "_"), &pair[i + 1..]),
            _ => return Err(usage(format!("Expected key=value, got \"{pair}\"."))),
        };
        match changes.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = Js::Str(raw.into()),
            None => changes.push((key, Js::Str(raw.into()))),
        }
    }
    let changes: Vec<(&str, String)> = ordered(&changes)
        .into_iter()
        .map(|(k, v)| (k.as_str(), v.string()))
        .collect();

    let current = list::get(&dir, id)?.ok_or_else(|| format!("Story {id} not found."))?;
    let mut next = current.props().to_vec();
    for (key, raw) in &changes {
        let key = *key;
        if IMMUTABLE.contains(&key) {
            return Err(format!("Field \"{key}\" cannot be set directly.").into());
        }
        match coerce(key, raw)? {
            Some(value) => Js::set(&mut next, key, value),
            None => next.retain(|(k, _)| k != key),
        }
    }
    let next = Js::Obj(next);

    let ready = |s: &Js| matches!(s.get("status"), Js::Str(st) if st == "ready");
    if ready(&next) && !ready(&current) && !flags.has("force") {
        let stories = list::stories(&dir, None)?;
        let ctx = Context {
            stories: Some(&stories),
            root: Some(&loaded.root),
        };
        let errors: Vec<String> = lint::lint_story(&next, &ctx)?
            .into_iter()
            .filter(|f| f.severity == Severity::Error)
            .map(|f| format!("  {}: {}", f.rule, f.message))
            .collect();
        if !errors.is_empty() {
            return Err(format!("{id} is not ready:\n{}", errors.join("\n")).into());
        }
    }

    let summary = changes
        .iter()
        .map(|(key, _)| format!("{key}={}", display(&next, key)))
        .collect::<Vec<_>>()
        .join(", ");
    let note = match flags.value("note") {
        Some(note) if !note.is_empty() => format!(" — {note}"),
        _ => String::new(),
    };
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let updated = append_changelog(&next, &format!("set {summary}{note}"), &date);
    list::save(&dir, &updated)?;
    Ok(if flags.has("json") {
        format!("{}\n", updated.json())
    } else {
        format!("{} updated.\n", updated.get("id").string())
    })
}

fn usage(message: String) -> Failure {
    Failure { code: 2, message }
}

fn coerce(key: &str, raw: &str) -> Result<Option<Js>, String> {
    if raw.is_empty() {
        return Ok(None);
    }
    if NUMERIC.contains(&key) {
        let n = stats::number(raw);
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
