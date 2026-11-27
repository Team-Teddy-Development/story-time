//! The naming port: invents new feature (and, when nothing fits, epic) names for stories the
//! taxonomy leaves uncovered. Adapters: `ClaudeNamer` (Messages API, JSON-schema output) and
//! `FileNamer` (proposals written by a person or another agent, such as an MCP caller).
//! A namer only proposes; `propose::verify` decides with Jev, so no model grades its own names.

use super::taxonomy::Taxonomy;
use serde_json::{json, Value};

pub(crate) const ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
pub(crate) const ANTHROPIC_VERSION: &str = "2026-10-21";
pub(crate) const DEFAULT_MODEL: &str = "claude-opus-5-5";

/// One uncovered story as a namer sees it: no id, owner or link leaves the machine.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Gap {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) statement: String,
    pub(crate) criteria: Vec<String>,
}

/// A proposed feature, under an existing epic or a new one (`epic_description` set).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Proposal {
    pub(crate) epic: String,
    pub(crate) epic_description: Option<String>,
    pub(crate) feature: String,
    pub(crate) feature_description: String,
    /// Story ids the proposal says it covers.
    pub(crate) stories: Vec<String>,
}

pub(crate) trait Namer {
    /// Who proposed, for the report: a model id or `file`.
    fn label(&self) -> String;
    fn propose(&mut self, t: &Taxonomy, gaps: &[Gap], max: usize) -> Result<Vec<Proposal>, String>;
}

fn text(v: &Value, key: &str) -> Option<String> {
    v[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Reads `{"proposals":[...]}`; `stories` are mapped through `id_of`.
fn parse(
    value: &Value,
    id_of: impl Fn(&str) -> Result<String, String>,
) -> Result<Vec<Proposal>, String> {
    let items = value["proposals"]
        .as_array()
        .ok_or("proposals must be a list")?;
    items
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let need = || {
                format!(
                    "proposal {} needs epic, feature, feature_description and stories",
                    i + 1
                )
            };
            let stories = p["stories"].as_array().ok_or_else(need)?;
            Ok(Proposal {
                epic: text(p, "epic").ok_or_else(need)?,
                epic_description: text(p, "epic_description"),
                feature: text(p, "feature").ok_or_else(need)?,
                feature_description: text(p, "feature_description").ok_or_else(need)?,
                stories: stories
                    .iter()
                    .map(|s| s.as_str().ok_or_else(need).and_then(&id_of))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect()
}

/// `{"proposals":[...]}` whose `stories` are story ids, as a file or an MCP caller gives them.
pub(crate) fn parse_proposals(value: &Value) -> Result<Vec<Proposal>, String> {
    parse(value, |id| Ok(id.to_owned()))
}

/// Proposals from a JSON file: `{"proposals":[{"epic","epic_description"?,"feature",
/// "feature_description","stories":[ids]}]}`.
pub(crate) struct FileNamer {
    pub(crate) text: String,
}

impl Namer for FileNamer {
    fn label(&self) -> String {
        "file".into()
    }
    fn propose(
        &mut self,
        _t: &Taxonomy,
        _gaps: &[Gap],
        _max: usize,
    ) -> Result<Vec<Proposal>, String> {
        let value: Value = serde_json::from_str(&self.text)
            .map_err(|e| format!("proposals: invalid JSON ({e})"))?;
        parse_proposals(&value)
    }
}

pub(crate) struct ClaudeNamer {
    pub(crate) endpoint: String,
    pub(crate) key: String,
    pub(crate) model: String,
    pub(crate) http: reqwest::blocking::Client,
}

const SYSTEM: &str = "You maintain a product backlog taxonomy: a short list of named epics, each with named features. \
Some stories fit no existing feature. Propose new features for them, grouped so each new feature covers at least two of the given stories. \
Prefer a new feature under an existing epic; propose a new epic only when no existing epic fits. \
Names are short plain words a product owner would use (for example \"Recruit portal\" or \"Guardian claim review\"), at most 50 characters, never ids or codes, and never an existing name. \
Give each name a one-sentence description of the work it covers. For an existing epic, leave epic_description empty. \
Leave a story out when no sensible group fits it.";

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["proposals"],
        "properties": {"proposals": {"type": "array", "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["epic", "epic_description", "feature", "feature_description", "stories"],
            "properties": {
                "epic": {"type": "string"},
                "epic_description": {"type": "string"},
                "feature": {"type": "string"},
                "feature_description": {"type": "string"},
                "stories": {"type": "array", "items": {"type": "string"}}
            }
        }}}
    })
}

