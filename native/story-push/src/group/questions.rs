//! The batched Jev `choice` questions for one story, and the reading of their answers. Pure.
//!
//! One request per story: `epic` picks among the taxonomy epics, and `feature_<n>` picks among
//! the features of epic n. Only the chosen epic's feature question is read. Every question
//! carries `none_fit`, so Jev is never forced into a wrong bucket.

use super::taxonomy::{Taxonomy, NONE_FIT};
use serde_json::{json, Map, Value};

/// A choice read back from an answer: the taxonomy name, or `None` for `none_fit`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pick {
    pub(crate) name: Option<String>,
    pub(crate) confidence: f64,
}

/// The epic and feature Jev (or the fallback) chose for one story.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Choice {
    pub(crate) epic: Pick,
    pub(crate) feature: Option<Pick>,
}

const NONE_FIT_TEXT: &str = "None of the other options describes this work";

/// A Jev option key for a name: lowercase words joined by `_`, at most 40 characters.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let mut out: String = out.trim_end_matches('_').chars().take(40).collect();
    if out.is_empty() || out == NONE_FIT {
        out = "option".into();
    }
    out
}

/// Option keys for names, in order, made unique with a `_<n>` suffix.
fn keys<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for name in names {
        let base = slug(name);
        let mut key = base.clone();
        let mut n = 2;
        while seen.contains(&key) {
            key = format!("{base}_{n}");
            n += 1;
        }
        seen.push(key);
    }
    seen
}

fn choice(instructions: String, options: Vec<(String, String)>) -> Value {
    let mut criteria = Map::new();
    for (key, text) in options {
        criteria.insert(key, Value::String(text));
    }
    criteria.insert(NONE_FIT.into(), Value::String(NONE_FIT_TEXT.into()));
    json!({"type": "choice", "instructions": instructions, "criteria": criteria})
}

/// The questions for one story: `epic`, then `feature_<n>` for epic n (1-based).
pub(crate) fn questions(t: &Taxonomy) -> Map<String, Value> {
    let mut q = Map::new();
    let epic_keys = keys(t.epics.iter().map(|e| e.name.as_str()));
    let options = t
        .epics
        .iter()
        .zip(epic_keys)
        .map(|(e, k)| (k, format!("{}: {}", e.name, e.description)))
        .collect();
    q.insert(
        "epic".into(),
        choice(
            "Which epic does the work described by state.title, state.statement and state.criteria belong to?".into(),
            options,
        ),
    );
    for (i, epic) in t.epics.iter().enumerate() {
        let feature_keys = keys(epic.features.iter().map(|f| f.name.as_str()));
        let options = epic
            .features
            .iter()
            .zip(feature_keys)
            .map(|(f, k)| (k, format!("{}: {}", f.name, f.description)))
            .collect();
        q.insert(
            format!("feature_{}", i + 1),
            choice(
                format!(
                    "If this work belongs to the epic \"{}\", which of its features does it build?",
                    epic.name
                ),
                options,
            ),
        );
    }
    q
}

/// The taxonomy name an answer chose, `Some(None)` for `none_fit`, `None` when untrustworthy.
fn pick<'a>(answer: Option<&Value>, names: impl Iterator<Item = &'a str> + Clone) -> Option<Pick> {
    let answer = answer?;
    let chosen = answer["choice"].as_str()?;
    let confidence = answer["confidence"]
        .as_f64()
        .filter(|c| (0.0..=1.0).contains(c))?;
    if chosen == NONE_FIT {
        return Some(Pick {
            name: None,
            confidence,
        });
    }
    let index = keys(names.clone()).iter().position(|k| k == chosen)?;
    Some(Pick {
        name: names.clone().nth(index).map(str::to_owned),
        confidence,
    })
}

