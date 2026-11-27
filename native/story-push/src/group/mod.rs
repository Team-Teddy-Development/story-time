//! `story group [id...] [--apply] [--reassign] [--min-confidence N] [--taxonomy PATH] [--json]`:
//! places each open story in a named epic and feature from the team taxonomy. Jev decides
//! with a TypeSafe key; the keyword fallback suggests without one. A dry run by default.
//!
//! `story taxonomy propose [--max 3] [--min-cards 2] [--min-gaps 5] [--names-from FILE]
//! [--model ID] [--write] [--json]`: invents names for the stories `story group` leaves
//! unresolved (Claude, or proposals from a file) and keeps only those Jev proves.
//!
//! This module is the composition root: it wires settings, the taxonomy, the classifiers,
//! the namer, the repository and the saga, and renders the reports.

pub(crate) mod classifier;
pub(crate) mod namer;
pub(crate) mod placement;
pub(crate) mod propose;
pub(crate) mod questions;
pub(crate) mod report;
pub(crate) mod repository;
pub(crate) mod saga;
pub(crate) mod taxonomy;

use crate::{
    adoption_apply,
    config::{self, Flags, Loaded},
    gateway::Gateway,
    judge::{Client, Retry, JEV_DEFAULT_MODEL},
    lenient_story::{trim, Js},
    list, secret, show, Failure,
};
use chrono::Utc;
use classifier::{Classifier, JevClassifier, KeywordClassifier};
use namer::{ClaudeNamer, FileNamer, Gap, Namer};
use placement::{place, Options, Outcome, Placement};
use repository::FileStoryRepository;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use taxonomy::Taxonomy;

pub(crate) struct Env {
    pub(crate) key: Option<String>,
    pub(crate) switch: Option<String>,
    pub(crate) anthropic: Option<String>,
}

impl Env {
    fn from_process() -> Self {
        let var = |name| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
        Env {
            key: var("TYPESAFE_API_KEY"),
            switch: var("STORY_TIME_JEV"),
            anthropic: var("ANTHROPIC_API_KEY"),
        }
    }
}

/// Where the two remote services live; tests point both at local fakes.
#[derive(Clone, Copy)]
pub(crate) struct Endpoints<'a> {
    pub(crate) jev: &'a str,
    pub(crate) anthropic: &'a str,
}

const LIVE: Endpoints<'static> = Endpoints {
    jev: crate::judge::JEV_ENDPOINT,
    anthropic: namer::ANTHROPIC_ENDPOINT,
};

pub fn run(args: &[String]) -> Result<(String, u8), Failure> {
    group(args, &Env::from_process(), LIVE.jev, Retry::DEFAULT)
}

/// `story taxonomy <propose>`.
pub fn run_taxonomy(args: &[String]) -> Result<(String, u8), Failure> {
    taxonomy_cmd(args, &Env::from_process(), LIVE, Retry::DEFAULT)
}

const CLOSED: [&str; 3] = ["done", "split", "archived"];
const OFF: [&str; 4] = ["off", "0", "false", "no"];
const GUIDE: &str = "docs/designs/epic-feature-grouping.md";

/// Everything both commands read before deciding anything.
struct Setup {
    flags: Flags,
    loaded: Loaded,
    settings: Value,
    dir: PathBuf,
    path: PathBuf,
    taxonomy: Taxonomy,
}

fn setup(args: &[String]) -> Result<Setup, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
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
    Ok(Setup {
        flags,
        loaded,
        settings,
        dir,
        path,
        taxonomy,
    })
}

/// The open stories, or the named ones.
fn targets(s: &Setup, ids: &[String]) -> Result<Vec<Js>, Failure> {
    Ok(if ids.is_empty() {
        list::stories(&s.dir, None)?
            .into_iter()
            .filter(|st| !matches!(st.get("status"), Js::Str(x) if CLOSED.contains(&x.as_str())))
            .collect()
    } else {
        show::require_all(&s.dir, ids)?
    })
}

