//! `story show <id> [--json]`, reading a story the way the Node CLI's lenient `parseStory` does.

use crate::{config, Failure};
use std::{fs, io::ErrorKind, path::Path};

/// A frontmatter value as Node holds it after parsing.
enum Js {
    Undefined,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Js>),
}

/// A parsed story: keys keep first-insertion order and later duplicates replace the value, as in a JavaScript object.
struct Story {
    meta: Vec<(String, Js)>,
    sections: Vec<(String, String)>,
}

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
    let story = parse(&String::from_utf8_lossy(&bytes))?;
    Ok(if flags.has("json") {
        let mut out = String::new();
        json_story(&story, &mut out);
        format!("{out}\n")
    } else {
        format!("{}\n", serialize(&story).trim_end_matches(js_space))
    })
}

/// JavaScript `\s`, which differs from Unicode White_Space by U+0085 and U+FEFF.
fn js_space(c: char) -> bool {
    c == '\u{feff}' || (c != '\u{85}' && c.is_whitespace())
}

/// Characters JavaScript's `.` refuses to match.
fn line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(js_space)
}

/// `str.split(/\r?\n/)`.
fn split_lines(s: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = s.split('\n').collect();
    let last = lines.len() - 1;
    for line in &mut lines[..last] {
        *line = line.strip_suffix('\r').unwrap_or(line);
    }
    lines
}

fn insert<V>(entries: &mut Vec<(String, V)>, key: &str, value: V) {
    if key == "__proto__" {
        return;
    }
    match entries.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value,
        None => entries.push((key.to_owned(), value)),
    }
}

/// `/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/`, then the frontmatter lines and the `## ` sections.
fn parse(text: &str) -> Result<Story, String> {
    const MISSING: &str = "Story is missing YAML frontmatter (--- ... ---)";
    let rest = text.strip_prefix("---").ok_or(MISSING)?;
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .ok_or(MISSING)?;
    let close = rest.find("\n---").ok_or(MISSING)?;
    let front = &rest[..close];
    let front = front.strip_suffix('\r').unwrap_or(front);
    let body = &rest[close + 4..];
    let body = body.strip_prefix('\r').unwrap_or(body);
    let body = body.strip_prefix('\n').unwrap_or(body);

    let mut meta = Vec::new();
    for line in split_lines(front) {
        if js_trim(line).is_empty() || line.trim_start_matches(js_space).starts_with('#') {
            continue;
        }
        let unsupported = || format!("Unsupported frontmatter line: {line}");
        let (key, raw) = line.split_once(':').ok_or_else(unsupported)?;
        let raw = raw.trim_start_matches(js_space);
        let key_ok = key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !key_ok || raw.contains(line_terminator) {
            return Err(unsupported());
        }
        insert(&mut meta, key, parse_value(strip_comment(raw))?);
    }

    let mut sections = Vec::new();
    let mut current: Option<String> = None;
    let mut buf: Vec<&str> = Vec::new();
    for line in split_lines(body) {
        if let Some(name) = heading(line) {
            if let Some(name) = current.replace(name) {
                insert(&mut sections, &name, js_trim(&buf.join("\n")).to_owned());
            }
            buf.clear();
        } else if current.is_some() {
            buf.push(line);
        }
    }
    if let Some(name) = current {
        insert(&mut sections, &name, js_trim(&buf.join("\n")).to_owned());
    }
    Ok(Story { meta, sections })
}

/// The name captured by `/^##\s+(.+?)\s*$/`, or None when the line is not a heading.
fn heading(line: &str) -> Option<String> {
    let rest: Vec<char> = line.strip_prefix("##")?.chars().collect();
    let lead = rest.iter().take_while(|c| js_space(**c)).count();
    if lead == 0 {
        return None;
    }
    if lead < rest.len() {
        let name: String = rest[lead..].iter().collect();
        let name = name.trim_end_matches(js_space);
        return (!name.contains(line_terminator)).then(|| name.to_owned());
    }
    (1..rest.len())
        .rev()
        .find(|&i| !line_terminator(rest[i]))
        .map(|i| rest[i].to_string())
}

fn strip_comment(v: &str) -> &str {
    if v.starts_with(['"', '\'']) {
        return v;
    }
    js_trim(v.find(" #").map_or(v, |i| &v[..i]))
}

fn number(v: &str) -> bool {
    let v = v.strip_prefix('-').unwrap_or(v);
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    match v.split_once('.') {
        Some((int, frac)) => digits(int) && digits(frac),
        None => digits(v),
    }
}

fn quoted(v: &str, q: char) -> bool {
    v.len() >= 2 && v.starts_with(q) && v.ends_with(q) && !v.contains(line_terminator)
}

