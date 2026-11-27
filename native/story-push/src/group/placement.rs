//! Application service: decides where one story goes. It asks the primary classifier, falls
//! back to the code classifier when that fails or answers untrustworthily, and turns the
//! choice into an outcome against the story's current epic and feature.

use super::classifier::{Classifier, Source};
use super::questions::{interpret, questions, Pick};
use super::taxonomy::Taxonomy;
use crate::judge::{statement, title};
use crate::lenient_story::{trim, Js};
use crate::lint::acceptance_criteria;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    /// Confident placement that differs from the story's current epic or feature.
    Assigned,
    /// Confident placement equal to what the story already has.
    Unchanged,
    /// Below the threshold or `none_fit`; a human decides.
    Unresolved(String),
    /// The story already names a taxonomy epic and feature; not re-asked without `--reassign`.
    Kept,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Placement {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) current_epic: Option<String>,
    pub(crate) current_feature: Option<String>,
    pub(crate) epic: Option<Pick>,
    pub(crate) feature: Option<Pick>,
    pub(crate) source: Option<Source>,
    pub(crate) outcome: Outcome,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    pub(crate) min_confidence: f64,
    pub(crate) reassign: bool,
}

/// What Jev sees of a story: its title, statement and criteria. No ids, owners or links.
pub(crate) fn state(story: &Js) -> Value {
    json!({
        "title": title(story),
        "statement": trim(&statement(story)),
        "criteria": acceptance_criteria(story),
    })
}

fn text(story: &Js, key: &str) -> Option<String> {
    match story.get(key) {
        Js::Str(s) if !trim(s).is_empty() => Some(trim(s).to_owned()),
        _ => None,
    }
}

