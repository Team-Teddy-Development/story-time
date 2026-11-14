use serde_json::{json, Map, Value};

const KEY_ORDER: &[&str] = &[
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
const INTERNAL: &[&str] = &[
    "Story",
    "Question",
    "Exit Criteria",
    "Acceptance Criteria",
    "Changelog",
    "Test Plan",
];

fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub(crate) fn numeric_literal(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let mut pieces = s.split('.');
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    digits(pieces.next().unwrap_or(""))
        && pieces.next().is_none_or(digits)
        && pieces.next().is_none()
}

fn format_value(v: &Value) -> Result<String, String> {
    match v {
        Value::Null => Ok("null".into()),
        Value::Bool(_) | Value::Number(_) => Ok(display(v)),
        Value::Array(items) => {
            if items.iter().any(|v| v.is_array() || v.is_object()) {
                return Err("Nested frontmatter collections are unsupported".into());
            }
            Ok(format!(
                "[{}]",
                items
                    .iter()
                    .map(format_value)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            ))
        }
        Value::String(s) => {
            let quote = s.is_empty()
                || s.trim() != s
                || s.contains([':', '#', '[', ']', ',', '"', '\'', '\n', '\r'])
                || matches!(s.as_str(), "true" | "false" | "null" | "~")
                || numeric_literal(s)
                || s.starts_with(['{', '&', '*', '!', '|', '>']);
            Ok(if quote {
                serde_json::to_string(s).map_err(|e| e.to_string())?
            } else {
                s.clone()
            })
        }
        Value::Object(_) => Err("Object-valued frontmatter is unsupported".into()),
    }
}

fn object(story: &Value) -> Result<&Map<String, Value>, String> {
    story.as_object().ok_or("Story must be an object".into())
}
fn sections(story: &Value) -> Result<Option<&Map<String, Value>>, String> {
    match story.get("sections") {
        None => Ok(None),
        Some(Value::Object(s)) if s.values().all(Value::is_string) => Ok(Some(s)),
        _ => Err("Story sections must be an object of strings".into()),
    }
}

pub fn serialize(story: &Value) -> Result<String, String> {
    let meta = object(story)?;
    let mut keys: Vec<&str> = KEY_ORDER
        .iter()
        .copied()
        .filter(|k| meta.contains_key(*k))
        .collect();
    let mut extras: Vec<&str> = meta
        .keys()
        .map(String::as_str)
        .filter(|k| *k != "sections" && !KEY_ORDER.contains(k))
        .collect();
    extras.sort_unstable();
    keys.extend(extras);
    let mut fm = Vec::new();
    for key in keys {
        if !valid_key(key) {
            return Err(format!("Unsupported frontmatter key: {key}"));
        }
        fm.push(format!("{key}: {}", format_value(&meta[key])?));
    }
    let mut body = Vec::new();
    if let Some(sections) = sections(story)? {
        for (name, content) in sections {
            if name.trim() != name || name.is_empty() || name.contains(['\n', '\r']) {
                return Err("Invalid section name".into());
            }
            let content = content.as_str().unwrap();
            if content.lines().any(|line| {
                line.strip_prefix("##").is_some_and(|tail| {
                    tail.starts_with(char::is_whitespace) && !tail.trim().is_empty()
                })
            }) {
                return Err("Section content contains another section heading".into());
            }
            body.push(format!("## {name}\n{content}").trim_end().to_owned());
        }
    }
    Ok(format!(
        "---\n{}\n---\n{}",
        fm.join("\n"),
        if body.is_empty() {
            String::new()
        } else {
            format!("\n{}\n", body.join("\n\n"))
        }
    ))
}

pub fn link(
    story: &Value,
    remote_id: &str,
    url: &str,
    date: &str,
    action: &str,
) -> Result<Value, String> {
    object(story)?;
    sections(story)?;
    let verb = match action {
        "create" => "pushed to tracker as",
        "update" => "updated tracker item",
        "link" => "linked existing tracker item",
        _ => return Err(format!("Unsupported link action: {action}")),
    };
    let remote = if !remote_id.is_empty() && remote_id.bytes().all(|c| c.is_ascii_digit()) {
        let number = remote_id
            .parse::<u64>()
            .map_err(|_| "Remote ID exceeds integer range")?;
        if number > 9_007_199_254_740_991 {
            return Err("Remote ID exceeds exact numeric range".into());
        }
        json!(number)
    } else {
        json!(remote_id)
    };
    let mut out = story.clone();
    out.as_object_mut()
        .ok_or("Story must be an object")?
        .remove("pending_create");
    out["remote"] = remote;
    out["remote_url"] = json!(url);
    out["updated"] = json!(date);
    let prev = story
        .get("sections")
        .and_then(|s| s.get("Changelog"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_end();
    let entry = format!("- {date} {verb} {remote_id}");
    if out.get("sections").is_none() {
        out["sections"] = json!({});
    }
    out["sections"]["Changelog"] = json!(if prev.is_empty() {
        entry
    } else {
        format!("{prev}\n{entry}")
    });
    Ok(out)
}

fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn section<'a>(s: &'a Value, key: &str) -> &'a str {
    s.get("sections")
        .map(|v| string(v, key))
        .unwrap_or("")
        .trim()
}
fn display(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(n) = v.as_f64() {
        if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 {
            return format!("{n:.0}");
        }
    }
    v.to_string()
}
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
fn lead(s: &Value) -> String {
    if string(s, "type") == "spike" {
        let mut out = format!("### Question\n{}", section(s, "Question"));
        if truthy(s.get("timebox")) {
            out.push_str(&format!("\n\nTimebox: {}", display(&s["timebox"])));
        }
        out
    } else {
        section(s, "Story").into()
    }
}

pub fn acceptance_criteria(s: &Value) -> Vec<Value> {
    let raw = s
        .get("sections")
        .and_then(|v| {
            v.get("Acceptance Criteria")
                .or_else(|| v.get("Exit Criteria"))
        })
        .and_then(Value::as_str)
        .unwrap_or("");
    raw.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let tail = line.strip_prefix('-').or_else(|| line.strip_prefix('*'))?;
            if !tail.starts_with(char::is_whitespace) {
                return None;
            }
            let tail = tail.trim_start();
            if tail.is_empty() {
                return None;
            }
            let (text, done) =
                if (tail.starts_with("[ ]") || tail.starts_with("[x]") || tail.starts_with("[X]"))
                    && tail[3..].starts_with(char::is_whitespace)
                    && !tail[3..].trim().is_empty()
                {
                    (&tail[3..], !tail.starts_with("[ ]"))
                } else {
                    (tail, false)
                };
            Some(json!({"text":text.trim(), "done":done}))
        })
        .collect()
}