/// Reads the epic answer, then only the chosen epic's feature answer. Any answer naming an
/// option the questions did not offer, or a missing answer, makes the whole reading untrusted.
pub(crate) fn interpret(t: &Taxonomy, answers: &Map<String, Value>) -> Option<Choice> {
    let epic = pick(answers.get("epic"), t.epics.iter().map(|e| e.name.as_str()))?;
    let Some(name) = &epic.name else {
        return Some(Choice {
            epic,
            feature: None,
        });
    };
    let index = t.epics.iter().position(|e| &e.name == name)?;
    let features = t.epics[index].features.iter().map(|f| f.name.as_str());
    let feature = pick(answers.get(&format!("feature_{}", index + 1)), features)?;
    Some(Choice {
        epic,
        feature: Some(feature),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taxonomy() -> Taxonomy {
        Taxonomy::parse(r#"{"epics":[
          {"name":"Platform & infrastructure","description":"Hosting, database roles, credentials","features":[
            {"name":"Core runtime hosting","description":"EC2 roles"},
            {"name":"Database roles & credentials","description":"Grants"}]},
          {"name":"Core cutover","description":"Move engine callers onto Core","features":[
            {"name":"Engine call cutovers","description":"Route one engine module through Core"}]}]}"#).unwrap()
    }

    fn answer(choice: &str, confidence: f64) -> Value {
        json!({"type":"choice","choice":choice,"confidence":confidence,"probabilities":{choice: confidence}})
    }

    #[test]
    fn one_epic_question_and_one_feature_question_per_epic_each_with_none_fit() {
        let q = questions(&taxonomy());
        assert_eq!(
            q.keys().collect::<Vec<_>>(),
            ["epic", "feature_1", "feature_2"]
        );
        let epic = &q["epic"];
        assert_eq!(epic["type"], "choice");
        let options: Vec<&String> = epic["criteria"].as_object().unwrap().keys().collect();
        assert_eq!(
            options,
            ["platform_infrastructure", "core_cutover", NONE_FIT]
        );
        assert_eq!(
            epic["criteria"]["core_cutover"],
            "Core cutover: Move engine callers onto Core"
        );
        let f1: Vec<&String> = q["feature_1"]["criteria"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(
            f1,
            [
                "core_runtime_hosting",
                "database_roles_credentials",
                NONE_FIT
            ]
        );
        assert!(q["feature_2"]["instructions"]
            .as_str()
            .unwrap()
            .contains("Core cutover"));
    }

    #[test]
    fn fifteen_epics_make_sixteen_questions_the_jev_batch_limit() {
        let epics: Vec<String> = (0..15)
            .map(|i| format!(r#"{{"name":"E{i}","description":"d","features":[{{"name":"F{i}","description":"d"}}]}}"#))
            .collect();
        let t = Taxonomy::parse(&format!(r#"{{"epics":[{}]}}"#, epics.join(","))).unwrap();
        assert_eq!(questions(&t).len(), 16);
    }

    #[test]
    fn option_keys_stay_unique_when_names_slug_alike() {
        let t = Taxonomy::parse(
            r#"{"epics":[
          {"name":"A & B","description":"d","features":[{"name":"F1","description":"d"}]},
          {"name":"A B","description":"d","features":[{"name":"F2","description":"d"}]}]}"#,
        )
        .unwrap();
        let q = questions(&t);
        let keys: Vec<&String> = q["epic"]["criteria"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["a_b", "a_b_2", NONE_FIT]);
    }

    #[test]
    fn the_feature_is_read_only_from_the_chosen_epics_question() {
        let mut a = Map::new();
        a.insert("epic".into(), answer("core_cutover", 0.97));
        a.insert("feature_1".into(), answer("core_runtime_hosting", 0.99));
        a.insert("feature_2".into(), answer("engine_call_cutovers", 0.88));
        let c = interpret(&taxonomy(), &a).unwrap();
        assert_eq!(
            c.epic,
            Pick {
                name: Some("Core cutover".into()),
                confidence: 0.97
            }
        );
        assert_eq!(
            c.feature,
            Some(Pick {
                name: Some("Engine call cutovers".into()),
                confidence: 0.88
            })
        );
    }

    #[test]
    fn none_fit_reads_as_no_name_and_skips_the_feature() {
        let mut a = Map::new();
        a.insert("epic".into(), answer(NONE_FIT, 0.9));
        a.insert("feature_1".into(), answer("core_runtime_hosting", 0.99));
        a.insert("feature_2".into(), answer("engine_call_cutovers", 0.88));
        let c = interpret(&taxonomy(), &a).unwrap();
        assert_eq!(c.epic.name, None);
        assert_eq!(c.feature, None);
    }

    #[test]
    fn an_answer_naming_an_unknown_option_is_not_trusted() {
        let mut a = Map::new();
        a.insert("epic".into(), answer("behind_the_scenes", 0.9));
        assert_eq!(interpret(&taxonomy(), &a), None);
        let mut b = Map::new();
        b.insert("epic".into(), answer("core_cutover", 0.9));
        assert_eq!(
            interpret(&taxonomy(), &b),
            None,
            "the chosen epic's feature answer is missing"
        );
    }
}
