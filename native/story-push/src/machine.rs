//! `story machine check|criteria|diagram <file>`: an entity lifecycle as data
//! (model/state/<entity>.toml), checked for defects, with its acceptance criteria and Mermaid
//! diagram. Ported from the Node CLI and `src/domain/state-machine.mjs`; a model's values keep
//! their JavaScript meaning, so a field the author left out is `None` (JavaScript `undefined`).

use crate::{
    config::{self, js_string, js_value, truthy, Flags},
    js_number::stringify,
    models::{model_name, out, read_model, report_defects, Defect},
    Failure,
};
use serde_json::{json, Map, Value};

const USAGE: &str = "Usage: story machine check|criteria|diagram <file>";

struct Transition {
    from: Option<Value>,
    to: Option<Value>,
    event: Option<Value>,
    guard: Option<Value>,
}

struct Machine {
    entity: Value,
    states: Vec<String>,
    initial: String,
    terminal: Vec<String>,
    events: Vec<String>,
    guards: Map<String, Value>,
    transitions: Vec<Transition>,
}

pub fn run(args: &[String]) -> Result<(String, u8), Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let (action, file) = match flags.positional.as_slice() {
        [action, file, ..]
            if ["check", "criteria", "diagram"].contains(&action.as_str()) && !file.is_empty() =>
        {
            (action.as_str(), file.as_str())
        }
        _ => {
            return Err(Failure {
                code: 2,
                message: USAGE.into(),
            })
        }
    };
    let json = flags.has("json");
    let name = model_name(file);
    let (machine, defects) = check(&read_model(file)?, &name);
    let machine = match machine {
        Some(machine) if defects.is_empty() && action != "check" => machine,
        machine => {
            let entity = machine.as_ref().map_or(json!(name), |m| m.entity.clone());
            return Ok(report_defects(json, &entity, &defects, || {
                let m = machine.expect("a machine without defects");
                format!(
                    "{} states, {} events, {} transitions",
                    m.states.len(),
                    m.events.len(),
                    m.transitions.len()
                )
            }));
        }
    };
    if action == "diagram" {
        let diagram = diagram(&machine);
        return Ok((
            out(json, &json!({ "diagram": diagram }), || diagram.clone()),
            0,
        ));
    }
    let criteria = criteria(&machine);
    let human = || {
        criteria
            .iter()
            .map(|c| format!("- [ ] {}", c["text"].as_str().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok((out(json, &criteria, human), 0))
}

/// `${value}` in a JavaScript template literal.
fn text(value: &Option<Value>) -> String {
    value.as_ref().map_or("undefined".into(), js_string)
}

fn is_in(names: &[String], value: &Option<Value>) -> bool {
    matches!(value, Some(Value::String(s)) if names.contains(s))
}

fn name_list(value: Option<&Value>, allow_empty_names: bool) -> Option<Vec<String>> {
    value?
        .as_array()?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| allow_empty_names || !s.is_empty())
                .map(String::from)
        })
        .collect()
}

fn defect(code: &'static str, message: String) -> Defect {
    Defect {
        code,
        message,
        extra: Map::new(),
    }
}

