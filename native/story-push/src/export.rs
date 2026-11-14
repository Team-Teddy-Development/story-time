//! `story export <id> [--format github|ado|gitea|markdown] [--command] [--json]`: the card push
//! renders, printed for copying, or the `gh`/`az` command that creates it.

use crate::{
    config::{self, js_string, truthy},
    show,
    story::{ado_args, render_card},
    Failure,
};
use serde_json::Value;

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
    let (settings, story) = show::read(&flags)?;
    let story: Value = serde_json::from_str(&story.json()).map_err(|e| e.to_string())?;
    let format = flags
        .value("format")
        .unwrap_or(match settings["tracker"].as_str() {
            Some(tracker @ ("github" | "ado" | "gitea")) => tracker,
            _ => "markdown",
        });
    if flags.has("command") {
        return Ok(format!("{}\n", command(&story, format, &settings)?));
    }
    let card = render_card(&story, format, &settings)?;
    Ok(if flags.has("json") {
        format!(
            "{}\n",
            serde_json::to_string_pretty(&card).map_err(|e| e.to_string())?
        )
    } else {
        format!("{}\n", js_string(&card["text"]))
    })
}

/// Node's `renderCommand`.
fn command(story: &Value, format: &str, settings: &Value) -> Result<String, String> {
    let title = quote(&js_string(&story["title"]));
    match format {
        "github" => {
            let repo = &settings["github"]["repo"];
            if !truthy(Some(repo)) {
                return Err("Set github.repo in .story-time/settings.json (run the story-setup skill) to print a gh command.".into());
            }
            let card = render_card(story, "github", settings)?;
            let labels = card["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|label| format!("--label {}", arg(&js_string(label))))
                .collect::<Vec<_>>()
                .join(" ");
            Ok(format!(
                "gh issue create --repo {} --title {title} {labels} --body-file - <<'STORY_TIME_EOF'\n{}\nSTORY_TIME_EOF",
                arg(&js_string(repo)),
                js_string(&card["body"])
            ))
        }
        "ado" => {
            let ado = &settings["ado"];
            if !truthy(ado.get("organization")) {
                return Err("Set ado.organization and ado.project in .story-time/settings.json (run the story-setup skill) to print an az command.".into());
            }
            let a = ado_args(&render_card(story, "ado", settings)?)?;
            let mut lines = vec![format!(
                "az boards work-item create --org {} --project {} --type {} --title {title}",
                quote(&js_string(&ado["organization"])),
                quote(&js_string(&ado["project"])),
                quote(&a.kind),
            )];
            for (flag, value) in [
                ("--description", a.description),
                ("--area", a.area),
                ("--iteration", a.iteration),
            ] {
                if let Some(value) = value.filter(|v| !v.is_empty()) {
                    lines.push(format!("{flag} {}", quote(&value)));
                }
            }
            let fields: Vec<String> = a.fields.iter().map(|field| quote(field)).collect();
            if !fields.is_empty() {
                lines.push(format!("--fields {}", fields.join(" ")));
            }
            Ok(lines.join(" \\\n  "))
        }
        _ => Err(format!(
            "Format \"{format}\" has no create command. Use github or ado."
        )),
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Quotes only when the shell needs it, so simple arguments stay readable.
fn arg(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.:/@-".contains(c))
    {
        value.to_owned()
    } else {
        quote(value)
    }
}