impl ClaudeNamer {
    fn request(&self, t: &Taxonomy, gaps: &[Gap], max: usize) -> Value {
        let taxonomy: Vec<Value> = t
            .epics
            .iter()
            .map(|e| {
                json!({"epic": e.name, "description": e.description,
                "features": e.features.iter().map(|f| f.name.as_str()).collect::<Vec<_>>()})
            })
            .collect();
        let stories: Vec<Value> = gaps
            .iter()
            .enumerate()
            .map(|(i, g)| json!({"key": format!("s{}", i + 1), "title": g.title, "statement": g.statement, "criteria": g.criteria}))
            .collect();
        let task = format!(
            "Propose at most {max} new features for these uncovered stories. Refer to stories by key.\n\n{}",
            json!({"taxonomy": taxonomy, "stories": stories})
        );
        json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": SYSTEM,
            "messages": [{"role": "user", "content": task}],
            "output_config": {"effort": "medium", "format": {"type": "json_schema", "schema": schema()}},
            "fallbacks": "default",
        })
    }

    fn send(&self, body: &Value) -> Result<Value, String> {
        for attempt in 1..=3u32 {
            let response = self
                .http
                .post(&self.endpoint)
                .header("x-api-key", &self.key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("anthropic-beta", "server-side-fallback-2026-07-01")
                .header("content-type", "application/json")
                .body(body.to_string())
                .send();
            let retry = match response {
                Ok(r) if r.status().is_success() => {
                    return r
                        .json::<Value>()
                        .map_err(|_| "Anthropic returned invalid JSON.".to_string())
                }
                Ok(r) if [401, 403].contains(&r.status().as_u16()) => {
                    return Err(format!(
                    "Anthropic rejected the API key (HTTP {}). Check it with: story secret status",
                    r.status().as_u16()
                ))
                }
                Ok(r) if r.status().as_u16() == 429 || r.status().is_server_error() => {
                    format!("Anthropic is unavailable (HTTP {}).", r.status().as_u16())
                }
                Ok(r) => {
                    return Err(format!(
                        "Anthropic refused the request (HTTP {}).",
                        r.status().as_u16()
                    ))
                }
                Err(e) if e.is_timeout() => "The Anthropic request timed out.".into(),
                Err(_) => "Could not reach Anthropic.".into(),
            };
            if attempt == 3 {
                return Err(retry);
            }
            std::thread::sleep(std::time::Duration::from_millis(
                500 * 2u64.pow(attempt - 1),
            ));
        }
        unreachable!()
    }
}