/// Normalizes a parsed document into a machine and lists every defect, in the Node checker's order.
/// Criteria and diagrams are only meaningful when there are no defects.
fn check(doc: &Value, name: &str) -> (Option<Machine>, Vec<Defect>) {
    let mut defects = Vec::new();
    let field = |key| doc.get(key);
    let states = name_list(field("states"), false).filter(|s| !s.is_empty());
    let initial = field("initial")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let events = name_list(field("events"), false).filter(|s| !s.is_empty());
    let transitions = field("transitions")
        .and_then(Value::as_array)
        .filter(|t| !t.is_empty());
    let terminal = field("terminal").map(|t| name_list(Some(t), true));
    let guards = field("guards").map(Value::as_object);
    if states.is_none() {
        defects.push(defect(
            "schema",
            "states must be a non-empty list of names.".into(),
        ));
    }
    if initial.is_none() {
        defects.push(defect("schema", "initial must name a state.".into()));
    }
    if events.is_none() {
        defects.push(defect(
            "schema",
            "events must be a non-empty list of names.".into(),
        ));
    }
    if transitions.is_none() {
        defects.push(defect(
            "schema",
            "transitions must be one or more [[transitions]] tables.".into(),
        ));
    }
    if terminal.as_ref().is_some_and(Option::is_none) {
        defects.push(defect(
            "schema",
            "terminal must be a list of state names.".into(),
        ));
    }
    if guards.is_some_and(|g| g.is_none()) {
        defects.push(defect(
            "schema",
            "guards must be a [guards] table of name = \"description\".".into(),
        ));
    }
    let (Some(states), Some(initial), Some(events), Some(transitions)) =
        (states, initial, events, transitions)
    else {
        return (None, defects);
    };
    if !defects.is_empty() {
        return (None, defects);
    }

    let get = |t: &Value, key: &str| t.as_object().and_then(|o| o.get(key)).cloned();
    let machine = Machine {
        entity: field("entity")
            .or(field("name"))
            .cloned()
            .unwrap_or_else(|| json!(name)),
        states,
        initial: initial.to_owned(),
        terminal: terminal.flatten().unwrap_or_default(),
        events,
        guards: guards.flatten().cloned().unwrap_or_default(),
        transitions: transitions
            .iter()
            .map(|t| Transition {
                from: get(t, "from"),
                to: get(t, "to"),
                event: get(t, "event"),
                guard: get(t, "guard"),
            })
            .collect(),
    };
    let m = &machine;

    if !m.states.contains(&m.initial) {
        defects.push(defect(
            "unknown-state",
            format!("Initial state \"{}\" is not in states.", m.initial),
        ));
    }
    for s in &m.terminal {
        if !m.states.contains(s) {
            defects.push(defect(
                "unknown-state",
                format!("Terminal state \"{s}\" is not in states."),
            ));
        }
    }

    for (i, t) in m.transitions.iter().enumerate() {
        let n = i + 1;
        if !is_in(&m.states, &t.from) {
            defects.push(defect(
                "unknown-state",
                format!(
                    "Transition {n} comes from unknown state \"{}\".",
                    text(&t.from)
                ),
            ));
        }
        if !is_in(&m.states, &t.to) {
            defects.push(defect(
                "unknown-state",
                format!("Transition {n} goes to unknown state \"{}\".", text(&t.to)),
            ));
        }
        if !truthy(t.event.as_ref()) {
            defects.push(defect(
                "missing-event",
                format!(
                    "Transition {n} ({} -> {}) has no event.",
                    text(&t.from),
                    text(&t.to)
                ),
            ));
        } else if !is_in(&m.events, &t.event) {
            defects.push(defect(
                "unknown-event",
                format!("Transition {n} uses unknown event \"{}\".", text(&t.event)),
            ));
        }
        if let Some(guard) = &t.guard {
            if !m.guards.contains_key(&js_string(guard)) {
                defects.push(defect(
                    "unknown-guard",
                    format!(
                        "Transition {n} uses unknown guard \"{}\".",
                        js_string(guard)
                    ),
                ));
            }
        }
        if is_in(&m.terminal, &t.from) && truthy(t.event.as_ref()) {
            defects.push(defect(
                "terminal-exit",
                format!(
                    "Terminal state \"{}\" has an outgoing transition on \"{}\".",
                    text(&t.from),
                    text(&t.event)
                ),
            ));
        }
    }

    // Grouped by JSON.stringify([from, event, guard ?? null]), in first-seen order, and named by
    // JSON.parse of that key, so Infinity reads back as null.
    let mut groups: Vec<(String, usize)> = Vec::new();
    for t in m.transitions.iter().filter(|t| truthy(t.event.as_ref())) {
        let parts = [&t.from, &t.event, &t.guard].map(|v| v.as_ref().unwrap_or(&Value::Null));
        let key = stringify(&parts, false);
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(group) => group.1 += 1,
            None => groups.push((key, 1)),
        }
    }
    for (key, count) in groups.into_iter().filter(|g| g.1 > 1) {
        let [from, event, _]: [Value; 3] =
            serde_json::from_str(&key).expect("a group key is a JSON triple");
        defects.push(defect(
            "ambiguous",
            format!(
                "State \"{}\" has {count} transitions on \"{}\" with the same guard.",
                js_string(&from),
                js_string(&event)
            ),
        ));
    }

    let mut reached: Vec<&String> = m
        .states
        .iter()
        .filter(|s| **s == m.initial)
        .take(1)
        .collect();
    let mut frontier = reached.clone();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for t in &m.transitions {
            let (Some(Value::String(from)), Some(Value::String(to))) = (&t.from, &t.to) else {
                continue;
            };
            if frontier.contains(&from) && m.states.contains(to) && !reached.contains(&to) {
                reached.push(to);
                next.push(to);
            }
        }
        frontier = next;
    }
    for s in &m.states {
        if !reached.is_empty() && !reached.contains(&s) {
            defects.push(defect(
                "unreachable",
                format!(
                    "State \"{s}\" cannot be reached from the initial state \"{}\".",
                    m.initial
                ),
            ));
        }
        if !m.terminal.contains(s)
            && !m
                .transitions
                .iter()
                .any(|t| matches!(&t.from, Some(Value::String(f)) if f == s))
        {
            defects.push(defect(
                "dead-end",
                format!("State \"{s}\" is not terminal and has no outgoing transition."),
            ));
        }
    }

    (Some(machine), defects)
}

