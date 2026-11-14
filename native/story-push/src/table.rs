//! `story table check|criteria <file>`: a business rule as a decision table
//! (model/rules/<rule>.toml) with finite input domains, condition rows, and a hit policy, checked
//! for gaps and overlaps, with one parameterized acceptance criterion. Ported from the Node CLI
//! and `src/domain/decision-table.mjs`; values keep their JavaScript meaning, so a `when` or `then`
//! that is a string or array exposes its indices as keys, as `Object.hasOwn` and `Object.keys` do.

use crate::{
    config::{self, js_value, Flags},
    js_number::{model_string, model_stringify},
    models::{model_name, out, read_model, report_defects, Defect},
    Failure,
};
use serde_json::{json, Map, Value};

const USAGE: &str = "Usage: story table check|criteria <file>";
const ANY: &str = "-";
const MAX_COMBINATIONS: u128 = 4096;

struct Rule {
    when: Value,
    then: Value,
}

struct Table {
    name: Value,
    hit_policy: Option<Value>,
    inputs: Vec<(String, Value)>,
    outputs: Vec<Value>,
    rules: Vec<Rule>,
}

pub fn run(args: &[String]) -> Result<(String, u8), Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let (action, file) = match flags.positional.as_slice() {
        [action, file, ..]
            if ["check", "criteria"].contains(&action.as_str()) && !file.is_empty() =>
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
    let (table, defects) = check(&js_value(&read_model(file)?), &model_name(file));
    if !defects.is_empty() || action == "check" {
        return Ok(report_defects(json, &table.name, &defects, || {
            format!(
                "{} inputs, {} combinations, {} rules, hit policy {}",
                table.inputs.len(),
                combination_count(&table.inputs),
                table.rules.len(),
                table
                    .hit_policy
                    .as_ref()
                    .map_or_else(String::new, model_string)
            )
        }));
    }
    let c = criteria(&table);
    Ok((
        out(json, &c, || {
            c["markdown"].as_str().unwrap_or_default().to_owned()
        }),
        0,
    ))
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `Object.hasOwn(value, key) ? value[key] : undefined`.
fn own(value: &Value, key: &str) -> Option<Value> {
    match value {
        Value::Object(o) => o.get(key).cloned(),
        Value::Array(a) if key == "length" => Some(json!(a.len())),
        Value::Array(a) => a.get(config::array_index(key)? as usize).cloned(),
        Value::String(s) if key == "length" => Some(json!(utf16(s).len())),
        Value::String(s) => {
            let unit = *utf16(s).get(config::array_index(key)? as usize)?;
            Some(json!(String::from_utf16_lossy(&[unit])))
        }
        _ => None,
    }
}

/// `Object.keys(value)`.
fn own_keys(value: &Value) -> Vec<String> {
    let indices = |n: usize| (0..n).map(|i| i.to_string()).collect();
    match value {
        Value::Object(o) => o.keys().cloned().collect(),
        Value::Array(a) => indices(a.len()),
        Value::String(s) => indices(utf16(s).len()),
        _ => Vec::new(),
    }
}

/// `===` (and `Array.prototype.includes`): objects and arrays never equal another parsed value.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::String(_), Value::String(_)) | (Value::Bool(_), Value::Bool(_)) => a == b,
        _ => false,
    }
}

fn is_any(value: &Value) -> bool {
    value.as_str() == Some(ANY)
}

fn is_scalar(value: &Value) -> bool {
    matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_))
}

fn stringify(value: &Value) -> String {
    model_stringify(value, false)
}

fn defect(code: &'static str, message: String, extra: Map<String, Value>) -> Defect {
    Defect {
        code,
        message,
        extra,
    }
}

fn combination_count(inputs: &[(String, Value)]) -> u128 {
    inputs.iter().fold(1, |p, (_, domain)| {
        p.saturating_mul(domain.as_array().map_or(0, Vec::len) as u128)
    })
}

