//! The INVEST / Definition of Ready lint from the Node `invest.mjs`, rule for rule, and
//! `story lint [id...] [--json]`. `set`, `rank`, and `judge` reuse [`lint_story`].
//!
//! Stories are the lenient [`Js`] values Node lints, so each rule keeps JavaScript's
//! truthiness and equality, and a value Node's rule throws on fails with Node's message.

use crate::{
    config::{self, Flags},
    lenient_story::{lines, trim, Js},
    list, show, Failure,
};
use regex_lite::Regex;
use std::{path::Path, sync::LazyLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Warning,
}
impl Severity {
    fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Finding {
    pub(crate) rule: &'static str,
    pub(crate) invest: &'static str,
    pub(crate) severity: Severity,
    pub(crate) message: String,
}

/// `ctx` in Node: the whole backlog, and the project root that `state_machine` paths are
/// relative to. A rule that needs one skips when it is absent.
#[derive(Clone, Copy, Default)]
pub(crate) struct Context<'a> {
    pub(crate) stories: Option<&'a [Js]>,
    pub(crate) root: Option<&'a Path>,
}

type Found = Result<Vec<(Severity, String)>, String>;

struct Rule {
    id: &'static str,
    invest: &'static str,
    check: fn(&Js, &Context) -> Found,
}

const RULES: [Rule; 16] = [
    rule("title.present", "N", title_present),
    rule("statement.format", "V", statement_format),
    rule("statement.benefit", "V", statement_benefit),
    rule("statement.persona", "V", statement_persona),
    rule("ac.present", "T", ac_present),
    rule("ac.gherkin", "T", ac_gherkin),
    rule("ac.vague", "T", ac_vague),
    rule("ac.count", "S", ac_count),
    rule("size.estimated", "E", size_estimated),
    rule("size.small", "S", size_small),
    rule("value.scored", "V", value_scored),
    rule("independent.deps", "I", independent_deps),
    rule("spike.timebox", "E", spike_timebox),
    rule("spike.question", "T", spike_question),
    rule("spike.unblocks", "V", spike_unblocks),
    rule("state_machine.exists", "T", state_machine_exists),
];

const fn rule(id: &'static str, invest: &'static str, check: fn(&Js, &Context) -> Found) -> Rule {
    Rule { id, invest, check }
}

/// `lintStory(story, ctx).findings`, in rule order. `Err` carries the TypeError Node throws.
pub(crate) fn lint_story(story: &Js, ctx: &Context) -> Result<Vec<Finding>, String> {
    let mut findings = Vec::new();
    for rule in &RULES {
        for (severity, message) in (rule.check)(story, ctx)? {
            findings.push(Finding {
                rule: rule.id,
                invest: rule.invest,
                severity,
                message,
            });
        }
    }
    Ok(findings)
}

pub(crate) fn ready(findings: &[Finding]) -> bool {
    !findings.iter().any(|f| f.severity == Severity::Error)
}

/// JavaScript's `\s`, the characters inside a class.
const SPACE: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";
const SIZE_WARN: f64 = 8.0;
const SIZE_MAX: f64 = 13.0;
const AC_MAX: usize = 8;

// regex-lite folds case for ASCII only, as JavaScript's non-Unicode `i` flag does here.
pub(crate) static STATEMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?is)^[{SPACE}]*As (?:an?[{SPACE}]+|the[{SPACE}]+)?(.+?),[{SPACE}]*I (?:want|need|can)[{SPACE}]+(.+?)(?:,?[{SPACE}]*so that[{SPACE}]+(.+?))?\.?[{SPACE}]*$"
    ))
    .unwrap()
});
static GENERIC_PERSONA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)^(?:end[-{SPACE}]?)?(?:user|someone|anyone|somebody|person|people|customer|visitor)s?$"
    ))
    .unwrap()
});
pub(crate) static GHERKIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)^Given\b.+\bwhen\b.+\bthen\b").unwrap());
pub(crate) static VAGUE: LazyLock<Regex> = LazyLock::new(|| {
    let words = [
        "fast",
        "quickly",
        "easy",
        "easily",
        "simple",
        "intuitive",
        "user-friendly",
        "seamless",
        "robust",
        "properly",
        "correctly",
        "appropriate",
        "etc",
        "and so on",
        "as needed",
        "should work",
        "efficient",
        "flexible",
        "nice",
    ]
    .map(|w| w.replace(['-', ' '], &format!("[-{SPACE}]")))
    .join("|");
    Regex::new(&format!(r"(?i)\b({words})\b")).unwrap()
});
static LIST_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^[{SPACE}]*[-*][{SPACE}]+(?:\[( |x|X)\][{SPACE}]+)?([^\n\r\x{{2028}}\x{{2029}}]+)$"
    ))
    .unwrap()
});

