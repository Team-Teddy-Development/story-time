//! `story group [id...] [--apply] [--reassign] [--min-confidence N] [--taxonomy PATH] [--json]`:
//! places each open story in a named epic and feature from the team taxonomy. Jev decides
//! with a TypeSafe key; the keyword fallback suggests without one. A dry run by default.
//! This module is the composition root: it wires settings, the taxonomy, the classifiers,
//! the repository and the saga, and renders the report.

pub(crate) mod classifier;
pub(crate) mod placement;
pub(crate) mod questions;
pub(crate) mod report;
pub(crate) mod repository;
pub(crate) mod saga;
pub(crate) mod taxonomy;

use crate::{
    adoption_apply,
    config::{self, Flags},
    gateway::Gateway,
    judge::{Client, Retry, JEV_DEFAULT_MODEL},
    lenient_story::{trim, Js},
    list, secret, show, Failure,
};
use chrono::Utc;
use classifier::{Classifier, JevClassifier, KeywordClassifier};
use placement::{place, Options, Placement};
use repository::FileStoryRepository;
use std::{fs, path::PathBuf};
use taxonomy::Taxonomy;

pub(crate) struct Env {
    pub(crate) key: Option<String>,
    pub(crate) switch: Option<String>,
}

pub fn run(args: &[String]) -> Result<(String, u8), Failure> {
    let var = |name| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
    let env = Env {
        key: var("TYPESAFE_API_KEY"),
        switch: var("STORY_TIME_JEV"),
    };
    group(args, &env, crate::judge::JEV_ENDPOINT, Retry::DEFAULT)
}

const CLOSED: [&str; 3] = ["done", "split", "archived"];
const OFF: [&str; 4] = ["off", "0", "false", "no"];
const GUIDE: &str = "docs/designs/epic-feature-grouping.md";

pub(crate) fn group(
    args: &[String],
    env: &Env,
    endpoint: &str,
    retry: Retry,
) -> Result<(String, u8), Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let opts = Options {
        min_confidence: min_confidence(flags.value("min-confidence"))?,
        reassign: flags.has("reassign"),
    };
    let path = flags
        .value("taxonomy")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            loaded
                .path
                .parent()
                .map(|d| d.join("taxonomy.json"))
                .unwrap_or_else(|| loaded.root.join(".story-time/taxonomy.json"))
        });
    let text = fs::read_to_string(&path).map_err(|_| {
        format!(
            "No taxonomy at {}. Write the team's named epics and features there; see {GUIDE}.",
            path.display()
        )
    })?;
    let taxonomy = Taxonomy::parse(&text)?;
    if flags.has("sync-fields") {
        if settings["tracker"].as_str() != Some("github") {
            return Err("story group --sync-fields mirrors to GitHub organization issue fields; set tracker to github first.".into());
        }
        let epics: Vec<String> = taxonomy.epics.iter().map(|e| e.name.clone()).collect();
        let features: Vec<String> = taxonomy
            .epics
            .iter()
            .flat_map(|e| e.features.iter().map(|f| f.name.clone()))
            .collect();
        let mut gateway = Gateway::new(&settings, &loaded.root)?;
        let (owner, changes) = gateway.sync_issue_fields(&epics, &features)?;
        return Ok((report::sync(&owner, &changes), 0));
    }
    let stories: Vec<Js> = if flags.positional.is_empty() {
        list::stories(&dir, None)?
            .into_iter()
            .filter(|s| !matches!(s.get("status"), Js::Str(st) if CLOSED.contains(&st.as_str())))
            .collect()
    } else {
        show::require_all(&dir, &flags.positional)?
    };

    let key = match env.key.as_deref().filter(|k| !k.is_empty()) {
        Some(key) => Some(key.to_owned()),
        None => secret::jev_key(&loaded.root.join(".story-time"))?,
    };
    let key = key.map(|k| trim(&k).to_owned()).filter(|k| !k.is_empty());
    let off = env
        .switch
        .as_deref()
        .is_some_and(|s| OFF.contains(&trim(s).to_lowercase().as_str()));
    let mut fallback = KeywordClassifier {
        why: "keyword fallback".into(),
    };
    let (engine, mut primary): (Engine, Box<dyn Classifier>) = match key {
        Some(key) if !off => {
            let model = settings["jev"]["model"]
                .as_str()
                .unwrap_or(JEV_DEFAULT_MODEL)
                .to_owned();
            let client = Client::new(endpoint, key, model.clone(), retry)?;
            (Engine::Jev(model), Box::new(JevClassifier { client }))
        }
        _ => {
            let why = if off {
                "disabled by STORY_TIME_JEV"
            } else {
                "no TypeSafe key; run story secret set jev to use Jev"
            };
            (
                Engine::Code(why),
                Box::new(KeywordClassifier { why: why.into() }),
            )
        }
    };

    let work: Vec<(Js, Placement)> = stories
        .into_iter()
        .map(|story| {
            let p = place(&story, &taxonomy, primary.as_mut(), &mut fallback, opts);
            (story, p)
        })
        .collect();
    let written = if flags.has("apply") {
        let _lock = adoption_apply::lock_for_write(&loaded.root)?;
        let mut repo = FileStoryRepository { dir: dir.clone() };
        let date = Utc::now().format("%Y-%m-%d").to_string();
        saga::apply(&mut repo, &work, &date)?
    } else {
        Vec::new()
    };
    let placements: Vec<&Placement> = work.iter().map(|(_, p)| p).collect();
    let output = if flags.has("json") {
        format!("{}\n", report::json(&engine, &placements, &written))
    } else {
        report::human(&engine, &placements, &written, flags.has("apply"))
    };
    Ok((output, 0))
}

fn min_confidence(raw: Option<&str>) -> Result<f64, String> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_MIN_CONFIDENCE);
    };
    match raw.trim().parse::<f64>() {
        Ok(n) if (0.0..=1.0).contains(&n) => Ok(n),
        _ => Err(format!(
            "--min-confidence must be a number from 0 to 1, got \"{raw}\"."
        )),
    }
}

pub(crate) const DEFAULT_MIN_CONFIDENCE: f64 = 0.7;

pub(crate) enum Engine {
    Code(&'static str),
    Jev(String),
}

#[cfg(test)]
mod tests;