/// Every combination of input values, the first input varying slowest.
fn combinations(inputs: &[(String, Value)]) -> Vec<Vec<Value>> {
    inputs.iter().fold(vec![Vec::new()], |combos, (_, domain)| {
        let values = domain.as_array().map_or(&[][..], Vec::as_slice);
        combos
            .iter()
            .flat_map(|prefix| {
                values.iter().map(move |v| {
                    let mut combo = prefix.clone();
                    combo.push(v.clone());
                    combo
                })
            })
            .collect()
    })
}

fn show(table: &Table, combo: &[Value]) -> String {
    table
        .inputs
        .iter()
        .zip(combo)
        .map(|((name, _), v)| format!("{name}={}", model_string(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn combo_object(table: &Table, combo: &[Value]) -> Value {
    let entries = table.inputs.iter().zip(combo);
    Value::Object(entries.map(|((n, _), v)| (n.clone(), v.clone())).collect())
}

fn matching(table: &Table, combo: &[Value]) -> Vec<usize> {
    (0..table.rules.len())
        .filter(|&i| {
            table.inputs.iter().zip(combo).all(|((name, _), v)| {
                own(&table.rules[i].when, name).is_some_and(|w| is_any(&w) || same(&w, v))
            })
        })
        .collect()
}

/// Normalizes a parsed (JavaScript-ordered) document into a table and lists every defect, in the
/// Node checker's order. Criteria are only meaningful when there are no defects.
fn check(doc: &Value, name: &str) -> (Table, Vec<Defect>) {
    let mut defects = Vec::new();
    let mut schema = |message: String| defects.push(defect("schema", message, Map::new()));
    let hit_policy = doc.get("hit_policy").cloned();
    if !hit_policy
        .as_ref()
        .is_some_and(|p| matches!(p.as_str(), Some("unique" | "first")))
    {
        schema(format!(
            "hit_policy must be \"unique\" or \"first\", got {}.",
            stringify(hit_policy.as_ref().unwrap_or(&Value::Null))
        ));
    }
    let empty = Map::new();
    let input_doc = doc
        .get("inputs")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let inputs: Vec<(String, Value)> = input_doc
        .iter()
        .map(|(n, d)| (n.clone(), d.clone()))
        .collect();
    if inputs.is_empty() {
        schema("inputs must be an [inputs] table of name = [values].".into());
    }
    for (input, domain) in &inputs {
        match domain.as_array() {
            Some(d) if !d.is_empty() && d.iter().all(is_scalar) => {
                if d.iter()
                    .enumerate()
                    .any(|(i, v)| d[..i].iter().any(|u| same(u, v)))
                {
                    schema(format!("Input \"{input}\" lists a value twice."));
                }
            }
            _ => schema(format!(
                "Input \"{input}\" needs a non-empty list of values."
            )),
        }
    }
    let outputs = doc
        .get("outputs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if outputs.is_empty() || !outputs.iter().all(Value::is_string) {
        schema("outputs must be a non-empty list of names.".into());
    }
    let rule_docs = doc
        .get("rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rule_docs.is_empty() {
        schema("rules must be one or more [[rules]] tables.".into());
    }

    let rules: Vec<Rule> = rule_docs
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            let n = idx + 1;
            let when = own(r, "when").unwrap_or_else(|| json!({}));
            let then = own(r, "then").unwrap_or_else(|| json!({}));
            for (input, domain) in &inputs {
                match own(&when, input) {
                    None => schema(format!(
                        "Rule {n}: no condition for input \"{input}\"; write \"-\" for any value."
                    )),
                    Some(value) => {
                        if let Some(d) = domain.as_array().filter(|_| !is_any(&value)) {
                            if !d.iter().any(|v| same(v, &value)) {
                                let listed =
                                    d.iter().map(model_string).collect::<Vec<_>>().join(", ");
                                schema(format!(
                                    "Rule {n}: {input} = {} is not in its domain ({listed}).",
                                    stringify(&value)
                                ));
                            }
                        }
                    }
                }
            }
            for k in own_keys(&when) {
                if !input_doc.contains_key(&k) {
                    schema(format!("Rule {n}: unknown input \"{k}\"."));
                }
            }
            for o in &outputs {
                if own(&then, &model_string(o)).is_none() {
                    schema(format!(
                        "Rule {n}: no value for output \"{}\".",
                        model_string(o)
                    ));
                }
            }
            for k in own_keys(&then) {
                if !outputs.iter().any(|o| same(o, &json!(k))) {
                    schema(format!("Rule {n}: unknown output \"{k}\"."));
                }
            }
            Rule { when, then }
        })
        .collect();

    let table = Table {
        name: doc.get("name").cloned().unwrap_or_else(|| json!(name)),
        hit_policy,
        inputs,
        outputs,
        rules,
    };
    if !defects.is_empty() {
        return (table, defects);
    }

    let total = combination_count(&table.inputs);
    if total > MAX_COMBINATIONS {
        let message =
            format!("{total} input combinations is more than {MAX_COMBINATIONS}. Split the table.");
        return (table, vec![defect("schema", message, Map::new())]);
    }

    let unique = table.hit_policy.as_ref().and_then(Value::as_str) == Some("unique");
    let mut fires = vec![false; table.rules.len()];
    let mut overlaps: Vec<((usize, usize), Vec<Value>)> = Vec::new();
    for combo in combinations(&table.inputs) {
        let hits = matching(&table, &combo);
        match hits.first() {
            None => defects.push(defect(
                "uncovered",
                format!("No rule covers {}.", show(&table, &combo)),
                Map::from_iter([("combination".into(), combo_object(&table, &combo))]),
            )),
            Some(&first) => fires[first] = true,
        }
        if unique {
            for (a, &x) in hits.iter().enumerate() {
                for &y in &hits[a + 1..] {
                    if !overlaps.iter().any(|(pair, _)| *pair == (x, y)) {
                        overlaps.push(((x, y), combo.clone()));
                    }
                }
            }
        }
    }
    for ((x, y), combo) in overlaps {
        let (a, b) = (x + 1, y + 1);
        defects.push(defect(
            "overlap",
            format!(
                "Rules {a} and {b} both match {} (hit policy unique).",
                show(&table, &combo)
            ),
            Map::from_iter([("rules".into(), json!([a, b]))]),
        ));
    }
    if !unique {
        for (i, _) in fires.iter().enumerate().filter(|(_, f)| !**f) {
            let n = i + 1;
            defects.push(defect(
                "unreachable",
                format!("Rule {n} never fires: earlier rules match every combination it covers (hit policy first)."),
                Map::from_iter([("rules".into(), json!([n]))]),
            ));
        }
    }
    (table, defects)
}

/// One Given/When/Then with a row per input combination and the rule that decides it.
fn criteria(table: &Table) -> Value {
    let criterion = format!(
        "Given the inputs in a row of the {} table, when the rule is evaluated, then the result is the outputs in that row.",
        model_string(&table.name)
    );
    let columns: Vec<Value> = std::iter::once(json!("rule"))
        .chain(table.inputs.iter().map(|(n, _)| json!(n)))
        .chain(table.outputs.iter().cloned())
        .collect();
    let rows: Vec<Value> = combinations(&table.inputs)
        .into_iter()
        .map(|combo| {
            let r = matching(table, &combo)[0];
            let then = &table.rules[r].then;
            std::iter::once(json!(r + 1))
                .chain(combo)
                .chain(
                    table
                        .outputs
                        .iter()
                        .map(|o| own(then, &model_string(o)).unwrap_or(Value::Null)),
                )
                .collect()
        })
        .collect();
    let row = |cells: &[Value]| {
        let cells: Vec<String> = cells.iter().map(model_string).collect();
        format!("  | {} |", cells.join(" | "))
    };
    let markdown = [
        format!("- [ ] {criterion}"),
        String::new(),
        row(&columns),
        format!("  |{}", "---|".repeat(columns.len())),
    ]
    .into_iter()
    .chain(
        rows.iter()
            .map(|r| row(r.as_array().expect("a row is an array"))),
    )
    .collect::<Vec<_>>()
    .join("\n");
    json!({ "criterion": criterion, "columns": columns, "rows": rows, "markdown": markdown })
}