/// Jev when a TypeSafe key is configured and not switched off; the keyword fallback otherwise.
fn classifiers(
    s: &Setup,
    env: &Env,
    endpoint: &str,
    retry: Retry,
) -> Result<(Engine, Box<dyn Classifier>, KeywordClassifier), Failure> {
    let key = match env.key.as_deref().filter(|k| !k.is_empty()) {
        Some(key) => Some(key.to_owned()),
        None => secret::jev_key(&s.loaded.root.join(".story-time"))?,
    };
    let key = key.map(|k| trim(&k).to_owned()).filter(|k| !k.is_empty());
    let off = env
        .switch
        .as_deref()
        .is_some_and(|x| OFF.contains(&trim(x).to_lowercase().as_str()));
    let fallback = KeywordClassifier {
        why: "keyword fallback".into(),
    };
    Ok(match key {
        Some(key) if !off => {
            let model = s.settings["jev"]["model"]
                .as_str()
                .unwrap_or(JEV_DEFAULT_MODEL)
                .to_owned();
            let client = Client::new(endpoint, key, model.clone(), retry)?;
            (
                Engine::Jev(model),
                Box::new(JevClassifier { client }),
                fallback,
            )
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
                fallback,
            )
        }
    })
}

pub(crate) fn group(
    args: &[String],
    env: &Env,
    endpoint: &str,
    retry: Retry,
) -> Result<(String, u8), Failure> {
    let s = setup(args)?;
    let opts = Options {
        min_confidence: min_confidence(s.flags.value("min-confidence"))?,
        reassign: s.flags.has("reassign"),
    };
    if s.flags.has("sync-fields") {
        if s.settings["tracker"].as_str() != Some("github") {
            return Err("story group --sync-fields mirrors to GitHub organization issue fields; set tracker to github first.".into());
        }
        let epics: Vec<String> = s.taxonomy.epics.iter().map(|e| e.name.clone()).collect();
        let features: Vec<String> = s
            .taxonomy
            .epics
            .iter()
            .flat_map(|e| e.features.iter().map(|f| f.name.clone()))
            .collect();
        let mut gateway = Gateway::new(&s.settings, &s.loaded.root)?;
        let (owner, changes) = gateway.sync_issue_fields(&epics, &features)?;
        return Ok((report::sync(&owner, &changes), 0));
    }
    let stories = targets(&s, &s.flags.positional)?;
    let (engine, mut primary, mut fallback) = classifiers(&s, env, endpoint, retry)?;
    let work: Vec<(Js, Placement)> = stories
        .into_iter()
        .map(|story| {
            let p = place(&story, &s.taxonomy, primary.as_mut(), &mut fallback, opts);
            (story, p)
        })
        .collect();
    let written = if s.flags.has("apply") {
        let _lock = adoption_apply::lock_for_write(&s.loaded.root)?;
        let mut repo = FileStoryRepository { dir: s.dir.clone() };
        let date = Utc::now().format("%Y-%m-%d").to_string();
        saga::apply(&mut repo, &work, &date)?
    } else {
        Vec::new()
    };
    let placements: Vec<&Placement> = work.iter().map(|(_, p)| p).collect();
    let output = if s.flags.has("json") {
        format!("{}\n", report::json(&engine, &placements, &written))
    } else {
        report::human(&engine, &placements, &written, s.flags.has("apply"))
    };
    Ok((output, 0))
}

fn count(flags: &Flags, name: &str, default: usize) -> Result<usize, String> {
    match flags.value(name) {
        None => Ok(default),
        Some(raw) => match raw.trim().parse::<usize>() {
            Ok(n) if n >= 1 => Ok(n),
            _ => Err(format!(
                "--{name} must be a whole number of 1 or more, got \"{raw}\"."
            )),
        },
    }
}

pub(crate) const DEFAULT_MAX: usize = 3;
pub(crate) const DEFAULT_MIN_CARDS: usize = 2;
pub(crate) const DEFAULT_MIN_GAPS: usize = 5;
/// Already-placed stories re-asked to check a new name steals none of them.
pub(crate) const STABILITY_SAMPLE: usize = 20;

