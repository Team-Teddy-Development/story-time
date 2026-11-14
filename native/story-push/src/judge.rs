//! `story judge [id...] [--json]`: the judgement calls the INVEST lint cannot make (need vs
//! solution, real benefit, testable criteria, type, size, split pattern). With a TypeSafe key
//! the calls go to Jev, one batched request per story; without one, or whenever Jev cannot
//! answer, the code heuristics of the Node `CodeDecisionEngine` decide.

use crate::{
    config::{self, Flags},
    js_number,
    lenient_story::{trim, Js},
    lint::{acceptance_criteria, GHERKIN, SPACE, STATEMENT, VAGUE},
    list, secret, show, stats, table, Failure,
};
use regex_lite::Regex;
use serde_json::{json, Map, Value};
use std::{sync::LazyLock, time::Duration};

pub(crate) const JEV_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const JEV_DEFAULT_MODEL: &str = "jev-1.13.0";
const JEV_INPUT_USD_PER_MTOK: f64 = 0.042;
const FIB: [f64; 7] = [1.0, 2.0, 3.0, 5.0, 8.0, 13.0, 20.0];
const CLOSED: [&str; 3] = ["done", "split", "archived"];
const OFF: [&str; 4] = ["off", "0", "false", "no"];

pub fn run(args: &[String]) -> Result<(String, u8), Failure> {
    let var = |name| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
    let env = Env {
        key: var("TYPESAFE_API_KEY"),
        switch: var("STORY_TIME_JEV"),
    };
    judge(args, &env, JEV_ENDPOINT, Retry::DEFAULT)
}

struct Env {
    key: Option<String>,
    switch: Option<String>,
}