/// One criterion per transition, one per (state, event) whose every transition is guarded (the
/// case where no guard holds), and one per event a state refuses.
fn criteria(m: &Machine) -> Vec<Value> {
    let e = js_string(&m.entity);
    let article = if e.starts_with(|c: char| "aeiouAEIOU".contains(c)) {
        "an"
    } else {
        "a"
    };
    let a = format!("{article} {e}");
    let description = |g: &Value| m.guards.get(&js_string(g)).filter(|d| truthy(Some(d)));
    let guard_text = |g: &Value| {
        description(g).map_or_else(|| format!("guard {} holds", js_string(g)), js_string)
    };
    let refuse =
        |state: &str, event: &str| format!("then {event} is refused and the {e} stays in {state}.");

    let mut allowed = Vec::new();
    for t in &m.transitions {
        let (from, event, to) = (text(&t.from), text(&t.event), text(&t.to));
        let guard = t.guard.as_ref().filter(|g| truthy(Some(g)));
        let mut c = Map::new();
        c.insert("kind".into(), json!("transition"));
        c.insert("from".into(), json!(from));
        c.insert("event".into(), json!(event));
        c.insert("to".into(), json!(to));
        if let Some(g) = guard {
            c.insert("guard".into(), js_value(g));
        }
        let and = guard.map_or(String::new(), |g| format!(" and {}", guard_text(g)));
        c.insert(
            "text".into(),
            json!(format!(
                "Given {a} in {from}{and}, when {event} is received, then the {e} is in {to}."
            )),
        );
        allowed.push(Value::Object(c));
    }

    let mut guarded = Vec::new();
    let mut refused = Vec::new();
    for state in &m.states {
        for event in &m.events {
            let ts: Vec<&Transition> = m
                .transitions
                .iter()
                .filter(|t| {
                    matches!(&t.from, Some(Value::String(f)) if f == state)
                        && matches!(&t.event, Some(Value::String(v)) if v == event)
                })
                .collect();
            if ts.is_empty() {
                refused.push(json!({
                    "kind": "refused",
                    "from": state,
                    "event": event,
                    "text": format!("Given {a} in {state}, when {event} is received, {}", refuse(state, event)),
                }));
            } else if ts.iter().all(|t| truthy(t.guard.as_ref())) {
                let guards: Vec<&Value> = ts.iter().filter_map(|t| t.guard.as_ref()).collect();
                let fails = guards
                    .iter()
                    .map(|g| {
                        let why = description(g)
                            .map_or(String::new(), |d| format!(" ({})", js_string(d)));
                        format!("guard {} fails{why}", js_string(g))
                    })
                    .collect::<Vec<_>>()
                    .join(" and ");
                guarded.push(json!({
                    "kind": "guard",
                    "from": state,
                    "event": event,
                    "guards": guards.iter().map(|g| js_value(g)).collect::<Vec<_>>(),
                    "text": format!("Given {a} in {state} and {fails}, when {event} is received, {}", refuse(state, event)),
                }));
            }
        }
    }
    allowed.into_iter().chain(guarded).chain(refused).collect()
}

fn diagram(m: &Machine) -> String {
    let mut lines = vec![
        "stateDiagram-v2".to_owned(),
        format!("    [*] --> {}", m.initial),
    ];
    for t in &m.transitions {
        let guard = t
            .guard
            .as_ref()
            .filter(|g| truthy(Some(g)))
            .map_or(String::new(), |g| format!(" [{}]", js_string(g)));
        lines.push(format!(
            "    {} --> {}: {}{guard}",
            text(&t.from),
            text(&t.to),
            text(&t.event)
        ));
    }
    lines.extend(m.terminal.iter().map(|s| format!("    {s} --> [*]")));
    lines.join("\n")
}