fn err(message: impl Into<String>) -> Found {
    Ok(vec![(Severity::Error, message.into())])
}
fn warn(message: impl Into<String>) -> Found {
    Ok(vec![(Severity::Warning, message.into())])
}
fn none() -> Found {
    Ok(Vec::new())
}

fn truthy(v: &Js) -> bool {
    match v {
        Js::Undefined => false,
        Js::Bool(b) => *b,
        Js::Num(n) => *n != 0.0 && !n.is_nan(),
        Js::Str(s) => !s.is_empty(),
        Js::Arr(_) | Js::Obj(_) => true,
    }
}

/// SameValueZero, which `Map`, `Set`, and `Array.prototype.includes` use.
fn same_value_zero(a: &Js, b: &Js) -> bool {
    match (a, b) {
        (Js::Undefined, Js::Undefined) => true,
        (Js::Bool(a), Js::Bool(b)) => a == b,
        (Js::Num(a), Js::Num(b)) => a == b || (a.is_nan() && b.is_nan()),
        (Js::Str(a), Js::Str(b)) => a == b,
        _ => false,
    }
}

fn is_str(v: &Js, s: &str) -> bool {
    matches!(v, Js::Str(v) if v == s)
}

fn finite(v: &Js) -> Option<f64> {
    match v {
        Js::Num(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

fn is_spike(s: &Js) -> bool {
    is_str(s.get("type"), "spike")
}

fn needs_statement(s: &Js) -> bool {
    !truthy(s.get("type")) || is_str(s.get("type"), "story")
}

fn section<'a>(s: &'a Js, name: &str) -> &'a Js {
    s.get("sections").get(name)
}

fn statement(s: &Js) -> String {
    match section(s, "Story") {
        Js::Undefined => String::new(),
        v => v.string(),
    }
}

/// `acceptanceCriteria(story)`: the item texts of Acceptance Criteria, else Exit Criteria.
pub(crate) fn acceptance_criteria(s: &Js) -> Vec<String> {
    let raw = match section(s, "Acceptance Criteria") {
        Js::Undefined => match section(s, "Exit Criteria") {
            Js::Undefined => String::new(),
            v => v.string(),
        },
        v => v.string(),
    };
    lines(&raw)
        .into_iter()
        .filter_map(|line| LIST_ITEM.captures(line))
        .map(|m| trim(&m[2]).to_owned())
        .collect()
}

fn title_present(s: &Js, _: &Context) -> Found {
    match s.get("title") {
        Js::Undefined => err("Story has no title."),
        Js::Str(t) if trim(t).is_empty() => err("Story has no title."),
        Js::Str(_) => none(),
        _ => Err("s.title?.trim is not a function".into()),
    }
}

fn statement_format(s: &Js, _: &Context) -> Found {
    if !needs_statement(s) || STATEMENT.is_match(&statement(s)) {
        return none();
    }
    err(r###""## Story" must read "As a <persona>, I want <goal>, so that <benefit>.""###)
}

fn statement_benefit(s: &Js, _: &Context) -> Found {
    match STATEMENT.captures(&statement(s)) {
        Some(m) if needs_statement(s) && m.get(3).is_none() => {
            err(r#"Statement has no "so that <benefit>". Without the why, value cannot be judged."#)
        }
        _ => none(),
    }
}

fn statement_persona(s: &Js, _: &Context) -> Found {
    let text = statement(s);
    match STATEMENT.captures(&text) {
        Some(m) if needs_statement(s) && GENERIC_PERSONA.is_match(trim(&m[1])) => warn(format!(
            r#"Generic persona "{}". Name who benefits (e.g. "billing admin", "first-time buyer")."#,
            trim(&m[1])
        )),
        _ => none(),
    }
}

fn ac_present(s: &Js, _: &Context) -> Found {
    if is_spike(s) || !acceptance_criteria(s).is_empty() {
        return none();
    }
    err("No acceptance criteria. A story nobody can test cannot be done.")
}

fn ac_gherkin(s: &Js, _: &Context) -> Found {
    if is_spike(s) {
        return none();
    }
    Ok(acceptance_criteria(s)
        .into_iter()
        .filter(|ac| !GHERKIN.is_match(ac))
        .map(|ac| {
            (
                Severity::Warning,
                format!(r#"Criterion is not Given/When/Then: "{ac}""#),
            )
        })
        .collect())
}

fn ac_vague(s: &Js, _: &Context) -> Found {
    Ok(acceptance_criteria(s)
        .into_iter()
        .filter_map(|ac| {
            let word = VAGUE.captures(&ac)?[1].to_owned();
            Some((
                Severity::Warning,
                format!(
                    r#"Vague word "{word}" in "{ac}". Replace it with a measurable threshold."#
                ),
            ))
        })
        .collect())
}

fn ac_count(s: &Js, _: &Context) -> Found {
    let n = acceptance_criteria(s).len();
    if n <= AC_MAX {
        return none();
    }
    warn(format!(
        "{n} acceptance criteria (> {AC_MAX}). The story is probably several stories; consider story-split."
    ))
}

fn size_estimated(s: &Js, _: &Context) -> Found {
    let one_criterion_chore = is_str(s.get("type"), "chore") && acceptance_criteria(s).len() == 1;
    if is_spike(s) || one_criterion_chore || finite(s.get("size")).is_some() {
        return none();
    }
    err("Not estimated. Run story-estimate.")
}

fn size_small(s: &Js, _: &Context) -> Found {
    let Some(size) = finite(s.get("size")) else {
        return none();
    };
    let shown = s.get("size").string();
    if size >= SIZE_MAX {
        return err(format!(
            "Size {shown} >= {SIZE_MAX}: too big to finish in a sprint. Split it."
        ));
    }
    if size >= SIZE_WARN {
        return warn(format!(
            "Size {shown} is large. Look for a split before committing."
        ));
    }
    none()
}

fn value_scored(s: &Js, _: &Context) -> Found {
    if is_spike(s) {
        return none();
    }
    let missing: Vec<_> = ["value", "time_criticality", "risk_reduction"]
        .into_iter()
        .filter(|k| finite(s.get(k)).is_none())
        .collect();
    if missing.is_empty() {
        return none();
    }
    warn(format!(
        "Missing WSJF inputs: {}. story-pick cannot rank it.",
        missing.join(", ")
    ))
}

fn independent_deps(s: &Js, ctx: &Context) -> Found {
    let Some(stories) = ctx.stories else {
        return none();
    };
    let deps = match s.get("depends_on") {
        Js::Arr(deps) if !deps.is_empty() => deps,
        Js::Str(text) if !text.is_empty() => {
            return Err("s.depends_on.flatMap is not a function".into())
        }
        _ => return none(),
    };
    Ok(deps
        .iter()
        .filter_map(|id| {
            match stories
                .iter()
                .rev()
                .find(|x| same_value_zero(x.get("id"), id))
            {
                None => Some((
                    Severity::Error,
                    format!("Depends on {}, which does not exist.", id.string()),
                )),
                Some(dep) if is_str(dep.get("status"), "done") => None,
                Some(dep) => Some((
                    Severity::Warning,
                    format!(
                        "Depends on {} ({}). Not independent until it is done.",
                        id.string(),
                        dep.get("status").string()
                    ),
                )),
            }
        })
        .collect())
}

fn spike_timebox(s: &Js, _: &Context) -> Found {
    if is_spike(s) && !truthy(s.get("timebox")) {
        return err("Spike has no timebox (e.g. timebox: 2d).");
    }
    none()
}

fn spike_question(s: &Js, _: &Context) -> Found {
    let answered = matches!(section(s, "Question"), Js::Str(q) if !trim(q).is_empty());
    if is_spike(s) && !answered {
        return err(r###"Spike has no "## Question" to answer."###);
    }
    none()
}

fn spike_unblocks(s: &Js, ctx: &Context) -> Found {
    let closed = ["done", "split", "archived"]
        .iter()
        .any(|c| is_str(s.get("status"), c));
    let Some(stories) = ctx.stories.filter(|_| is_spike(s) && !closed) else {
        return none();
    };
    let id = s.get("id");
    for x in stories {
        let listed = match x.get("depends_on") {
            Js::Undefined => false,
            Js::Arr(deps) => deps.iter().any(|d| same_value_zero(d, id)),
            Js::Str(deps) => deps.contains(&id.string()),
            _ => return Err("x.depends_on?.includes is not a function".into()),
        };
        if listed {
            return none();
        }
    }
    err(format!(
        "No story lists {} in depends_on. A spike must unblock a named story: link it, or archive the spike.",
        id.string()
    ))
}

fn state_machine_exists(s: &Js, ctx: &Context) -> Found {
    let machine = s.get("state_machine");
    let Some(root) = ctx.root.filter(|_| truthy(machine)) else {
        return none();
    };
    let path = match machine {
        Js::Str(path) => path,
        other => {
            let received = match other {
                Js::Num(n) if *n == 0.0 && n.is_sign_negative() => "type number (-0)".into(),
                Js::Num(_) => format!("type number ({})", other.string()),
                Js::Bool(b) => format!("type boolean ({b})"),
                Js::Arr(_) => "an instance of Array".into(),
                _ => "an instance of Object".into(),
            };
            return Err(format!(
                r#"The "paths[1]" argument must be of type string. Received {received}"#
            ));
        }
    };
    if config::resolve(&root.join(path)).is_ok_and(|p| p.exists()) {
        return none();
    }
    err(format!(
        "state_machine: {path} does not exist (paths are relative to the repo root). Write it with the state-machine skill."
    ))
}

/// `story lint [id...] [--dir D] [--config C] [--json]`: exit 1 when any story is not ready.
pub fn run(args: &[String]) -> Result<(String, u8), Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(&flags, &loaded.root, &settings)?;
    let all = list::stories(&dir, None)?;
    let targets = if flags.positional.is_empty() {
        all.clone()
    } else {
        show::require_all(&dir, &flags.positional)?
    };
    let ctx = Context {
        stories: Some(&all),
        root: Some(&loaded.root),
    };
    let mut results = Vec::new();
    for story in &targets {
        results.push((story, lint_story(story, &ctx)?));
    }
    let code = u8::from(results.iter().any(|(_, f)| !ready(f)));
    let output = if flags.has("json") {
        Js::Arr(results.iter().map(|(s, f)| json(s, f)).collect()).json()
    } else if results.is_empty() {
        "No stories.".into()
    } else {
        results
            .iter()
            .map(|(s, f)| human(s, f))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    Ok((format!("{output}\n"), code))
}

fn json(story: &Js, findings: &[Finding]) -> Js {
    let items = findings
        .iter()
        .map(|f| {
            Js::Obj(vec![
                ("rule".into(), Js::Str(f.rule.into())),
                ("invest".into(), Js::Str(f.invest.into())),
                ("severity".into(), Js::Str(f.severity.name().into())),
                ("message".into(), Js::Str(f.message.clone())),
            ])
        })
        .collect();
    Js::Obj(vec![
        ("id".into(), story.get("id").clone()),
        ("findings".into(), Js::Arr(items)),
        ("ready".into(), Js::Bool(ready(findings))),
        ("title".into(), story.get("title").clone()),
    ])
}

/// `formatLint`.
fn human(story: &Js, findings: &[Finding]) -> String {
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let verdict = if ready(findings) {
        "READY"
    } else {
        "NOT READY"
    };
    let head = format!(
        "{}  {}  {verdict} ({errors} errors, {} warnings)",
        story.get("id").string(),
        story.get("title").string(),
        findings.len() - errors
    );
    std::iter::once(head)
        .chain(findings.iter().map(|f| {
            format!(
                "  {:<7} [{}] {:<20} {}",
                f.severity.name(),
                f.invest,
                f.rule,
                f.message
            )
        }))
        .collect::<Vec<_>>()
        .join("\n")
}
