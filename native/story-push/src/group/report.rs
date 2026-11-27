//! Renders `story group` placements for people and as JSON. No decisions are made here.

use super::classifier::Source;
use super::placement::{round2, Outcome, Placement};
use super::questions::Pick;
use super::Engine;
use serde_json::{json, Value};

fn engine_label(source: &Option<Source>) -> String {
    match source {
        Some(Source::Jev { model }) => model.clone(),
        Some(Source::Code { .. }) => "code keywords".into(),
        None => "none".into(),
    }
}

fn names(p: &Placement) -> Option<String> {
    let epic = p.epic.as_ref()?.name.as_deref()?;
    let feature = p.feature.as_ref()?.name.as_deref()?;
    Some(format!("{epic} / {feature}"))
}

fn confidences(p: &Placement) -> String {
    let c = |pick: &Option<Pick>| {
        pick.as_ref()
            .map(|x| round2(x.confidence).to_string())
            .unwrap_or("-".into())
    };
    format!(
        "epic {}, feature {}, {}",
        c(&p.epic),
        c(&p.feature),
        engine_label(&p.source)
    )
}

pub(crate) fn human(
    engine: &Engine,
    placements: &[&Placement],
    written: &[String],
    applied: bool,
) -> String {
    let mut out = match engine {
        Engine::Jev(model) => format!("engine: jev ({model})\n"),
        Engine::Code(why) => format!("engine: code ({why})\n"),
    };
    let (mut assigned, mut unchanged, mut unresolved, mut kept) = (0, 0, 0, 0);
    for p in placements {
        let from = p.current_epic.as_deref().unwrap_or("(no epic)");
        out.push_str(&format!("\n{}  {}\n", p.id, p.title));
        match &p.outcome {
            Outcome::Assigned => {
                assigned += 1;
                out.push_str(&format!(
                    "  {from} → {}  ({})\n",
                    names(p).unwrap_or_default(),
                    confidences(p)
                ));
            }
            Outcome::Unchanged => {
                unchanged += 1;
                out.push_str(&format!(
                    "  unchanged: {}  ({})\n",
                    names(p).unwrap_or_default(),
                    confidences(p)
                ));
            }
            Outcome::Kept => {
                kept += 1;
                out.push_str(&format!(
                    "  kept: {} / {}\n",
                    from,
                    p.current_feature.as_deref().unwrap_or("")
                ));
            }
            Outcome::Unresolved(reason) => {
                unresolved += 1;
                out.push_str(&format!("  {from} → unresolved: {reason}"));
                if let Some(suggestion) = names(p) {
                    out.push_str(&format!("; suggests {suggestion}  ({})", confidences(p)));
                }
                out.push('\n');
            }
        }
    }
    out.push_str(&format!(
        "\n{assigned} assigned, {unchanged} unchanged, {unresolved} unresolved, {kept} kept. "
    ));
    if !applied {
        out.push_str("Dry run: rerun with --apply to write.\n");
    } else if written.is_empty() {
        out.push_str("Nothing to write.\n");
    } else {
        out.push_str(&format!(
            "Wrote {}; publish with story push.\n",
            written.join(", ")
        ));
    }
    out
}

fn pick(p: &Option<Pick>) -> Value {
    match p {
        Some(Pick { name, confidence }) => json!({"name": name, "confidence": round2(*confidence)}),
        None => Value::Null,
    }
}

pub(crate) fn json(engine: &Engine, placements: &[&Placement], written: &[String]) -> Value {
    let engine = match engine {
        Engine::Jev(model) => {
            json!({"mode": "jev", "model": model, "why": "TypeSafe key configured"})
        }
        Engine::Code(why) => json!({"mode": "code", "why": why}),
    };
    let count = |o: &str| {
        placements
            .iter()
            .filter(|p| outcome(&p.outcome).0 == o)
            .count()
    };
    let summary = json!({
        "assigned": count("assigned"),
        "unchanged": count("unchanged"),
        "unresolved": count("unresolved"),
        "kept": count("kept"),
        "written": written,
    });
    let placements: Vec<Value> = placements
        .iter()
        .map(|p| {
            let (name, reason) = outcome(&p.outcome);
            let mut v = json!({
                "id": p.id,
                "title": p.title,
                "from": {"epic": p.current_epic, "feature": p.current_feature},
                "epic": pick(&p.epic),
                "feature": pick(&p.feature),
                "source": match &p.source {
                    Some(Source::Jev { model }) => json!({"mode": "jev", "model": model}),
                    Some(Source::Code { why }) => json!({"mode": "code", "why": why}),
                    None => Value::Null,
                },
                "outcome": name,
            });
            if let Some(reason) = reason {
                v["reason"] = json!(reason);
            }
            v
        })
        .collect();
    json!({"engine": engine, "placements": placements, "summary": summary})
}

fn outcome(o: &Outcome) -> (&'static str, Option<&str>) {
    match o {
        Outcome::Assigned => ("assigned", None),
        Outcome::Unchanged => ("unchanged", None),
        Outcome::Unresolved(r) => ("unresolved", Some(r.as_str())),
        Outcome::Kept => ("kept", None),
    }
}

/// The `--sync-fields` report: one line per field.
pub(crate) fn sync(org: &str, changes: &[crate::github_fields::Change]) -> String {
    use crate::github_fields::Change;
    let mut out = format!("{org} issue fields:\n");
    for change in changes {
        out.push_str(&match change {
            Change::Created { field, options } => {
                format!("  {field}: created with {}\n", options.join(", "))
            }
            Change::Extended { field, added } => format!("  {field}: added {}\n", added.join(", ")),
            Change::Unchanged { field } => format!("  {field}: unchanged\n"),
        });
    }
    out
}