fn parse_value(v: &str) -> Result<Js, String> {
    if v.is_empty() || v == "~" || v == "null" {
        return Ok(Js::Undefined);
    }
    if v.starts_with('[') && v.ends_with(']') {
        let inner = js_trim(&v[1..v.len() - 1]);
        if inner.is_empty() {
            return Ok(Js::Arr(Vec::new()));
        }
        return inner
            .split(',')
            .map(|x| parse_value(js_trim(x)))
            .collect::<Result<_, _>>()
            .map(Js::Arr);
    }
    if quoted(v, '"') {
        return serde_json::from_str::<String>(v)
            .map(Js::Str)
            .map_err(|e| format!("Invalid quoted frontmatter value {v}: {e}"));
    }
    if quoted(v, '\'') {
        return Ok(Js::Str(v[1..v.len() - 1].replace("''", "'")));
    }
    if v == "true" || v == "false" {
        return Ok(Js::Bool(v == "true"));
    }
    if number(v) {
        return Ok(Js::Num(v.parse().map_err(|_| "Invalid number")?));
    }
    Ok(Js::Str(v.to_owned()))
}

/// `String(n)` for a finite JavaScript number.
fn js_number(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    let sign = if n < 0.0 { "-" } else { "" };
    let exp = format!("{:e}", n.abs());
    let (mantissa, e) = exp.split_once('e').unwrap();
    let digits = mantissa.replace('.', "");
    let k = digits.len() as i32;
    let point: i32 = e.parse::<i32>().unwrap() + 1;
    let body = if k <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - k) as usize))
    } else if 0 < point && point <= 21 {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else {
        let tail = if k == 1 {
            String::new()
        } else {
            format!(".{}", &digits[1..])
        };
        let e = point - 1;
        format!(
            "{}{tail}e{}{}",
            &digits[..1],
            if e < 0 { "-" } else { "+" },
            e.abs()
        )
    };
    format!("{sign}{body}")
}

fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap()
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
        Js::Num(n) => return js_number(*n),
        Js::Bool(b) => return b.to_string(),
        Js::Undefined => "undefined".to_owned(),
        Js::Str(s) => s.clone(),
    };
    let needs_quotes = s.is_empty()
        || s.contains([':', '#', '[', ']', ',', '"', '\''])
        || s.starts_with(js_space)
        || s.ends_with(js_space)
        || matches!(s.as_str(), "true" | "false" | "null" | "~")
        || number(&s);
    if needs_quotes {
        json_string(&s)
    } else {
        s
    }
}

/// JavaScript object key order: array-index keys first, ascending, then insertion order.
fn js_order<V>(entries: &[(String, V)]) -> Vec<&(String, V)> {
    let mut ordered: Vec<_> = entries.iter().collect();
    ordered.sort_by_key(|(k, _)| config::array_index(k).map_or((1, 0), |i| (0, i)));
    ordered
}

/// `serializeStory` from the Node domain module.
fn serialize(story: &Story) -> String {
    let get = |k: &str| story.meta.iter().find(|(key, _)| key == k);
    let mut extras: Vec<&(String, Js)> = js_order(&story.meta)
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
    let body = js_order(&story.sections)
        .into_iter()
        .map(|(name, content)| {
            format!("## {name}\n{content}")
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

fn json_value(v: &Js, indent: usize, out: &mut String) {
    match v {
        Js::Undefined => out.push_str("null"),
        Js::Bool(b) => out.push_str(&b.to_string()),
        Js::Num(n) => out.push_str(&js_number(*n)),
        Js::Str(s) => out.push_str(&json_string(s)),
        Js::Arr(items) if items.is_empty() => out.push_str("[]"),
        Js::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.push_str(&" ".repeat(indent + 2));
                json_value(item, indent + 2, out);
            }
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            out.push(']');
        }
    }
}

/// `JSON.stringify(story, null, 2)`: undefined properties are left out, and `sections` keeps the position a frontmatter `sections` key gave it.
fn json_story(story: &Story, out: &mut String) {
    enum Field<'a> {
        Meta(&'a Js),
        Sections,
    }
    let mut fields: Vec<(String, Field)> = story
        .meta
        .iter()
        .map(|(k, v)| (k.clone(), Field::Meta(v)))
        .collect();
    insert(&mut fields, "sections", Field::Sections);
    let fields: Vec<_> = js_order(&fields)
        .into_iter()
        .filter(|(_, f)| !matches!(f, Field::Meta(Js::Undefined)))
        .collect();
    out.push('{');
    for (i, (key, field)) in fields.iter().enumerate() {
        out.push_str(if i == 0 { "\n  " } else { ",\n  " });
        out.push_str(&json_string(key));
        out.push_str(": ");
        match field {
            Field::Meta(v) => json_value(v, 2, out),
            Field::Sections if story.sections.is_empty() => out.push_str("{}"),
            Field::Sections => {
                out.push('{');
                for (j, (name, content)) in js_order(&story.sections).into_iter().enumerate() {
                    out.push_str(if j == 0 { "\n    " } else { ",\n    " });
                    out.push_str(&json_string(name));
                    out.push_str(": ");
                    out.push_str(&json_string(content));
                }
                out.push_str("\n  }");
            }
        }
    }
    out.push_str(if fields.is_empty() { "}" } else { "\n}" });
}