impl Namer for ClaudeNamer {
    fn label(&self) -> String {
        self.model.clone()
    }
    fn propose(&mut self, t: &Taxonomy, gaps: &[Gap], max: usize) -> Result<Vec<Proposal>, String> {
        let body = self.request(t, gaps, max);
        if crate::judge::egress_findings(&body["messages"]) {
            return Err("A story looks like it holds personal data or a secret, so nothing was sent to Claude.".into());
        }
        let reply = self.send(&body)?;
        match reply["stop_reason"].as_str() {
            Some("refusal") => {
                let category = reply["stop_details"]["category"]
                    .as_str()
                    .unwrap_or("unspecified");
                return Err(format!(
                    "Claude declined to propose names (refusal: {category})."
                ));
            }
            Some("max_tokens") => return Err("Claude's proposal was cut off (max_tokens).".into()),
            _ => {}
        }
        let text = reply["content"]
            .as_array()
            .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
            .and_then(|b| b["text"].as_str())
            .ok_or("Claude returned no proposal text.")?;
        let value: Value = serde_json::from_str(text)
            .map_err(|_| "Claude's proposal was not valid JSON.".to_string())?;
        parse(&value, |key| {
            key.strip_prefix('s')
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| gaps.get(n.wrapping_sub(1)))
                .map(|g| g.id.clone())
                .ok_or_else(|| format!("Claude named a story it was not given: {key}."))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::tests::{Fake, Reply};

    fn taxonomy() -> Taxonomy {
        Taxonomy::parse(r#"{"epics":[{"name":"Athletes & privacy","description":"Athlete profiles and consent","features":[
          {"name":"Athlete profiles & guardian consent","description":"Profiles and consent"}]}]}"#).unwrap()
    }
    fn gaps() -> Vec<Gap> {
        vec![
            Gap {
                id: "VC-301".into(),
                title: "Review guardian claims before approval".into(),
                statement: "As a program admin, I want to review guardian claims.".into(),
                criteria: vec![
                    "Given a claim, when reviewed, then it is approved or denied".into(),
                ],
            },
            Gap {
                id: "VC-302".into(),
                title: "Escalate disputed guardian claims".into(),
                statement: "".into(),
                criteria: vec![],
            },
        ]
    }
    fn client(fake: &Fake) -> ClaudeNamer {
        ClaudeNamer {
            endpoint: fake.url.clone(),
            key: "sk-ant-test-key".into(),
            model: DEFAULT_MODEL.into(),
            http: reqwest::blocking::Client::new(),
        }
    }
    fn message(text: &str, stop: &str) -> Reply {
        (200, json!({"id":"msg_1","type":"message","role":"assistant","model":DEFAULT_MODEL,
            "content":[{"type":"text","text":text}],"stop_reason":stop,"usage":{"input_tokens":900,"output_tokens":120}}).to_string(), vec![])
    }
    const ANSWER: &str = r#"{"proposals":[{"epic":"Athletes & privacy","epic_description":"","feature":"Guardian claim review","feature_description":"Review, approve and escalate guardian claims","stories":["s1","s2"]}]}"#;

    #[test]
    fn one_structured_request_carries_no_ids_and_maps_story_keys_back() {
        let fake = Fake::new(|_: &Value, _| message(ANSWER, "end_turn"));
        let proposals = client(&fake).propose(&taxonomy(), &gaps(), 3).unwrap();
        assert_eq!(
            proposals,
            [Proposal {
                epic: "Athletes & privacy".into(),
                epic_description: None,
                feature: "Guardian claim review".into(),
                feature_description: "Review, approve and escalate guardian claims".into(),
                stories: vec!["VC-301".into(), "VC-302".into()],
            }]
        );
        let requests = fake.requests();
        assert_eq!(requests.len(), 1);
        let head = requests[0].head.to_lowercase();
        assert!(head.contains("x-api-key: sk-ant-test-key"), "{head}");
        assert!(head.contains("anthropic-version: 2026-10-21"), "{head}");
        assert!(
            head.contains("anthropic-beta: server-side-fallback-2026-07-01"),
            "{head}"
        );
        let body = requests[0].json();
        assert_eq!(body["model"], DEFAULT_MODEL);
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(
            body["output_config"]["format"]["schema"]["additionalProperties"],
            false
        );
        assert!(
            body.get("thinking").is_none(),
            "Opus 5.5 thinking is always on; never sent"
        );
        assert!(
            !requests[0].body.contains("VC-30"),
            "story ids never leave the machine: {}",
            requests[0].body
        );
        assert!(requests[0]
            .body
            .contains("Review guardian claims before approval"));
        assert!(
            requests[0].body.contains("at most 3"),
            "the cap is in the prompt"
        );
    }

    #[test]
    fn refusals_truncation_and_unknown_story_keys_are_errors() {
        let fake = Fake::new(|_: &Value, _| {
            (200, json!({"content":[],"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber","explanation":"x"}}).to_string(), vec![])
        });
        assert_eq!(
            client(&fake).propose(&taxonomy(), &gaps(), 3).unwrap_err(),
            "Claude declined to propose names (refusal: cyber)."
        );
        let fake = Fake::new(|_: &Value, _| message("{\"proposals\":[", "max_tokens"));
        assert_eq!(
            client(&fake).propose(&taxonomy(), &gaps(), 3).unwrap_err(),
            "Claude's proposal was cut off (max_tokens)."
        );
        let fake = Fake::new(|_: &Value, _| {
            message(
                r#"{"proposals":[{"epic":"A","epic_description":"","feature":"F","feature_description":"d","stories":["s9"]}]}"#,
                "end_turn",
            )
        });
        assert_eq!(
            client(&fake).propose(&taxonomy(), &gaps(), 3).unwrap_err(),
            "Claude named a story it was not given: s9."
        );
        let fake = Fake::new(|_: &Value, _| {
            (401, json!({"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}).to_string(), vec![])
        });
        assert_eq!(
            client(&fake).propose(&taxonomy(), &gaps(), 3).unwrap_err(),
            "Anthropic rejected the API key (HTTP 401). Check it with: story secret status"
        );
    }

    #[test]
    fn a_story_with_personal_data_never_leaves_the_machine() {
        let fake = Fake::new(|_: &Value, _| message(ANSWER, "end_turn"));
        let mut g = gaps();
        g[1].statement = "Call coach@example.com about it".into();
        assert_eq!(
            client(&fake).propose(&taxonomy(), &g, 3).unwrap_err(),
            "A story looks like it holds personal data or a secret, so nothing was sent to Claude."
        );
        assert!(fake.requests().is_empty());
    }

    #[test]
    fn file_proposals_parse_and_a_new_epic_keeps_its_description() {
        let mut f = FileNamer { text: r#"{"proposals":[{"epic":"Coach tools","epic_description":"Tools coaches use daily","feature":"Practice plans","feature_description":"Plan practices","stories":["VC-310","VC-311"]}]}"#.into() };
        let p = f.propose(&taxonomy(), &[], 3).unwrap();
        assert_eq!(
            p[0].epic_description.as_deref(),
            Some("Tools coaches use daily")
        );
        assert_eq!(p[0].stories, ["VC-310", "VC-311"]);
        let mut bad = FileNamer {
            text: r#"{"proposals":[{"epic":"A"}]}"#.into(),
        };
        assert_eq!(
            bad.propose(&taxonomy(), &[], 3).unwrap_err(),
            "proposal 1 needs epic, feature, feature_description and stories"
        );
    }
}
