//! Stories read the way the Node CLI's `parseStory` reads them, which is more lenient than
//! [`crate::story::parse`]: read commands (`show`, `list`) must match Node byte for byte,
//! including for files the strict parser rejects.

use crate::{config::array_index, js_space};

/// A value `parseStory` can produce, with JavaScript's semantics for printing it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Js {
    Undefined,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Js>),
    /// Properties in insertion order; a reassigned key keeps its first position.
    Obj(Vec<(String, Js)>),
}

impl Js {
    pub(crate) fn get(&self, key: &str) -> &Js {
        match self {
            Js::Obj(entries) => entries
                .iter()
                .find(|(k, _)| k == key)
                .map_or(&Js::Undefined, |(_, v)| v),
            _ => &Js::Undefined,
        }
    }

    /// The properties of an object, or none for any other value.
    pub(crate) fn props(&self) -> &[(String, Js)] {
        match self {
            Js::Obj(entries) => entries,
            _ => &[],
        }
    }

    fn set(entries: &mut Vec<(String, Js)>, key: &str, value: Js) {
        if key == "__proto__" {
            return;
        }
        match entries.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value,
            None => entries.push((key.to_owned(), value)),
        }
    }

    /// `String(value)`.
    pub(crate) fn string(&self) -> String {
        match self {
            Js::Undefined => "undefined".into(),
            Js::Bool(b) => b.to_string(),
            Js::Num(n) => number(*n),
            Js::Str(s) => s.clone(),
            Js::Arr(items) => items
                .iter()
                .map(|v| match v {
                    Js::Undefined => String::new(),
                    v => v.string(),
                })
                .collect::<Vec<_>>()
                .join(","),
            Js::Obj(_) => "[object Object]".into(),
        }
    }

    /// `JSON.stringify(value, null, 2)` for a value that is not itself undefined.
    pub(crate) fn json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out, 0);
        out
    }

    fn write_json(&self, out: &mut String, depth: usize) {
        let indent = |out: &mut String, depth: usize| out.push_str(&"  ".repeat(depth));
        match self {
            Js::Undefined => out.push_str("null"),
            Js::Bool(b) => out.push_str(&b.to_string()),
            Js::Num(n) if n.is_finite() => out.push_str(&number(*n)),
            Js::Num(_) => out.push_str("null"),
            Js::Str(s) => out.push_str(&serde_json::to_string(s).unwrap()),
            Js::Arr(items) if items.is_empty() => out.push_str("[]"),
            Js::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    item.write_json(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push(']');
            }
            Js::Obj(entries) => {
                let entries: Vec<_> = ordered(entries)
                    .into_iter()
                    .filter(|(_, v)| *v != Js::Undefined)
                    .collect();
                if entries.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (key, value)) in entries.into_iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    out.push_str(&serde_json::to_string(key).unwrap());
                    out.push_str(": ");
                    value.write_json(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push('}');
            }
        }
    }
}

/// JavaScript property order: array-index keys first, ascending, then insertion order.
pub(crate) fn ordered(entries: &[(String, Js)]) -> Vec<&(String, Js)> {
    let mut ordered: Vec<_> = entries.iter().collect();
    ordered.sort_by_key(|(k, _)| array_index(k).map_or((1, 0), |i| (0, i)));
    ordered
}

/// `Number.prototype.toString()`.
fn number(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if n < 0.0 {
        return format!("-{}", number(-n));
    }
    let scientific = format!("{n:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let digits = mantissa.replace('.', "");
    let k = digits.len() as i32;
    let point = exponent.parse::<i32>().unwrap() + 1;
    if k <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - k) as usize))
    } else if 0 < point && point <= 21 {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat(-point as usize))
    } else {
        let sign = if point - 1 < 0 { '-' } else { '+' };
        let fraction = if k == 1 {
            String::new()
        } else {
            format!(".{}", &digits[1..])
        };
        format!(
            "{}{fraction}e{sign}{}",
            &digits[..1],
            (point - 1).unsigned_abs()
        )
    }
}

fn trim_start(s: &str) -> &str {
    s.trim_start_matches(js_space)
}
fn trim(s: &str) -> &str {
    s.trim_matches(js_space)
}
fn line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `text.split(/\r?\n/)`.
fn lines(text: &str) -> Vec<&str> {
    let mut pieces: Vec<&str> = text.split('\n').collect();
    let last = pieces.len() - 1;
    for piece in &mut pieces[..last] {
        *piece = piece.strip_suffix('\r').unwrap_or(piece);
    }
    pieces
}

