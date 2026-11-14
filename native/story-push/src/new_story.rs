//! `story new`: Node's `createStory` writing a draft story file with the next free id. Like
//! Node, it never contacts a tracker; `story push` publishes the story afterwards.

use crate::{
    config::{self, Flags},
    lenient_story::{trim, Js},
    list::{self, backlog_dir},
    next_id::next_id,
    Failure,
};
use chrono::Utc;
use std::path::Path;

const TYPES: [&str; 4] = ["story", "spike", "bug", "chore"];
const STATEMENT_TODO: &str = "TODO: As a <persona>, I want <goal>, so that <benefit>.";

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = backlog_dir(&flags, &loaded.root, &settings)?;
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
            fields.push((field.into(), Js::Num(js_number(value))));
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

/// JavaScript's `Number(string)`.
fn js_number(s: &str) -> f64 {
    let t = trim(s);
    if t.is_empty() {
        return 0.0;
    }
    for (marker, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        let digits = t
            .strip_prefix(marker)
            .or_else(|| t.strip_prefix(&marker.to_uppercase()));
        if let Some(digits) = digits {
            return if !digits.is_empty() && digits.chars().all(|c| c.is_digit(radix)) {
                digits.chars().fold(0.0, |n, c| {
                    n * f64::from(radix) + f64::from(c.to_digit(radix).unwrap())
                })
            } else {
                f64::NAN
            };
        }
    }
    let (sign, body) = match t.strip_prefix('-') {
        Some(body) => (-1.0, body),
        None => (1.0, t.strip_prefix('+').unwrap_or(t)),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let valid_exponent = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    if (int.is_empty() && frac.is_empty()) || !digits(int) || !digits(frac) || !valid_exponent {
        return f64::NAN;
    }
    sign * body.parse::<f64>().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::js_number;

    #[test]
    fn strings_convert_like_javascript_number() {
        for (input, expected) in [
            ("3", 3.0),
            (" 1e3 ", 1000.0),
            ("", 0.0),
            ("0x10", 16.0),
            ("0B101", 5.0),
            (".5", 0.5),
            ("5.", 5.0),
            ("-1.50", -1.5),
            ("+2", 2.0),
            ("-Infinity", f64::NEG_INFINITY),
        ] {
            assert_eq!(js_number(input), expected, "{input:?}");
        }
        for input in [
            "abc", "1_000", "inf", "NaN", "0x", "-0x10", "1e", ".", "+-5",
        ] {
            assert!(js_number(input).is_nan(), "{input:?}");
        }
        assert!(js_number("-0").is_sign_negative());
    }
}