fn judge(
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
    let targets = if flags.positional.is_empty() {
        list::stories(&dir, None)?
            .into_iter()
            .filter(|s| !matches!(s.get("status"), Js::Str(status) if CLOSED.contains(&status.as_str())))
            .collect()
    } else {
        show::require_all(&dir, &flags.positional)?
    };
    let key = match env.key.as_deref().filter(|k| !k.is_empty()) {
        Some(key) => Some(key.to_owned()),
        None => secret::jev_key(&loaded.root.join(".story-time"))?,
    };
    let switch = env.switch.as_deref().map(|s| trim(s).to_lowercase());
    let key = key.map(|k| trim(&k).to_owned()).filter(|k| !k.is_empty());
    let (engine, mut client) = if switch.is_some_and(|s| OFF.contains(&s.as_str())) {
        (Engine::Code("disabled by STORY_TIME_JEV"), None)
    } else if let Some(key) = key {
        let model = settings["jev"]["model"]
            .as_str()
            .unwrap_or(JEV_DEFAULT_MODEL)
            .to_owned();
        let client = Client::new(endpoint, key, model.clone(), retry)?;
        (Engine::Jev(model), Some(client))
    } else {
        (
            Engine::Code("no TypeSafe key; run story secret set jev to use Jev"),
            None,
        )
    };
    let judgements: Vec<Judgement> = targets
        .iter()
        .map(|story| match &mut client {
            Some(client) => jev_judge(client, story),
            None => code_judge(story),
        })
        .collect();
    let output = if flags.has("json") {
        let info = match &engine {
            Engine::Code(why) => vec![
                ("mode".into(), Js::Str("code".into())),
                ("why".into(), Js::Str((*why).into())),
            ],
            Engine::Jev(model) => vec![
                ("mode".into(), Js::Str("jev".into())),
                ("model".into(), Js::Str(model.clone())),
                ("why".into(), Js::Str("TypeSafe key configured".into())),
            ],
        };
        Js::Obj(vec![
            ("engine".into(), Js::Obj(info)),
            (
                "judgements".into(),
                Js::Arr(judgements.iter().map(Judgement::js).collect()),
            ),
        ])
        .json()
    } else {
        let head = match &engine {
            Engine::Code(why) => format!("engine: code ({why})"),
            Engine::Jev(model) => format!("engine: jev ({model})"),
        };
        std::iter::once(head)
            .chain(judgements.iter().map(Judgement::human))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    Ok((format!("{output}\n"), 0))
}

enum Engine {
    Code(&'static str),
    Jev(String),
}

struct Decision {
    value: Js,
    confidence: f64,
    reason: Option<String>,
}
impl Decision {
    fn new(value: Js, confidence: f64) -> Self {
        Self {
            value,
            confidence,
            reason: None,
        }
    }
    fn because(value: Js, confidence: f64, reason: impl Into<String>) -> Self {
        Self {
            value,
            confidence,
            reason: Some(reason.into()),
        }
    }
    fn js(&self) -> Js {
        let mut entries = vec![
            ("value".into(), self.value.clone()),
            ("confidence".into(), Js::Num(self.confidence)),
        ];
        if let Some(reason) = &self.reason {
            entries.push(("reason".into(), Js::Str(reason.clone())));
        }
        Js::Obj(entries)
    }
}

struct Criterion {
    text: String,
    testable: bool,
    confidence: f64,
}

struct Decisions {
    negotiable: Option<Decision>,
    valuable: Option<Decision>,
    criteria: Vec<Criterion>,
    kind: Decision,
    size: Option<Decision>,
    split_pattern: Option<Decision>,
}

enum Source {
    Code,
    Fallback(&'static str),
    Jev {
        model: String,
        input: u64,
        output: u64,
    },
}

struct Judgement {
    id: Js,
    title: Js,
    source: Source,
    decisions: Decisions,
}
impl Judgement {
    fn js(&self) -> Js {
        let d = &self.decisions;
        let mut decisions = Vec::new();
        for (key, decision) in [("negotiable", &d.negotiable), ("valuable", &d.valuable)] {
            if let Some(decision) = decision {
                decisions.push((key.into(), decision.js()));
            }
        }
        let criteria = d.criteria.iter().enumerate().map(|(i, c)| {
            Js::Obj(vec![
                ("index".into(), Js::Num((i + 1) as f64)),
                ("text".into(), Js::Str(c.text.clone())),
                ("testable".into(), Js::Bool(c.testable)),
                ("confidence".into(), Js::Num(c.confidence)),
            ])
        });
        decisions.push(("criteria".into(), Js::Arr(criteria.collect())));
        decisions.push(("type".into(), d.kind.js()));
        for (key, decision) in [("size", &d.size), ("splitPattern", &d.split_pattern)] {
            if let Some(decision) = decision {
                decisions.push((key.into(), decision.js()));
            }
        }
        let mut entries = vec![("id".into(), self.id.clone())];
        match &self.source {
            Source::Code => entries.push(("source".into(), Js::Str("code".into()))),
            Source::Fallback(_) => entries.push(("source".into(), Js::Str("fallback".into()))),
            Source::Jev {
                model,
                input,
                output,
            } => entries.extend([
                ("source".into(), Js::Str("jev".into())),
                ("model".into(), Js::Str(model.clone())),
                (
                    "usage".into(),
                    Js::Obj(vec![
                        ("inputTokens".into(), Js::Num(*input as f64)),
                        ("outputTokens".into(), Js::Num(*output as f64)),
                        ("costUsd".into(), Js::Num(cost_usd(*input))),
                    ]),
                ),
            ]),
        }
        entries.push(("decisions".into(), Js::Obj(decisions)));
        if let Source::Fallback(reason) = self.source {
            entries.push(("reason".into(), Js::Str(reason.into())));
        }
        entries.push(("title".into(), self.title.clone()));
        Js::Obj(entries)
    }

    /// `formatJudgement`.
    fn human(&self) -> String {
        let d = &self.decisions;
        let yes_no = |v: &Js| {
            if matches!(v, Js::Bool(true)) {
                "yes"
            } else {
                "no"
            }
        };
        let row = |label: String, value: String, confidence: f64, why: &str| {
            vec![label, value, format!("{confidence:.2}"), why.to_owned()]
        };
        let mut rows = Vec::new();
        for (label, decision) in [
            ("need, not solution", &d.negotiable),
            ("real benefit", &d.valuable),
        ] {
            if let Some(x) = decision {
                rows.push(row(
                    label.into(),
                    yes_no(&x.value).into(),
                    x.confidence,
                    x.reason.as_deref().unwrap_or(""),
                ));
            }
        }
        for (i, c) in d.criteria.iter().enumerate() {
            let value = if c.testable { "yes" } else { "no" };
            rows.push(row(
                format!("criterion {} testable", i + 1),
                value.into(),
                c.confidence,
                "",
            ));
        }
        rows.push(row(
            "type".into(),
            d.kind.value.string(),
            d.kind.confidence,
            "",
        ));
        if let Some(x) = &d.size {
            rows.push(row(
                "size".into(),
                x.value.string(),
                x.confidence,
                x.reason.as_deref().unwrap_or(""),
            ));
        }
        if let Some(x) = &d.split_pattern {
            rows.push(row(
                "split pattern".into(),
                x.value.string(),
                x.confidence,
                "",
            ));
        }
        let (source, model) = match &self.source {
            Source::Code => ("code", String::new()),
            Source::Fallback(_) => ("fallback", String::new()),
            Source::Jev { model, .. } => ("jev", format!(" {model}")),
        };
        let mut out = format!(
            "{}  {}  [{source}{model}]",
            self.id.string(),
            self.title.string()
        );
        if let Source::Fallback(reason) = self.source {
            out.push_str(&format!(
                "\n  Jev unavailable ({reason}); code heuristics used."
            ));
        }
        let body = table(&["Decision", "Value", "Conf", "Why"], rows);
        for line in body.trim_end_matches('\n').split('\n') {
            out.push_str(&format!("\n  {line}"));
        }
        if let Source::Jev { input, output, .. } = self.source {
            out.push_str(&format!(
                "\n  tokens: {input} in / {output} out, about ${}",
                js_number::to_string(cost_usd(input))
            ));
        }
        out
    }
}

fn cost_usd(input: u64) -> f64 {
    js_round(input as f64 / 1e6 * JEV_INPUT_USD_PER_MTOK * 1e10) / 1e10
}

/// `Math.round`: halves round toward positive infinity.
fn js_round(n: f64) -> f64 {
    let floor = n.floor();
    if n - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

fn round2(n: f64) -> f64 {
    js_round(n * 100.0) / 100.0
}

fn is_spike(story: &Js) -> bool {
    matches!(story.get("type"), Js::Str(t) if t == "spike")
}

/// `(spike ? sections?.Question : sections?.Story) ?? ''`.
fn statement(story: &Js) -> String {
    let name = if is_spike(story) { "Question" } else { "Story" };
    match story.get("sections").get(name) {
        Js::Undefined => String::new(),
        v => v.string(),
    }
}

fn title(story: &Js) -> String {
    match story.get("title") {
        Js::Undefined => String::new(),
        v => v.string(),
    }
}

static SOLUTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(button|dropdown|drop-down|checkbox|radio button|modal|pop-?up|tooltip|database|db|table|column|schema|endpoint|api|microservice|redis|postgres|mysql|sql|mongo(?:db)?|react|angular|vue|kafka|lambda|cron(?:job)?|webhook|json)\b").unwrap()
});
static CIRCULAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(use|have|access) (the|this|that) (feature|functionality|app|application|system|tool)\b").unwrap()
});
static BUG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(bug|crash(?:es|ed|ing)?|broken|error|errors|fails?|failing|failed|regression|incorrect|wrong|throws?|throwing)\b").unwrap()
});
static SPIKE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(investigate|research|spike|evaluate|explore|prototype|feasib\w*|whether)\b")
        .unwrap()
});
static CHORE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(upgrade|bump|refactor|migrate|ci|lint|dependency|dependencies|cleanup|clean up)\b",
    )
    .unwrap()
});
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new("[a-z0-9]+").unwrap());
static AND: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i) and ").unwrap());
const STOP: [&str; 23] = [
    "i", "can", "to", "my", "a", "an", "the", "be", "able", "so", "that", "we", "our", "it", "is",
    "and", "of", "for", "me", "will", "get", "want", "need",
];

