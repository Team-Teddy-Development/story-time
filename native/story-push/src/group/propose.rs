//! Application service: proves proposed names with the classifier (Jev) before any is kept.
//! A proposal is applied to a trial taxonomy; it is accepted only if the classifier places at
//! least `min_cards` of its claimed uncovered stories into exactly that epic and feature, and
//! moves no story already placed in the taxonomy onto it. Accepted proposals accumulate, so
//! later proposals are checked against the earlier ones.

use super::classifier::Classifier;
use super::namer::Proposal;
use super::placement::{place, Options, Outcome};
use super::taxonomy::Taxonomy;
use crate::lenient_story::Js;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Verdict {
    pub(crate) proposal: Proposal,
    pub(crate) accepted: bool,
    pub(crate) reason: Option<String>,
    /// Claimed stories the classifier placed onto the new name.
    pub(crate) captured: Vec<String>,
    /// Already-placed stories the new name would take.
    pub(crate) stolen: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Rules {
    pub(crate) min_cards: usize,
    pub(crate) min_confidence: f64,
}

/// The taxonomy as its JSON file.
pub(crate) fn to_json(t: &Taxonomy) -> Value {
    json!({"epics": t.epics.iter().map(|e| json!({
        "name": e.name,
        "description": e.description,
        "features": e.features.iter().map(|f| json!({"name": f.name, "description": f.description})).collect::<Vec<_>>(),
    })).collect::<Vec<_>>()})
}

/// `t` with the proposal's feature added (and its epic, when new), re-validated by
/// `Taxonomy::parse`, so the caps and unique names hold for every accepted name.
pub(crate) fn with(t: &Taxonomy, p: &Proposal) -> Result<Taxonomy, String> {
    let mut raw = to_json(t);
    let feature = json!({"name": p.feature, "description": p.feature_description});
    let epics = raw["epics"].as_array_mut().expect("epics");
    match epics.iter_mut().find(|e| {
        e["name"]
            .as_str()
            .is_some_and(|n| n.trim().to_lowercase() == p.epic.trim().to_lowercase())
    }) {
        Some(epic) => epic["features"]
            .as_array_mut()
            .expect("features")
            .push(feature),
        None => {
            let description = p
                .epic_description
                .as_deref()
                .ok_or_else(|| format!("new epic \"{}\" needs a description", p.epic))?;
            epics.push(json!({"name": p.epic, "description": description, "features": [feature]}));
        }
    }
    Taxonomy::parse(&raw.to_string())
        .map_err(|e| e.strip_prefix("taxonomy: ").unwrap_or(&e).to_owned())
}

fn on(p: &super::placement::Placement, epic: &str, feature: &str) -> bool {
    matches!(p.outcome, Outcome::Assigned | Outcome::Unchanged)
        && p.epic.as_ref().and_then(|x| x.name.as_deref()) == Some(epic)
        && p.feature.as_ref().and_then(|x| x.name.as_deref()) == Some(feature)
}

pub(crate) fn verify(
    t: &Taxonomy,
    proposals: &[Proposal],
    gaps: &[Js],
    placed: &[Js],
    primary: &mut dyn Classifier,
    fallback: &mut dyn Classifier,
    rules: Rules,
) -> (Taxonomy, Vec<Verdict>) {
    let opts = Options {
        min_confidence: rules.min_confidence,
        reassign: true,
    };
    let mut current = t.clone();
    let mut verdicts = Vec::new();
    for p in proposals {
        let mut verdict = Verdict {
            proposal: p.clone(),
            accepted: false,
            reason: None,
            captured: vec![],
            stolen: vec![],
        };
        let trial = match with(&current, p) {
            Ok(trial) => trial,
            Err(reason) => {
                verdict.reason = Some(reason);
                verdicts.push(verdict);
                continue;
            }
        };
        let claimed: Result<Vec<&Js>, String> = p
            .stories
            .iter()
            .map(|id| {
                gaps.iter()
                    .find(|s| s.get("id").string() == *id)
                    .ok_or_else(|| format!("{id} is not an uncovered story"))
            })
            .collect();
        let claimed = match claimed {
            Ok(c) => c,
            Err(reason) => {
                verdict.reason = Some(reason);
                verdicts.push(verdict);
                continue;
            }
        };
        let epic = trial
            .epic(&p.epic)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let feature = p.feature.trim().to_owned();
        for story in &claimed {
            if on(
                &place(story, &trial, primary, fallback, opts),
                &epic,
                &feature,
            ) {
                verdict.captured.push(story.get("id").string());
            }
        }
        for story in placed {
            if on(
                &place(story, &trial, primary, fallback, opts),
                &epic,
                &feature,
            ) {
                verdict.stolen.push(story.get("id").string());
            }
        }
        verdict.reason = if !verdict.stolen.is_empty() {
            Some(format!(
                "it would move {} off its current feature",
                verdict.stolen.join(", ")
            ))
        } else if verdict.captured.len() < rules.min_cards {
            Some(format!(
                "Jev placed {} of {} claimed stories on it; at least {} are needed",
                verdict.captured.len(),
                claimed.len(),
                rules.min_cards
            ))
        } else {
            None
        };
        verdict.accepted = verdict.reason.is_none();
        if verdict.accepted {
            current = trial;
        }
        verdicts.push(verdict);
    }
    (current, verdicts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::classifier::{Classified, Source};
    use serde_json::Map;
    use std::collections::HashMap;

    fn taxonomy() -> Taxonomy {
        Taxonomy::parse(
            r#"{"epics":[
          {"name":"Athletes & privacy","description":"Athlete profiles and consent","features":[
            {"name":"Athlete profiles & guardian consent","description":"Profiles and consent"}]},
          {"name":"Core cutover","description":"Engine callers onto Core","features":[
            {"name":"Engine call cutovers","description":"One module"}]}]}"#,
        )
        .unwrap()
    }
    fn story(id: &str, title: &str, epic: Option<(&str, &str)>) -> Js {
        let placed = epic
            .map(|(e, f)| format!("epic: {e}\nfeature: {f}\n"))
            .unwrap_or_default();
        crate::lenient_story::parse(&format!("---\nid: {id}\ntitle: {title}\nstatus: refined\ntype: story\n{placed}---\n\n## Story\nAs a user, I want {title}.\n")).unwrap()
    }

    /// Answers by story title: the option whose text starts with the named epic or feature,
    /// `none_fit` when the trial taxonomy has no such option. Counts calls.
    struct ByTitle {
        map: HashMap<&'static str, (&'static str, &'static str)>,
        calls: usize,
    }
    impl Classifier for ByTitle {
        fn classify(
            &mut self,
            state: &Value,
            q: &Map<String, Value>,
        ) -> Result<Classified, String> {
            self.calls += 1;
            let (epic, feature) = self
                .map
                .get(state["title"].as_str().unwrap())
                .copied()
                .unwrap_or(("-", "-"));
            let answers = q
                .iter()
                .map(|(id, question)| {
                    let want = if id == "epic" { epic } else { feature };
                    let key = question["criteria"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .find(|(_, text)| text.as_str().unwrap().starts_with(&format!("{want}:")))
                        .map(|(k, _)| k.clone())
                        .unwrap_or("none_fit".into());
                    (
                        id.clone(),
                        json!({"type":"choice","choice":key,"confidence":0.95,"probabilities":{}}),
                    )
                })
                .collect();
            Ok(Classified {
                answers,
                source: Source::Jev {
                    model: "jev-1.14.0".into(),
                },
            })
        }
    }
    fn proposal(
        epic: &str,
        epic_description: Option<&str>,
        feature: &str,
        stories: &[&str],
    ) -> Proposal {
        Proposal {
            epic: epic.into(),
            epic_description: epic_description.map(Into::into),
            feature: feature.into(),
            feature_description: format!("{feature} work"),
            stories: stories.iter().map(|s| s.to_string()).collect(),
        }
    }
    const RULES: Rules = Rules {
        min_cards: 2,
        min_confidence: 0.7,
    };

    fn run(
        proposals: &[Proposal],
        map: &[(&'static str, (&'static str, &'static str))],
    ) -> (Taxonomy, Vec<Verdict>, usize) {
        let gaps = vec![
            story("VC-301", "Review guardian claims", None),
            story("VC-302", "Escalate disputed guardian claims", None),
            story("VC-303", "Coach practice plans", None),
        ];
        let placed = vec![story(
            "VC-020",
            "Edit athlete profile",
            Some(("Athletes & privacy", "Athlete profiles & guardian consent")),
        )];
        let mut jev = ByTitle {
            map: map.iter().copied().collect(),
            calls: 0,
        };
        let mut code = ByTitle {
            map: HashMap::new(),
            calls: 0,
        };
        let (t, v) = verify(
            &taxonomy(),
            proposals,
            &gaps,
            &placed,
            &mut jev,
            &mut code,
            RULES,
        );
        (t, v, jev.calls)
    }

    #[test]
    fn a_name_that_wins_its_stories_without_stealing_is_accepted_and_added() {
        let p = proposal(
            "athletes & PRIVACY",
            None,
            "Guardian claim review",
            &["VC-301", "VC-302"],
        );
        let (t, v, calls) = run(
            &[p],
            &[
                (
                    "Review guardian claims",
                    ("Athletes & privacy", "Guardian claim review"),
                ),
                (
                    "Escalate disputed guardian claims",
                    ("Athletes & privacy", "Guardian claim review"),
                ),
                (
                    "Edit athlete profile",
                    ("Athletes & privacy", "Athlete profiles & guardian consent"),
                ),
            ],
        );
        assert!(v[0].accepted, "{v:?}");
        assert_eq!(v[0].captured, ["VC-301", "VC-302"]);
        assert_eq!(
            calls, 3,
            "two claimed stories and one placed story are re-asked"
        );
        let athletes = t.epic("Athletes & privacy").unwrap();
        assert_eq!(
            athletes
                .features
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Athlete profiles & guardian consent",
                "Guardian claim review"
            ]
        );
        assert_eq!(
            to_json(&t)["epics"][0]["features"][1],
            json!({"name":"Guardian claim review","description":"Guardian claim review work"})
        );
    }

    #[test]
    fn too_few_captured_stories_or_a_stolen_story_rejects() {
        let p = proposal(
            "Athletes & privacy",
            None,
            "Guardian claim review",
            &["VC-301", "VC-302"],
        );
        let (t, v, _) = run(
            std::slice::from_ref(&p),
            &[(
                "Review guardian claims",
                ("Athletes & privacy", "Guardian claim review"),
            )],
        );
        assert!(!v[0].accepted);
        assert_eq!(
            v[0].reason.as_deref(),
            Some("Jev placed 1 of 2 claimed stories on it; at least 2 are needed")
        );
        assert_eq!(t, taxonomy(), "a rejected name never reaches the taxonomy");
        let (_, v, _) = run(
            &[p],
            &[
                (
                    "Review guardian claims",
                    ("Athletes & privacy", "Guardian claim review"),
                ),
                (
                    "Escalate disputed guardian claims",
                    ("Athletes & privacy", "Guardian claim review"),
                ),
                (
                    "Edit athlete profile",
                    ("Athletes & privacy", "Guardian claim review"),
                ),
            ],
        );
        assert!(!v[0].accepted);
        assert_eq!(v[0].stolen, ["VC-020"]);
        assert_eq!(
            v[0].reason.as_deref(),
            Some("it would move VC-020 off its current feature")
        );
    }

    #[test]
    fn a_new_epic_needs_a_description_and_taxonomy_rules_still_apply() {
        let (_, v, calls) = run(
            &[proposal("Coach tools", None, "Practice plans", &["VC-303"])],
            &[],
        );
        assert_eq!(
            v[0].reason.as_deref(),
            Some("new epic \"Coach tools\" needs a description")
        );
        assert_eq!(calls, 0, "an invalid proposal costs no Jev call");
        let (_, v, _) = run(
            &[proposal(
                "Core cutover",
                None,
                "engine CALL cutovers",
                &["VC-301", "VC-302"],
            )],
            &[],
        );
        assert_eq!(v[0].reason.as_deref(), Some("feature \"engine call cutovers\" is listed twice; feature names share one GitHub field, so they must be unique"));
        let (_, v, _) = run(
            &[proposal(
                "Athletes & privacy",
                None,
                "Guardian claim review",
                &["VC-999", "VC-301"],
            )],
            &[],
        );
        assert_eq!(
            v[0].reason.as_deref(),
            Some("VC-999 is not an uncovered story")
        );
    }

    #[test]
    fn a_new_epic_that_wins_is_added_with_its_description() {
        let p = proposal(
            "Coach tools",
            Some("Tools coaches use daily"),
            "Practice plans",
            &["VC-303", "VC-302"],
        );
        let (t, v, _) = run(
            &[p],
            &[
                ("Coach practice plans", ("Coach tools", "Practice plans")),
                (
                    "Escalate disputed guardian claims",
                    ("Coach tools", "Practice plans"),
                ),
                (
                    "Edit athlete profile",
                    ("Athletes & privacy", "Athlete profiles & guardian consent"),
                ),
            ],
        );
        assert!(v[0].accepted, "{v:?}");
        assert_eq!(t.epics.len(), 3);
        assert_eq!(
            t.epic("Coach tools").unwrap().description,
            "Tools coaches use daily"
        );
    }
}