/// Node's `parseStory`: frontmatter fields, then a `sections` object.
pub(crate) fn parse(text: &str) -> Result<Js, String> {
    const MISSING: &str = "Story is missing YAML frontmatter (--- ... ---)";
    let start = ["---\r\n", "---\n"]
        .iter()
        .find(|open| text.starts_with(*open))
        .ok_or(MISSING)?
        .len();
    let newline = text[start..]
        .match_indices("\n---")
        .next()
        .ok_or(MISSING)?
        .0
        + start;
    let end = if newline > start && text[..newline].ends_with('\r') {
        newline - 1
    } else {
        newline
    };
    let close = newline + "\n---".len();
    let mut body = &text[close..];
    body = body.strip_prefix('\r').unwrap_or(body);
    body = body.strip_prefix('\n').unwrap_or(body);

    let mut story = Vec::new();
    for line in lines(&text[start..end]) {
        if trim(line).is_empty() || trim_start(line).starts_with('#') {
            continue;
        }
        let unsupported = || format!("Unsupported frontmatter line: {line}");
        let key_len = line
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(line.len());
        let key = &line[..key_len];
        if key.is_empty() || key.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
            return Err(unsupported());
        }
        let raw = line[key_len..].strip_prefix(':').ok_or_else(unsupported)?;
        let raw = trim_start(raw);
        if raw.contains(line_terminator) {
            return Err(unsupported());
        }
        Js::set(&mut story, key, value(strip_comment(raw))?);
    }

    let mut sections = Vec::new();
    let mut current: Option<&str> = None;
    let mut buffer: Vec<&str> = Vec::new();
    for line in lines(body) {
        if let Some(name) = heading(line) {
            if let Some(name) = current {
                Js::set(
                    &mut sections,
                    name,
                    Js::Str(trim(&buffer.join("\n")).into()),
                );
            }
            current = Some(name);
            buffer.clear();
        } else if current.is_some() {
            buffer.push(line);
        }
    }
    if let Some(name) = current {
        Js::set(
            &mut sections,
            name,
            Js::Str(trim(&buffer.join("\n")).into()),
        );
    }
    Js::set(&mut story, "sections", Js::Obj(sections));
    Ok(Js::Obj(story))
}

/// The name `/^##\s+(.+?)\s*$/` captures, if the line is a heading.
fn heading(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("##")?;
    if !rest.starts_with(js_space) {
        return None;
    }
    let name = trim_start(rest).trim_end_matches(js_space);
    if !name.is_empty() {
        return (!name.contains(line_terminator)).then_some(name);
    }
    let (i, c) = rest
        .char_indices()
        .skip(1)
        .filter(|(_, c)| !line_terminator(*c))
        .last()?;
    Some(&rest[i..i + c.len_utf8()])
}

fn strip_comment(raw: &str) -> &str {
    if raw.starts_with(['"', '\'']) {
        return raw;
    }
    trim(raw.find(" #").map_or(raw, |i| &raw[..i]))
}

fn value(raw: &str) -> Result<Js, String> {
    if matches!(raw, "" | "~" | "null") {
        return Ok(Js::Undefined);
    }
    if raw.starts_with('[') && raw.ends_with(']') {
        let inner = trim(&raw[1..raw.len() - 1]);
        if inner.is_empty() {
            return Ok(Js::Arr(Vec::new()));
        }
        return inner
            .split(',')
            .map(|item| value(trim(item)))
            .collect::<Result<_, _>>()
            .map(Js::Arr);
    }
    let quoted = |q: char| raw.len() >= 2 && raw.starts_with(q) && raw.ends_with(q);
    if quoted('"') {
        return serde_json::from_str::<String>(raw)
            .map(Js::Str)
            .map_err(|e| format!("Invalid quoted frontmatter value {raw}: {e}"));
    }
    if quoted('\'') {
        return Ok(Js::Str(raw[1..raw.len() - 1].replace("''", "'")));
    }
    if raw == "true" || raw == "false" {
        return Ok(Js::Bool(raw == "true"));
    }
    if crate::story::numeric_literal(raw) {
        return Ok(Js::Num(raw.parse().map_err(|_| "Invalid number")?));
    }
    Ok(Js::Str(raw.into()))
}

#[cfg(test)]
mod tests {
    use super::number;

    #[test]
    fn numbers_print_like_javascript() {
        for (n, text) in [
            (7.0, "7"),
            (-0.0, "0"),
            (1.5, "1.5"),
            (1e21, "1e+21"),
            (1.2345678901234568e22, "1.2345678901234568e+22"),
            (123456789012345680000.0, "123456789012345680000"),
            (1e-7, "1e-7"),
            (0.000001, "0.000001"),
            (9007199254740993.0, "9007199254740992"),
            (-2.5, "-2.5"),
        ] {
            assert_eq!(number(n), text, "{n}");
        }
    }
}