fn words(s: &str) -> Vec<String> {
    let lower = s.to_lowercase();
    WORD.find_iter(&lower)
        .map(|m| m.as_str().to_owned())
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect()
}

fn size_for(n: usize) -> f64 {
    match n {
        0 | 1 => 1.0,
        2 => 2.0,
        3 => 3.0,
        4 | 5 => 5.0,
        6..=8 => 8.0,
        9..=12 => 13.0,
        _ => FIB[FIB.len() - 1],
    }
}

fn needs_split(size: Option<&Decision>, criteria: usize) -> bool {
    let size = match size.map(|d| &d.value) {
        Some(Js::Num(n)) => *n,
        _ => 0.0,
    };
    size >= 8.0 || criteria > 8
}

/// `CodeDecisionEngine.judge`.
fn code_judge(story: &Js) -> Judgement {
    Judgement {
        id: story.get("id").clone(),
        title: story.get("title").clone(),
        source: Source::Code,
        decisions: code_decisions(story),
    }
}

fn code_decisions(story: &Js) -> Decisions {
    let spike = is_spike(story);
    let statement = statement(story);
    let acs = acceptance_criteria(story);
    let m = STATEMENT.captures(&statement);
    let (mut negotiable, mut valuable) = (None, None);
    if !spike {
        let goal = m
            .as_ref()
            .and_then(|m| m.get(2))
            .map_or(statement.as_str(), |g| g.as_str());
        negotiable = Some(match SOLUTION.captures(goal) {
            Some(solution) => Decision::because(
                Js::Bool(false),
                0.6,
                format!("names a solution: \"{}\"", &solution[1]),
            ),
            None => Decision::new(Js::Bool(true), 0.5),
        });
        valuable = Some(match m.as_ref().and_then(|m| m.get(3)) {
            None => Decision::because(Js::Bool(false), 0.7, "no \"so that\" benefit"),
            Some(benefit) if CIRCULAR.is_match(benefit.as_str()) => {
                Decision::because(Js::Bool(false), 0.6, "the benefit is \"using the feature\"")
            }
            Some(benefit) => {
                let goal_words = words(goal);
                let benefit_words = words(benefit.as_str());
                let restated = !benefit_words.is_empty()
                    && benefit_words.iter().all(|w| goal_words.contains(w));
                if restated {
                    Decision::because(Js::Bool(false), 0.6, "the benefit restates the goal")
                } else {
                    Decision::new(Js::Bool(true), 0.5)
                }
            }
        });
    }
    let criteria = acs
        .iter()
        .map(|text| Criterion {
            text: text.clone(),
            testable: GHERKIN.is_match(text) && !VAGUE.is_match(text),
            confidence: 0.5,
        })
        .collect();
    let text = format!("{} {statement}", title(story));
    let kind = if BUG.is_match(&text) {
        "bug"
    } else if SPIKE.is_match(&text) {
        "spike"
    } else if CHORE.is_match(&text) {
        "chore"
    } else {
        "story"
    };
    let (mut size, mut split_pattern) = (None, None);
    if !spike {
        let sized = Decision::because(
            Js::Num(size_for(acs.len())),
            0.3,
            format!("{} acceptance criteria", acs.len()),
        );
        if needs_split(Some(&sized), acs.len()) {
            let goal = m.as_ref().and_then(|m| m.get(2)).map_or("", |g| g.as_str());
            let pattern = if AND.is_match(goal) {
                "workflow-steps"
            } else if acs.len() > 8 {
                "paths"
            } else {
                "simple-complex"
            };
            split_pattern = Some(Decision::new(Js::Str(pattern.into()), 0.3));
        }
        size = Some(sized);
    }
    Decisions {
        negotiable,
        valuable,
        criteria,
        kind: Decision::new(Js::Str(kind.into()), 0.55),
        size,
        split_pattern,
    }
}