fn markdown_body(s: &Value) -> String {
    let mut parts = vec![lead(s)];
    let name = if string(s, "type") == "spike" {
        "Exit Criteria"
    } else {
        "Acceptance Criteria"
    };
    if !section(s, name).is_empty() {
        parts.push(format!("### {name}\n{}", section(s, name)));
    }
    if let Some(sections) = s["sections"].as_object() {
        for name in sections
            .keys()
            .filter(|name| !INTERNAL.contains(&name.as_str()))
        {
            if !section(s, name).is_empty() {
                parts.push(format!("### {name}\n{}", section(s, name)));
            }
        }
    }
    parts
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}
fn footer(s: &Value) -> String {
    let score = (|| {
        let size = s["size"].as_f64()?;
        if size <= 0.0 {
            return None;
        }
        let sum = s["value"].as_f64()?
            + s["time_criticality"].as_f64()?
            + s["risk_reduction"].as_f64()?;
        let score = ((sum / size * 100.0 + 0.5).floor()) / 100.0;
        score.is_finite().then_some(score)
    })();
    format!(
        "<sub>story-time {}{}</sub>",
        string(s, "id"),
        score.map(|n| format!(" · WSJF {n:.2}")).unwrap_or_default()
    )
}

fn tracker_title(story: &Value) -> String {
    let title = string(story, "title");
    let prefix = format!("{}: ", string(story, "id"));
    if title.starts_with(&prefix) {
        title.to_owned()
    } else {
        format!("{prefix}{title}")
    }
}

