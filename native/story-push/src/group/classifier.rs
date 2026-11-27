//! The classification port and its adapters. `JevClassifier` asks Jev; `KeywordClassifier`
//! is the code fallback, answering in the same shape so one interpreter reads both.

use super::taxonomy::NONE_FIT;
use crate::judge::{words, Client};
use serde_json::{json, Map, Value};

/// Who answered.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Source {
    Jev { model: String },
    Code { why: String },
}

pub(crate) struct Classified {
    pub(crate) answers: Map<String, Value>,
    pub(crate) source: Source,
}

/// Port: answer the taxonomy questions for one story state.
pub(crate) trait Classifier {
    fn classify(
        &mut self,
        state: &Value,
        questions: &Map<String, Value>,
    ) -> Result<Classified, String>;
}

pub(crate) struct JevClassifier {
    pub(crate) client: Client,
}

impl Classifier for JevClassifier {
    fn classify(
        &mut self,
        state: &Value,
        questions: &Map<String, Value>,
    ) -> Result<Classified, String> {
        let answer = self
            .client
            .decide(state, &Value::Object(questions.clone()))
            .map_err(str::to_owned)?;
        Ok(Classified {
            answers: answer.answers,
            source: Source::Jev {
                model: answer.model,
            },
        })
    }
}

/// Code fallback: word overlap between the story and each option's text. Its confidence is
/// capped at `KEYWORD_CEILING`, below the default threshold, so a keyword guess is a
/// suggestion unless the caller lowers `--min-confidence` on purpose.
pub(crate) struct KeywordClassifier {
    pub(crate) why: String,
}

pub(crate) const KEYWORD_CEILING: f64 = 0.6;

/// One `choice` answer: the option sharing the most distinct words with the story. A tie
/// keeps the first option; no shared word answers `none_fit`.
fn keyword_answer(story: &[String], criteria: &Map<String, Value>) -> Value {
    let scores: Vec<(&String, usize)> = criteria
        .iter()
        .filter(|(key, _)| key.as_str() != NONE_FIT)
        .map(|(key, text)| {
            let mut option = words(text.as_str().unwrap_or(""));
            option.sort();
            option.dedup();
            (key, option.iter().filter(|w| story.contains(w)).count())
        })
        .collect();
    let total: usize = scores.iter().map(|(_, n)| n).sum();
    let best = scores
        .iter()
        .fold(None::<(&String, usize)>, |best, &(key, n)| match best {
            Some((_, m)) if m >= n => best,
            _ => Some((key, n)),
        });
    let mut probabilities = Map::new();
    for (key, n) in &scores {
        let p = if total == 0 {
            0.0
        } else {
            *n as f64 / total as f64
        };
        probabilities.insert((*key).clone(), json!(p * KEYWORD_CEILING));
    }
    match best {
        Some((key, n)) if n > 0 => {
            let confidence = KEYWORD_CEILING * n as f64 / total as f64;
            probabilities.insert(NONE_FIT.into(), json!(KEYWORD_CEILING - confidence));
            json!({"type": "choice", "choice": key, "confidence": confidence, "probabilities": probabilities})
        }
        _ => {
            probabilities.insert(NONE_FIT.into(), json!(KEYWORD_CEILING));
            json!({"type": "choice", "choice": NONE_FIT, "confidence": KEYWORD_CEILING, "probabilities": probabilities})
        }
    }
}

impl Classifier for KeywordClassifier {
    fn classify(
        &mut self,
        state: &Value,
        questions: &Map<String, Value>,
    ) -> Result<Classified, String> {
        let text = [&state["title"], &state["statement"], &state["criteria"]]
            .iter()
            .map(|v| match v {
                Value::Array(items) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
                v => v.as_str().unwrap_or("").to_owned(),
            })
            .collect::<Vec<_>>()
            .join(" ");
        let story = words(&text);
        let answers = questions
            .iter()
            .map(|(id, q)| {
                let criteria = q["criteria"].as_object().cloned().unwrap_or_default();
                (id.clone(), keyword_answer(&story, &criteria))
            })
            .collect();
        Ok(Classified {
            answers,
            source: Source::Code {
                why: self.why.clone(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::questions::{interpret, questions};
    use crate::group::taxonomy::Taxonomy;

    fn taxonomy() -> Taxonomy {
        Taxonomy::parse(r#"{"epics":[
          {"name":"Platform & infrastructure","description":"Hosting servers EC2 database roles credentials","features":[
            {"name":"Core runtime hosting","description":"EC2 host OS roles"},
            {"name":"Database roles & credentials","description":"Postgres grants"}]},
          {"name":"Core cutover","description":"Cut varsity-engine callers over to the Core API","features":[
            {"name":"Engine call cutovers","description":"Cut one engine module over to Core API calls"}]}]}"#).unwrap()
    }

    #[test]
    fn keywords_pick_the_overlapping_option_below_the_ceiling() {
        let t = taxonomy();
        let mut k = KeywordClassifier {
            why: "no key".into(),
        };
        let state = json!({"title":"Cut varsity-engine results calls over to the Core API","statement":"","criteria":[]});
        let out = k.classify(&state, &questions(&t)).unwrap();
        let choice = interpret(&t, &out.answers).unwrap();
        assert_eq!(choice.epic.name.as_deref(), Some("Core cutover"));
        assert_eq!(
            choice.feature.unwrap().name.as_deref(),
            Some("Engine call cutovers")
        );
        assert!(choice.epic.confidence <= KEYWORD_CEILING && choice.epic.confidence > 0.0);
        assert_eq!(
            out.source,
            Source::Code {
                why: "no key".into()
            }
        );
    }

    #[test]
    fn no_overlap_answers_none_fit() {
        let t = taxonomy();
        let mut k = KeywordClassifier {
            why: "no key".into(),
        };
        let state = json!({"title":"Bake a sourdough loaf","statement":"","criteria":[]});
        let out = k.classify(&state, &questions(&t)).unwrap();
        let choice = interpret(&t, &out.answers).unwrap();
        assert_eq!(choice.epic.name, None);
    }
}