/// `JevDecisionEngine.judge`: one batched request, and the code engine whenever Jev cannot answer.
fn jev_judge(client: &mut Client, story: &Js) -> Judgement {
    let fallback = |reason| Judgement {
        source: Source::Fallback(reason),
        ..code_judge(story)
    };
    let answer = match client.decide(&judge_state(story), &judge_questions(story)) {
        Ok(answer) => answer,
        Err(reason) => return fallback(reason),
    };
    Judgement {
        id: story.get("id").clone(),
        title: story.get("title").clone(),
        source: Source::Jev {
            model: answer.model,
            input: answer.input,
            output: answer.output,
        },
        decisions: interpret(story, &answer.answers),
    }
}

fn judge_state(story: &Js) -> Value {
    let criteria = acceptance_criteria(story);
    let title = match story.get("title") {
        Js::Undefined => Value::String(String::new()),
        v => v.to_value(),
    };
    let count = criteria.len();
    json!({
        "title": title,
        "persona": story.get("persona").to_value(),
        "statement": trim(&statement(story)),
        "criteria": criteria,
        "count": count,
    })
}

const TYPE_OPTIONS: [(&str, &str); 4] = [
    (
        "story",
        "new or changed behaviour that gives a user or the business a benefit",
    ),
    (
        "bug",
        "existing behaviour that is broken and should be fixed",
    ),
    (
        "spike",
        "time-boxed research that answers a question before anything is built",
    ),
    (
        "chore",
        "technical upkeep with no direct change a user would notice",
    ),
];