fn namer_for(s: &Setup, env: &Env, endpoint: &str) -> Result<Box<dyn Namer>, Failure> {
    if let Some(file) = s.flags.value("names-from") {
        let text = fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
        return Ok(Box::new(FileNamer { text }));
    }
    let key = match env.anthropic.as_deref().filter(|k| !k.trim().is_empty()) {
        Some(key) => Some(key.trim().to_owned()),
        None => secret::anthropic_key(&s.loaded.root.join(".story-time"))?,
    };
    let Some(key) = key else {
        return Err("No Anthropic key: set ANTHROPIC_API_KEY (or .story-time/.anthropic.env, mode 600), or pass --names-from proposals.json.".into());
    };
    let http = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(Box::new(ClaudeNamer {
        endpoint: endpoint.to_owned(),
        key,
        model: s
            .flags
            .value("model")
            .unwrap_or(namer::DEFAULT_MODEL)
            .to_owned(),
        http,
    }))
}

fn gap(story: &Js) -> Gap {
    let state = placement::state(story);
    Gap {
        id: story.get("id").string(),
        title: state["title"].as_str().unwrap_or("").to_owned(),
        statement: state["statement"].as_str().unwrap_or("").to_owned(),
        criteria: state["criteria"]
            .as_array()
            .map(|c| {
                c.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

pub(crate) fn taxonomy_cmd(
    args: &[String],
    env: &Env,
    endpoints: Endpoints,
    retry: Retry,
) -> Result<(String, u8), Failure> {
    let s = setup(args)?;
    let (sub, ids) = match s.flags.positional.split_first() {
        Some((sub, ids)) if sub == "propose" => (sub.clone(), ids.to_vec()),
        _ => {
            return Err(Failure {
                code: 2,
                message: "Usage: story taxonomy propose [id...] [--max 3] [--min-cards 2] [--min-gaps 5] [--names-from FILE] [--model ID] [--write] [--json]".into(),
            })
        }
    };
    let _ = sub;
    let max = count(&s.flags, "max", DEFAULT_MAX)?;
    let rules = propose::Rules {
        min_cards: count(&s.flags, "min-cards", DEFAULT_MIN_CARDS)?,
        min_confidence: min_confidence(s.flags.value("min-confidence"))?,
    };
    let min_gaps = count(&s.flags, "min-gaps", DEFAULT_MIN_GAPS)?;
    let stories = targets(&s, &ids)?;
    let (engine, mut primary, mut fallback) = classifiers(&s, env, endpoints.jev, retry)?;
    let opts = Options {
        min_confidence: rules.min_confidence,
        reassign: false,
    };
    let (mut gaps, mut placed) = (Vec::new(), Vec::new());
    for story in stories {
        match place(&story, &s.taxonomy, primary.as_mut(), &mut fallback, opts).outcome {
            Outcome::Unresolved(_) => gaps.push(story),
            Outcome::Kept if placed.len() < STABILITY_SAMPLE => placed.push(story),
            _ => {}
        }
    }
    let json = s.flags.has("json");
    if gaps.len() < min_gaps {
        return Ok((
            report::proposals(&engine, None, gaps.len(), min_gaps, &[], false, json),
            0,
        ));
    }
    let mut namer = namer_for(&s, env, endpoints.anthropic)?;
    let label = namer.label();
    let mut proposals =
        namer.propose(&s.taxonomy, &gaps.iter().map(gap).collect::<Vec<_>>(), max)?;
    proposals.truncate(max);
    let (accepted, verdicts) = propose::verify(
        &s.taxonomy,
        &proposals,
        &gaps,
        &placed,
        primary.as_mut(),
        &mut fallback,
        rules,
    );
    let wrote = s.flags.has("write") && verdicts.iter().any(|v| v.accepted);
    if wrote {
        let _lock = adoption_apply::lock_for_write(&s.loaded.root)?;
        write_taxonomy(&s.path, &accepted)?;
    }
    Ok((
        report::proposals(
            &engine,
            Some(&label),
            gaps.len(),
            min_gaps,
            &verdicts,
            wrote,
            json,
        ),
        0,
    ))
}

fn write_taxonomy(path: &Path, t: &Taxonomy) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&propose::to_json(t)).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, format!("{text}\n")).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
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