/// The card push sends: [`render_card`] with the story ID prefixed to the title.
pub fn tracker_card(story: &Value, tracker: &str, config: &Value) -> Result<Value, String> {
    let mut titled = story.clone();
    if story.get("title").is_some_and(Value::is_string) {
        titled["title"] = json!(tracker_title(story));
    }
    render_card(&titled, tracker, config)
}

/// Node's `renderCard`: the card `story export` prints.
pub fn render_card(story: &Value, tracker: &str, config: &Value) -> Result<Value, String> {
    object(story)?;
    sections(story)?;
    for key in ["id", "title"] {
        if !story[key].is_string() {
            return Err(format!("Story {key} must be a string"));
        }
    }
    let title = string(story, "title");
    match tracker {
        "github" | "gitea" => {
            let mut labels = vec![story
                .get("type")
                .filter(|v| !v.is_null())
                .map(display)
                .unwrap_or("story".into())];
            for key in ["size", "value"] {
                if story[key].is_number() {
                    labels.push(format!("{key}:{}", display(&story[key])));
                }
            }
            if truthy(story.get("epic")) {
                labels.push(format!("epic:{}", display(&story["epic"])));
            }
            let body = format!("{}\n\n{}", markdown_body(story), footer(story));
            let text = format!(
                "**Title:** {title}\n**Labels:** {}\n\n{body}",
                labels
                    .iter()
                    .map(|v| format!("`{v}`"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            Ok(json!({"format":tracker,"title":title,"labels":labels,"body":body,"text":text}))
        }
        "markdown" => {
            let mut meta = Vec::new();
            for (key, label) in [
                ("size", "Size"),
                ("value", "Value"),
                ("epic", "Epic"),
                ("status", "Status"),
            ] {
                if if key == "size" || key == "value" {
                    story[key].is_number()
                } else {
                    truthy(story.get(key))
                } {
                    meta.push(format!("**{label}:** {}", display(&story[key])));
                }
            }
            let body = [markdown_body(story), meta.join(" · "), footer(story)]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(
                json!({"format":"markdown","title":title,"body":body,"text":format!("## {title}\n\n{body}")}),
            )
        }
        "ado" => {
            let fields = ado_fields(story, config)?;
            let text = fields
                .iter()
                .map(|f| format!("{}\n```\n{}\n```", string(f, "name"), string(f, "value")))
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(json!({"format":"ado","title":title,"fields":fields,"text":text}))
        }
        _ => Err(format!(
            "Unknown export format \"{tracker}\". Use one of: github, ado, gitea, markdown."
        )),
    }
}

fn ado_fields(s: &Value, config: &Value) -> Result<Vec<Value>, String> {
    let ado = &config["ado"];
    let process = ado
        .get("process")
        .and_then(Value::as_str)
        .unwrap_or("agile");
    let (story_type, bug_type, points, points_ref, criteria_field) = match process {
        "agile" => (
            "User Story",
            "Bug",
            "Story Points",
            "Microsoft.VSTS.Scheduling.StoryPoints",
            true,
        ),
        "scrum" => (
            "Product Backlog Item",
            "Bug",
            "Effort",
            "Microsoft.VSTS.Scheduling.Effort",
            true,
        ),
        "basic" => (
            "Issue",
            "Issue",
            "Effort",
            "Microsoft.VSTS.Scheduling.Effort",
            false,
        ),
        _ => return Err(format!("Unknown ADO process: {process}")),
    };
    let bug = string(s, "type") == "bug";
    let criteria = acceptance_criteria(s)
        .iter()
        .map(|ac| format!("- {}", string(ac, "text")))
        .collect::<Vec<_>>()
        .join("\n");
    let mut description = lead(s);
    if let Some(question) = description.strip_prefix("### Question\n") {
        description = format!("Question: {question}");
    }
    if !criteria_field && !criteria.is_empty() {
        description.push_str(&format!("\n\nAcceptance Criteria:\n{criteria}"));
    }
    if !section(s, "Out of Scope").is_empty() {
        description.push_str(&format!(
            "\n\nOut of Scope:\n{}",
            section(s, "Out of Scope")
        ));
    }
    let mut fields = vec![
        json!({"name":"Work Item Type","value":if bug {bug_type} else {story_type}}),
        json!({"name":"Title","value":string(s, "title")}),
    ];
    if bug && bug_type == "Bug" {
        let mut parts = vec![description];
        let steps = ["Steps to Reproduce", "Expected", "Actual"]
            .iter()
            .filter(|n| !section(s, n).is_empty())
            .map(|n| format!("{n}:\n{}", section(s, n)))
            .collect::<Vec<_>>()
            .join("\n\n");
        parts.push(steps);
        fields.push(json!({"name":"Repro Steps","value":parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n"),"ref":"Microsoft.VSTS.TCM.ReproSteps"}));
    } else {
        fields.push(json!({"name":"Description","value":description,"ref":"System.Description"}));
    }
    if criteria_field && !criteria.is_empty() {
        fields.push(json!({"name":"Acceptance Criteria","value":criteria,"ref":"Microsoft.VSTS.Common.AcceptanceCriteria"}));
    }
    if s["size"].is_number() {
        fields.push(json!({"name":points,"value":display(&s["size"]),"ref":points_ref}));
    }
    let mut tags = vec!["story-time".into(), string(s, "id").into()];
    if truthy(s.get("type")) && string(s, "type") != "story" && !bug {
        tags.push(display(&s["type"]));
    }
    if truthy(s.get("epic")) {
        tags.push(format!("epic:{}", display(&s["epic"])));
    }
    fields.push(json!({"name":"Tags","value":tags.join("; "),"ref":"System.Tags"}));
    for (key, name, reference) in [
        ("areaPath", "Area Path", "System.AreaPath"),
        ("iterationPath", "Iteration Path", "System.IterationPath"),
    ] {
        if truthy(ado.get(key)) {
            fields.push(json!({"name":name,"value":ado[key],"ref":reference}));
        }
    }
    Ok(fields)
}

pub fn to_html(text: &str) -> String {
    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    let lines: Vec<_> = text.split('\n').collect();
    let items: Vec<_> = lines.iter().filter_map(|s| s.strip_prefix("- ")).collect();
    if !items.is_empty() && items.len() == lines.iter().filter(|s| !s.is_empty()).count() {
        return format!(
            "<ul>{}</ul>",
            items
                .iter()
                .map(|s| format!("<li>{}</li>", escape(s)))
                .collect::<String>()
        );
    }
    let mut paragraphs = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\n' && bytes[i + 1] == b'\n' {
            paragraphs.push(&text[start..i]);
            while i < bytes.len() && bytes[i] == b'\n' {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    paragraphs.push(&text[start..]);
    paragraphs
        .iter()
        .map(|p| format!("<p>{}</p>", escape(p).replace('\n', "<br>")))
        .collect()
}

/// The `az boards work-item` arguments for a rendered ADO card. Push and the printed
/// `az` command both use it, so the two cannot disagree about field mapping.
#[derive(Debug)]
pub struct AdoArgs {
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub area: Option<String>,
    pub iteration: Option<String>,
    pub fields: Vec<String>,
}

/// Node's `adoArgs`, over the card [`render_card`] or [`tracker_card`] returns.
pub fn ado_args(card: &Value) -> Result<AdoArgs, String> {
    let fields = card["fields"]
        .as_array()
        .ok_or("Rendered ADO card has invalid fields.")?;
    let get = |name| {
        fields
            .iter()
            .find(|f| string(f, "name") == name)
            .map(|f| string(f, "value").to_owned())
    };
    Ok(AdoArgs {
        kind: get("Work Item Type").ok_or("Rendered ADO card has no work item type.")?,
        title: string(card, "title").to_owned(),
        description: get("Description").map(|d| to_html(&d)),
        area: get("Area Path").filter(|v| !v.is_empty()),
        iteration: get("Iteration Path").filter(|v| !v.is_empty()),
        fields: fields
            .iter()
            .filter_map(|f| {
                let reference = f.get("ref")?.as_str()?;
                if [
                    "System.Description",
                    "System.AreaPath",
                    "System.IterationPath",
                ]
                .contains(&reference)
                {
                    return None;
                }
                let value = string(f, "value");
                Some(format!(
                    "{reference}={}",
                    if reference.contains("AcceptanceCriteria") || reference.contains("ReproSteps")
                    {
                        to_html(value)
                    } else {
                        value.into()
                    }
                ))
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reread(text: &str) -> Value {
        crate::lenient_story::parse(text).unwrap().to_value()
    }
    fn story() -> Value {
        reread("---\nid: ST-001\ntitle: Reset password\nstatus: ready\ntype: story\nepic: EP-1\nvalue: 8\ntime_criticality: 5\nrisk_reduction: 3\nsize: 3\ndepends_on: []\n---\n\n## Story\nAs a user, I want reset.\n\n## Acceptance Criteria\n- [ ] Given email <exists>\n* [X] Given token & valid\n\n## Notes\nKeep this.\n\n## Out of Scope\n- SMS reset\n\n## Changelog\n- 2026-11-02 created\n")
    }
    fn field(card: &Value, name: &str) -> Value {
        card["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .map(|f| f["value"].clone())
            .unwrap_or(Value::Null)
    }
    #[test]
    fn roundtrip_order_quotes_and_arrays() {
        let mut s = story();
        s["title"] = json!("Fix: login \"quoted\" # hash");
        s["depends_on"] = json!(["ST-1", "quote's", "true", "123"]);
        s["custom"] = json!("null");
        let text = serialize(&s).unwrap();
        assert_eq!(reread(&text), s);
        assert!(text.find("id:").unwrap() < text.find("title:").unwrap());
        assert_eq!(
            s["sections"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "Story",
                "Acceptance Criteria",
                "Notes",
                "Out of Scope",
                "Changelog"
            ]
        );
        assert_eq!(
            reread("---\nid: 'it''s'\nflag: true # comment\n---\n")["id"],
            "it's"
        );
        assert_eq!(reread(&text.replace('\n', "\r\n")), s);
    }
    #[test]
    fn rejects_unrenderable_input() {
        assert!(serialize(&json!({"id":"ST-1","unknown":{"a":1}})).is_err());
    }
    #[test]
    fn links_numeric_remote_and_appends_exact_log_without_mutation() {
        let s = story();
        let linked = link(&s, "0042", "https://example/42", "2026-11-05", "create").unwrap();
        assert_eq!(linked["remote"], 42);
        assert_eq!(linked["updated"], "2026-11-05");
        assert_eq!(
            linked["sections"]["Changelog"],
            "- 2026-11-02 created\n- 2026-11-05 pushed to tracker as 0042"
        );
        assert!(s.get("remote").is_none());
        let next = link(&linked, "ABC-1", "url", "2026-11-06", "update").unwrap();
        assert_eq!(next["remote"], "ABC-1");
        assert!(next["sections"]["Changelog"]
            .as_str()
            .unwrap()
            .ends_with("- 2026-11-06 updated tracker item ABC-1"));
        assert_eq!(reread(&serialize(&linked).unwrap()), linked);
    }
    #[test]
    fn tracker_titles_include_one_story_id_without_mutating_local_titles() {
        for id in ["ST-067", "CRT-1024"] {
            for title in ["Build a bridge".to_owned(), format!("{id}: Build a bridge")] {
                let mut s = story();
                s["id"] = json!(id);
                s["title"] = json!(title);
                let original = s.clone();
                let expected = format!("{id}: Build a bridge");
                for tracker in ["github", "gitea", "ado"] {
                    let card = tracker_card(&s, tracker, &json!({})).unwrap();
                    assert_eq!(card["title"], expected, "{tracker}");
                    if tracker == "ado" {
                        assert_eq!(field(&card, "Title"), expected);
                    }
                }
                assert_eq!(s, original);
                for format in ["github", "gitea", "ado", "markdown"] {
                    let card = render_card(&s, format, &json!({})).unwrap();
                    assert_eq!(card["title"], title, "{format}");
                }
                let card = render_card(&s, "ado", &json!({})).unwrap();
                assert_eq!(ado_args(&card).unwrap().title, title);
            }
        }
    }
    #[test]
    fn github_gitea_and_markdown_cards_match_contract() {
        let s = story();
        let gh = render_card(&s, "github", &json!({})).unwrap();
        assert_eq!(
            gh["labels"],
            json!(["story", "size:3", "value:8", "epic:EP-1"])
        );
        let body = gh["body"].as_str().unwrap();
        assert!(body.starts_with("As a user"));
        assert!(body.contains("### Acceptance Criteria\n- [ ] Given email"));
        assert!(body.contains("### Notes\nKeep this."));
        assert!(!body.contains("Changelog"));
        assert!(body.ends_with("<sub>story-time ST-001 · WSJF 5.33</sub>"));
        let gt = render_card(&s, "gitea", &json!({})).unwrap();
        assert_eq!(gt["body"], gh["body"]);
        assert_eq!(gt["text"], gh["text"]);
        assert_eq!(gt["format"], "gitea");
        let md = render_card(&s, "markdown", &json!({})).unwrap();
        assert!(md["text"]
            .as_str()
            .unwrap()
            .starts_with("## Reset password\n\nAs a user"));
        assert!(md["body"]
            .as_str()
            .unwrap()
            .contains("**Size:** 3 · **Value:** 8"));
    }
    #[test]
    fn ado_process_fields_and_html() {
        let s = story();
        for (process, kind, points) in [
            ("agile", "User Story", "Story Points"),
            ("scrum", "Product Backlog Item", "Effort"),
            ("basic", "Issue", "Effort"),
        ] {
            let cfg = json!({"ado":{"process":process,"areaPath":"Shop\\Web","iterationPath":"Sprint 12"}});
            let card = render_card(&s, "ado", &cfg).unwrap();
            assert_eq!(field(&card, "Work Item Type"), kind);
            assert_eq!(field(&card, points), "3");
            assert_eq!(field(&card, "Tags"), "story-time; ST-001; epic:EP-1");
            assert_eq!(field(&card, "Area Path"), "Shop\\Web");
            if process == "basic" {
                assert!(field(&card, "Acceptance Criteria").is_null());
                assert!(field(&card, "Description")
                    .as_str()
                    .unwrap()
                    .contains("Acceptance Criteria:\n- Given email"));
            } else {
                assert_eq!(
                    field(&card, "Acceptance Criteria"),
                    "- Given email <exists>\n- Given token & valid"
                );
                let args = ado_args(&card).unwrap();
                assert!(args.fields.contains(&"Microsoft.VSTS.Common.AcceptanceCriteria=<ul><li>Given email &lt;exists&gt;</li><li>Given token &amp; valid</li></ul>".to_owned()));
                assert!(args.description.unwrap().starts_with("<p>As a user"));
                assert_eq!(args.area.as_deref(), Some("Shop\\Web"));
            }
        }
        assert_eq!(
            to_html("one < two\nline\n\n& end"),
            "<p>one &lt; two<br>line</p><p>&amp; end</p>"
        );
    }
    #[test]
    fn spike_and_bug_cards() {
        let mut s = story();
        s["type"] = json!("spike");
        s["timebox"] = json!("1d");
        s["sections"] =
            json!({"Question":"Can X do 3DS?","Exit Criteria":"- [x] Decision recorded"});
        let card = render_card(&s, "github", &json!({})).unwrap();
        assert!(card["body"]
            .as_str()
            .unwrap()
            .contains("### Question\nCan X do 3DS?\n\nTimebox: 1d\n\n### Exit Criteria"));
        assert_eq!(acceptance_criteria(&s)[0]["done"], true);
        s["type"] = json!("bug");
        s["sections"]["Steps to Reproduce"] = json!("1. Open cart\n2. Refresh");
        let card = render_card(&s, "ado", &json!({})).unwrap();
        assert_eq!(field(&card, "Work Item Type"), "Bug");
        assert!(field(&card, "Repro Steps")
            .as_str()
            .unwrap()
            .contains("1. Open cart"));
        assert!(field(&card, "Description").is_null());
    }
}