const SPLIT_PATTERNS: [(&str, &str); 10] = [
    (
        "workflow-steps",
        "split by the steps of the user journey, one step per story",
    ),
    (
        "paths",
        "ship the happy path first, then each alternate path as its own story",
    ),
    (
        "business-rules",
        "one business rule or rule variation per story",
    ),
    (
        "data-variations",
        "one kind of data, input, or source at a time",
    ),
    ("interfaces", "one channel, device, or interface at a time"),
    (
        "simple-complex",
        "the simplest useful version first, then the harder cases",
    ),
    (
        "operations",
        "split create, read, update, and delete into separate stories",
    ),
    (
        "defer-quality",
        "make it work first, then performance, accessibility, or localisation",
    ),
    (
        "relax-rules",
        "ship without a constraint first, then add the constraint",
    ),
    (
        "spike",
        "pull the unknown out into a time-boxed research spike",
    ),
];

const SIZE_LEVELS: [&str; 7] = [
    "1 point: trivial, one obvious change with a single acceptance criterion",
    "2 points: small, a couple of criteria in one familiar part of the product",
    "3 points: modest, a few criteria, still well understood",
    "5 points: medium, several criteria or a few parts of the product",
    "8 points: large, many criteria or unfamiliar areas; probably worth splitting",
    "13 points: very large, too big to finish in one sprint without splitting",
    "20 points: epic sized, must be broken into several stories",
];

fn options(pairs: &[(&str, &str)]) -> Value {
    Value::Object(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), Value::String((*v).to_owned())))
            .collect(),
    )
}

fn noul(instructions: String, yes: &str, no: &str) -> Value {
    json!({"type": "noul", "instructions": instructions, "criteria": {"true": yes, "false": no}})
}

/// `judgeQuestions`.
fn judge_questions(story: &Js) -> Value {
    let spike = is_spike(story);
    let mut q = Map::new();
    if !spike {
        q.insert("negotiable".into(), noul(
            "Read state.statement, a user story written as \"As a <persona>, I want <goal>, so that <benefit>.\"".into(),
            "The statement describes what the persona needs and why, without prescribing a specific user interface control, data store, technology, or implementation.",
            "The statement prescribes a specific solution, such as a named user interface control, database table, API endpoint, framework, or other implementation detail.",
        ));
        q.insert("valuable".into(), noul(
            "Read the \"so that\" part of state.statement, which should say why the persona wants the goal.".into(),
            "The \"so that\" part names a concrete gain for the persona or the business that is different from the goal itself.",
            "The \"so that\" part is missing, merely restates the goal, or names no concrete gain for anyone.",
        ));
    }
    for (i, text) in acceptance_criteria(story).iter().enumerate() {
        q.insert(format!("criterion_{}", i + 1), noul(
            format!("Judge this acceptance criterion of the story in state: \"{text}\""),
            "The criterion describes one observable outcome with concrete values, so an automated test could clearly pass or fail it.",
            "The criterion is vague, subjective, bundles several outcomes, or could not clearly pass or fail a test.",
        ));
    }
    q.insert(
        "type".into(),
        json!({"type": "choice", "instructions": "Classify the work described by state.title and state.statement.", "criteria": options(&TYPE_OPTIONS)}),
    );
    if !spike {
        q.insert("size".into(), json!({"type": "score", "instructions": "Estimate the relative effort of this story from state.statement and state.criteria, as a team would in planning poker.", "criteria": SIZE_LEVELS}));
        q.insert("split_pattern".into(), json!({"type": "choice", "instructions": "If this story had to be split into smaller vertical slices, which splitting pattern fits it best?", "criteria": options(&SPLIT_PATTERNS)}));
    }
    Value::Object(q)
}