fn folded(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Places one story. A story already naming a taxonomy epic and one of its features is kept
/// without a classifier call unless `reassign`.
pub(crate) fn place(
    story: &Js,
    t: &Taxonomy,
    primary: &mut dyn Classifier,
    fallback: &mut dyn Classifier,
    opts: Options,
) -> Placement {
    let current_epic = text(story, "epic");
    let current_feature = text(story, "feature");
    let mut placement = Placement {
        id: story.get("id").string(),
        title: title(story),
        current_epic: current_epic.clone(),
        current_feature: current_feature.clone(),
        epic: None,
        feature: None,
        source: None,
        outcome: Outcome::Kept,
    };
    let in_taxonomy = current_epic
        .as_deref()
        .and_then(|e| t.epic(e))
        .is_some_and(|e| {
            current_feature
                .as_deref()
                .is_some_and(|f| e.features.iter().any(|x| folded(&x.name, f)))
        });
    if in_taxonomy && !opts.reassign {
        return placement;
    }
    let state = state(story);
    let questions = questions(t);
    let answered = primary
        .classify(&state, &questions)
        .ok()
        .and_then(|c| interpret(t, &c.answers).map(|choice| (choice, c.source)));
    let (choice, source) = match answered {
        Some(found) => found,
        None => {
            let reason = "Jev gave no usable answer";
            match fallback.classify(&state, &questions) {
                Ok(c) => match interpret(t, &c.answers) {
                    Some(choice) => (
                        choice,
                        match c.source {
                            Source::Code { why } => Source::Code {
                                why: format!("{why}; {reason}"),
                            },
                            other => other,
                        },
                    ),
                    None => {
                        placement.outcome = Outcome::Unresolved("no classifier answered".into());
                        return placement;
                    }
                },
                Err(e) => {
                    placement.outcome = Outcome::Unresolved(e);
                    return placement;
                }
            }
        }
    };
    placement.source = Some(source);
    placement.epic = Some(choice.epic.clone());
    placement.feature = choice.feature.clone();
    placement.outcome = decide(
        &choice.epic,
        choice.feature.as_ref(),
        &current_epic,
        &current_feature,
        opts,
    );
    placement
}

fn decide(
    epic: &Pick,
    feature: Option<&Pick>,
    current_epic: &Option<String>,
    current_feature: &Option<String>,
    opts: Options,
) -> Outcome {
    let Some(epic_name) = &epic.name else {
        return Outcome::Unresolved("no taxonomy epic fits".into());
    };
    if epic.confidence < opts.min_confidence {
        return Outcome::Unresolved(format!(
            "epic confidence {} is below {}",
            round2(epic.confidence),
            opts.min_confidence
        ));
    }
    let Some(feature) = feature else {
        return Outcome::Unresolved(format!("no feature of \"{epic_name}\" fits"));
    };
    let Some(feature_name) = &feature.name else {
        return Outcome::Unresolved(format!("no feature of \"{epic_name}\" fits"));
    };
    if feature.confidence < opts.min_confidence {
        return Outcome::Unresolved(format!(
            "feature confidence {} is below {}",
            round2(feature.confidence),
            opts.min_confidence
        ));
    }
    let same = |current: &Option<String>, name: &str| current.as_deref().is_some_and(|c| c == name);
    if same(current_epic, epic_name) && same(current_feature, feature_name) {
        Outcome::Unchanged
    } else {
        Outcome::Assigned
    }
}

pub(crate) fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::classifier::Classified;
    use serde_json::Map;

    fn taxonomy() -> Taxonomy {
        Taxonomy::parse(
            r#"{"epics":[
          {"name":"Platform & infrastructure","description":"Hosting","features":[
            {"name":"Core runtime hosting","description":"EC2 roles"}]},
          {"name":"Core cutover","description":"Engine callers onto Core","features":[
            {"name":"Engine call cutovers","description":"One module"},
            {"name":"Bypass retirement","description":"Remove bypass paths"}]}]}"#,
        )
        .unwrap()
    }

    fn story(epic: Option<&str>, feature: Option<&str>) -> Js {
        let mut text = String::from("---\nid: VC-073\ntitle: Cut varsity-engine results calls over to the Core API\nstatus: refined\ntype: story\n");
        if let Some(e) = epic {
            text.push_str(&format!("epic: {e}\n"));
        }
        if let Some(f) = feature {
            text.push_str(&format!("feature: {f}\n"));
        }
        text.push_str("---\n\n## Story\nAs a maintainer, I want results through Core.\n\n## Acceptance Criteria\n- [ ] Given x, when y, then z\n");
        crate::lenient_story::parse(&text).unwrap()
    }

    /// Answers `epic` and `feature_2` with fixed choices; counts its calls; can fail.
    struct Scripted {
        epic: (&'static str, f64),
        feature: (&'static str, f64),
        fail: Option<&'static str>,
        calls: usize,
        model: &'static str,
    }
    fn answer(choice: &str, confidence: f64) -> Value {
        json!({"type":"choice","choice":choice,"confidence":confidence,"probabilities":{}})
    }
    impl Classifier for Scripted {
        fn classify(
            &mut self,
            state: &Value,
            q: &Map<String, Value>,
        ) -> Result<Classified, String> {
            self.calls += 1;
            assert_eq!(
                state["title"],
                "Cut varsity-engine results calls over to the Core API"
            );
            assert!(state.get("id").is_none(), "ids never leave the machine");
            if let Some(reason) = self.fail {
                return Err(reason.into());
            }
            let mut a = Map::new();
            for id in q.keys() {
                a.insert(
                    id.clone(),
                    answer(
                        if id == "epic" {
                            self.epic.0
                        } else {
                            self.feature.0
                        },
                        if id == "epic" {
                            self.epic.1
                        } else {
                            self.feature.1
                        },
                    ),
                );
            }
            Ok(Classified {
                answers: a,
                source: Source::Jev {
                    model: self.model.into(),
                },
            })
        }
    }
    fn jev(epic: (&'static str, f64), feature: (&'static str, f64)) -> Scripted {
        Scripted {
            epic,
            feature,
            fail: None,
            calls: 0,
            model: "jev-1.14.0",
        }
    }
    fn code() -> Scripted {
        Scripted {
            epic: ("core_cutover", 0.5),
            feature: ("engine_call_cutovers", 0.5),
            fail: None,
            calls: 0,
            model: "unused",
        }
    }
    const OPTS: Options = Options {
        min_confidence: 0.7,
        reassign: false,
    };

    #[test]
    fn a_confident_jev_choice_assigns_names_from_the_bucket() {
        let (mut p, mut f) = (
            jev(("core_cutover", 0.99), ("engine_call_cutovers", 0.91)),
            code(),
        );
        let out = place(
            &story(Some("VC-CORE"), None),
            &taxonomy(),
            &mut p,
            &mut f,
            OPTS,
        );
        assert_eq!(out.outcome, Outcome::Assigned);
        assert_eq!(out.current_epic.as_deref(), Some("VC-CORE"));
        assert_eq!(out.epic.unwrap().name.as_deref(), Some("Core cutover"));
        assert_eq!(
            out.feature.unwrap().name.as_deref(),
            Some("Engine call cutovers")
        );
        assert_eq!(
            out.source,
            Some(Source::Jev {
                model: "jev-1.14.0".into()
            })
        );
        assert_eq!((p.calls, f.calls), (1, 0));
    }

    #[test]
    fn below_the_threshold_or_none_fit_is_unresolved() {
        let (mut p, mut f) = (
            jev(("core_cutover", 0.99), ("engine_call_cutovers", 0.55)),
            code(),
        );
        let out = place(&story(None, None), &taxonomy(), &mut p, &mut f, OPTS);
        assert_eq!(
            out.outcome,
            Outcome::Unresolved("feature confidence 0.55 is below 0.7".into())
        );
        let (mut p, mut f) = (
            jev(("none_fit", 0.95), ("engine_call_cutovers", 0.9)),
            code(),
        );
        let out = place(&story(None, None), &taxonomy(), &mut p, &mut f, OPTS);
        assert_eq!(
            out.outcome,
            Outcome::Unresolved("no taxonomy epic fits".into())
        );
        let (mut p, mut f) = (jev(("core_cutover", 0.9), ("none_fit", 0.9)), code());
        let out = place(&story(None, None), &taxonomy(), &mut p, &mut f, OPTS);
        assert_eq!(
            out.outcome,
            Outcome::Unresolved("no feature of \"Core cutover\" fits".into())
        );
    }

    #[test]
    fn a_story_already_in_the_taxonomy_is_kept_without_a_call_unless_reassigned() {
        let s = story(Some("core cutover"), Some("Bypass retirement"));
        let (mut p, mut f) = (
            jev(("core_cutover", 0.99), ("engine_call_cutovers", 0.99)),
            code(),
        );
        let out = place(&s, &taxonomy(), &mut p, &mut f, OPTS);
        assert_eq!((out.outcome, p.calls), (Outcome::Kept, 0));
        let out = place(
            &s,
            &taxonomy(),
            &mut p,
            &mut f,
            Options {
                reassign: true,
                ..OPTS
            },
        );
        assert_eq!((out.outcome, p.calls), (Outcome::Assigned, 1));
    }

    #[test]
    fn the_same_confident_choice_is_unchanged() {
        let s = story(Some("Core cutover"), Some("Engine call cutovers"));
        let (mut p, mut f) = (
            jev(("core_cutover", 0.99), ("engine_call_cutovers", 0.99)),
            code(),
        );
        let out = place(
            &s,
            &taxonomy(),
            &mut p,
            &mut f,
            Options {
                reassign: true,
                ..OPTS
            },
        );
        assert_eq!(out.outcome, Outcome::Unchanged);
    }

    #[test]
    fn a_jev_failure_falls_back_to_code_and_says_why() {
        let mut p = jev(("core_cutover", 0.99), ("engine_call_cutovers", 0.99));
        p.fail = Some("canary_failed");
        let mut f = code();
        let out = place(&story(None, None), &taxonomy(), &mut p, &mut f, OPTS);
        assert_eq!(f.calls, 1);
        assert_eq!(
            out.source,
            Some(Source::Jev {
                model: "unused".into()
            }),
            "the fallback's own source is reported"
        );
        assert_eq!(
            out.outcome,
            Outcome::Unresolved("epic confidence 0.5 is below 0.7".into())
        );
    }
}