/// `interpretAnswers` with the default threshold of 0.5, on answers [`decode`] accepted.
fn interpret(story: &Js, answers: &Map<String, Value>) -> Decisions {
    let num = |id: &str, key: &str| answers[id][key].as_f64().unwrap_or(0.0);
    let noul = |id: &str| {
        let p = num(id, "noul");
        Decision::new(Js::Bool(p >= 0.5), round2((2.0 * p - 1.0).abs()))
    };
    let negotiable = answers
        .contains_key("negotiable")
        .then(|| noul("negotiable"));
    let valuable = answers.contains_key("valuable").then(|| noul("valuable"));
    let criteria: Vec<Criterion> = acceptance_criteria(story)
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let d = noul(&format!("criterion_{}", i + 1));
            Criterion {
                text,
                testable: d.value == Js::Bool(true),
                confidence: d.confidence,
            }
        })
        .collect();
    let choice = |id: &str| Js::Str(answers[id]["choice"].as_str().unwrap_or("").to_owned());
    let kind = Decision::new(choice("type"), round2(num("type", "confidence")));
    let size = answers.contains_key("size").then(|| {
        let index = js_round(num("size", "score")).clamp(0.0, (FIB.len() - 1) as f64);
        Decision::new(
            Js::Num(FIB[index as usize]),
            round2(num("size", "confidence")),
        )
    });
    let split_pattern = (answers.contains_key("split_pattern")
        && needs_split(size.as_ref(), criteria.len()))
    .then(|| {
        Decision::new(
            choice("split_pattern"),
            round2(num("split_pattern", "confidence")),
        )
    });
    Decisions {
        negotiable,
        valuable,
        criteria,
        kind,
        size,
        split_pattern,
    }
}

#[derive(Clone, Copy)]
struct Retry {
    max_attempts: u32,
    base_delay_ms: f64,
    max_delay_ms: f64,
    jitter_ratio: f64,
    max_retry_after_ms: f64,
    request_timeout: Duration,
}
impl Retry {
    const DEFAULT: Retry = Retry {
        max_attempts: 3,
        base_delay_ms: 500.0,
        max_delay_ms: 5_000.0,
        jitter_ratio: 0.25,
        max_retry_after_ms: 30_000.0,
        request_timeout: Duration::from_millis(10_000),
    };
}

#[derive(Clone, Copy, PartialEq)]
enum Canary {
    Pending,
    Passed,
    Failed,
}

struct Answer {
    model: String,
    answers: Map<String, Value>,
    input: u64,
    output: u64,
}

/// `JevClient`: strict decoding, retries for 408/429/5xx/timeouts/transport errors, a key
/// rejection that latches for the run, and a contradiction canary before the first real call.
struct Client {
    endpoint: String,
    key: String,
    model: String,
    retry: Retry,
    http: reqwest::blocking::Client,
    canary: Canary,
    auth_rejected: bool,
    warned: Vec<&'static str>,
}

impl Client {
    fn new(endpoint: &str, key: String, model: String, retry: Retry) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(retry.request_timeout)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            endpoint: endpoint.to_owned(),
            key,
            model,
            retry,
            http,
            canary: Canary::Pending,
            auth_rejected: false,
            warned: Vec::new(),
        })
    }

    fn decide(&mut self, state: &Value, questions: &Value) -> Result<Answer, &'static str> {
        if self.auth_rejected {
            return Err("auth_rejected");
        }
        match self.canary {
            Canary::Failed => return Err("canary_failed"),
            Canary::Pending => self.run_canary()?,
            Canary::Passed => {}
        }
        self.send(state, questions)
    }

    fn run_canary(&mut self) -> Result<(), &'static str> {
        let state = json!({"statement": "As a shopper, I want to save my cart, so that I can finish checkout later."});
        let ask = |yes: &str, no: &str| noul("Read state.statement.".into(), yes, no);
        let questions = json!({
            "canary_true": ask("The statement says why the shopper wants to save the cart.", "The statement gives no reason for wanting to save the cart."),
            "canary_false_database": ask("The statement names a specific database or storage technology.", "The statement does not name any database or storage technology."),
            "canary_false_bicycle": ask("The statement is about repairing or riding a bicycle.", "The statement has nothing to do with bicycles."),
        });
        let answer = self.send(&state, &questions)?;
        let p = |id: &str| answer.answers[id]["noul"].as_f64().unwrap_or(0.0);
        let values: Vec<f64> = answer
            .answers
            .values()
            .map(|a| a["noul"].as_f64().unwrap_or(0.0))
            .collect();
        let spread = values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - values.iter().copied().fold(f64::INFINITY, f64::min);
        let correct = p("canary_true") >= 0.5
            && p("canary_false_database") < 0.5
            && p("canary_false_bicycle") < 0.5;
        if correct && spread >= 0.3 {
            self.canary = Canary::Passed;
            return Ok(());
        }
        self.canary = Canary::Failed;
        self.warn_once("canary", "Jev failed its contradiction check (it answered unrelated questions alike). Jev is off for this run; code heuristics decide.");
        Err("canary_failed")
    }

    fn send(&mut self, state: &Value, questions: &Value) -> Result<Answer, &'static str> {
        if egress_findings(state) || egress_findings(questions) {
            return Err("egress_refused");
        }
        let body = json!({"state": state, "model": self.model, "questions": questions}).to_string();
        let max = self.retry.max_attempts;
        for attempt in 1..=max {
            let response = self
                .http
                .post(&self.endpoint)
                .header("authorization", format!("Bearer {}", self.key))
                .header("content-type", "application/json")
                .body(body.clone())
                .send();
            let (reason, delay) = match response {
                Err(e) => (
                    if e.is_timeout() {
                        "timeout"
                    } else {
                        "transport_error"
                    },
                    None,
                ),
                Ok(r) if [301, 302, 303, 307, 308].contains(&r.status().as_u16()) => {
                    ("transport_error", None)
                }
                Ok(r) if r.status().is_success() => {
                    return r
                        .text()
                        .ok()
                        .and_then(|text| serde_json::from_str(&text).ok())
                        .and_then(|json| decode(questions, json))
                        .ok_or("protocol_error");
                }
                Ok(r) => match r.status().as_u16() {
                    status @ (401 | 403) => {
                        self.auth_rejected = true;
                        self.warn_once("auth", &format!("TypeSafe rejected the API key (HTTP {status}). Jev is off for this run; code heuristics decide. Check it with: story secret status"));
                        return Err("auth_rejected");
                    }
                    422 => return Err("validation_error"),
                    status @ (408 | 429 | 500..) => (
                        if status == 429 {
                            "rate_limited"
                        } else {
                            "overloaded"
                        },
                        self.retry_hint(r.headers()),
                    ),
                    _ => return Err("protocol_error"),
                },
            };
            if attempt == max {
                return Err(reason);
            }
            let ms = delay.unwrap_or_else(|| self.backoff(attempt));
            std::thread::sleep(Duration::from_secs_f64(ms / 1000.0));
        }
        Err("transport_error")
    }

    fn backoff(&self, attempt: u32) -> f64 {
        let Retry {
            base_delay_ms,
            max_delay_ms,
            jitter_ratio,
            ..
        } = self.retry;
        let delay = max_delay_ms.min(base_delay_ms * 2f64.powi(attempt as i32 - 1));
        js_round(delay + (random() * 2.0 - 1.0) * jitter_ratio * delay).max(1.0)
    }

    /// `retry-after-ms` first, then `retry-after` in seconds, both clamped; `None` backs off.
    fn retry_hint(&self, headers: &reqwest::header::HeaderMap) -> Option<f64> {
        let clamp = |ms: f64| ms.max(0.0).min(self.retry.max_retry_after_ms);
        let read = |name| {
            headers
                .get(name)
                .map(|v| stats::number(&String::from_utf8_lossy(v.as_bytes())))
                .filter(|n| n.is_finite())
        };
        read("retry-after-ms")
            .map(clamp)
            .or_else(|| read("retry-after").map(|s| clamp(s * 1000.0)))
    }

    fn warn_once(&mut self, key: &'static str, message: &str) {
        if !self.warned.contains(&key) {
            self.warned.push(key);
            eprintln!("Jev: {message}");
        }
    }
}

fn random() -> f64 {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 8];
    match ring::rand::SystemRandom::new().fill(&mut bytes) {
        Ok(()) => (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64,
        Err(_) => 0.5,
    }
}

fn is_prob(v: &Value) -> bool {
    v.as_f64().is_some_and(|n| (0.0..=1.0).contains(&n))
}

fn is_count(v: &Value) -> bool {
    v.as_f64()
        .is_some_and(|n| n >= 0.0 && n.fract() == 0.0 && n <= 9_007_199_254_740_991.0)
}

/// JavaScript's `typeof v === 'object' && v`, as an object's own properties.
fn object(v: &Value) -> Option<Map<String, Value>> {
    match v {
        Value::Object(map) => Some(map.clone()),
        Value::Array(items) => Some(
            items
                .iter()
                .enumerate()
                .map(|(i, x)| (i.to_string(), x.clone()))
                .collect(),
        ),
        _ => None,
    }
}

/// `decodeResponse`: any missing, extra, mistyped, or out-of-range value rejects the response.
fn decode(questions: &Value, json: Value) -> Option<Answer> {
    let model = json.get("model")?.as_str().filter(|m| !m.is_empty())?;
    let usage = json
        .get("usage")
        .filter(|u| u.is_object() || u.is_array())?;
    let (input, output) = (usage.get("input_tokens")?, usage.get("output_tokens")?);
    if !is_count(input) || !is_count(output) {
        return None;
    }
    let answers = object(json.get("answers")?)?;
    let questions = questions.as_object()?;
    let mut want: Vec<_> = questions.keys().collect();
    let mut got: Vec<_> = answers.keys().collect();
    want.sort();
    got.sort();
    if want != got {
        return None;
    }
    for (id, q) in questions {
        let ans = &answers[id];
        if !(ans.is_object() || ans.is_array()) || ans.get("type") != q.get("type") {
            return None;
        }
        let ok = match q["type"].as_str() {
            Some("noul") => is_prob(&ans["noul"]),
            Some("choice") => {
                let chosen = ans["choice"]
                    .as_str()
                    .is_some_and(|c| q["criteria"].get(c).is_some());
                let probabilities = match &ans["probabilities"] {
                    p @ (Value::Object(_) | Value::Array(_)) => {
                        object(p).unwrap().values().all(is_prob)
                    }
                    _ => false,
                };
                chosen && is_prob(&ans["confidence"]) && probabilities
            }
            Some("score") => {
                let confidence = match ans.get("confidence") {
                    None => true,
                    Some(c) => is_prob(c),
                };
                ans["score"].as_f64().is_some_and(f64::is_finite) && confidence
            }
            _ => true,
        };
        if !ok {
            return None;
        }
    }
    Some(Answer {
        model: model.to_owned(),
        answers,
        input: input.as_f64()? as u64,
        output: output.as_f64()? as u64,
    })
}

static EGRESS: LazyLock<[Regex; 3]> = LazyLock::new(|| {
    [
        Regex::new(r"[\w.+-]+@[\w-]+\.[\w.-]+").unwrap(),
        Regex::new(&format!(
            r"(?:\+?1[{SPACE}.-]?)?\(?\d{{3}}\)?[{SPACE}.-]?\d{{3}}[{SPACE}.-]?\d{{4}}\b"
        ))
        .unwrap(),
        Regex::new(r"\bgh[pousr]_[A-Za-z0-9]{20,}|\bgithub_pat_[A-Za-z0-9_]{20,}|\bAKIA[0-9A-Z]{16}\b|\bsk-[A-Za-z0-9_-]{20,}|\bxox[abprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----").unwrap(),
    ]
});

/// `scanPayload(...).length > 0`: any string that looks like personal data or a secret.
fn egress_findings(v: &Value) -> bool {
    match v {
        Value::String(s) => EGRESS.iter().any(|re| re.is_match(s)),
        Value::Array(items) => items.iter().any(egress_findings),
        Value::Object(map) => map.values().any(egress_findings),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
