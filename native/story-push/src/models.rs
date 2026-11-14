//! File and output handling shared by the checked design models (`story machine`, `story table`),
//! as the Node CLI's `readModel`, `modelName`, `reportDefects`, and `out` do it.

use crate::{config, toml_subset, Failure};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::io::ErrorKind;

#[derive(Serialize)]
pub(crate) struct Defect {
    pub code: &'static str,
    pub message: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The file name without its directory and `.toml` suffix, like `basename(file).replace(/\.toml$/, '')`.
pub(crate) fn model_name(file: &str) -> String {
    let trimmed = file.trim_end_matches('/');
    let base = trimmed.rsplit('/').next().unwrap_or_default();
    base.strip_suffix(".toml").unwrap_or(base).to_owned()
}

pub(crate) fn read_model(file: &str) -> Result<Value, Failure> {
    let bytes = std::fs::read(file).map_err(|e| match e.kind() {
        ErrorKind::NotFound => format!("{file}: not found."),
        ErrorKind::IsADirectory => {
            format!("{file}: EISDIR: illegal operation on a directory, read")
        }
        ErrorKind::NotADirectory => format!("{file}: ENOTDIR: not a directory, open '{file}'"),
        ErrorKind::PermissionDenied => format!("{file}: EACCES: permission denied, open '{file}'"),
        _ => format!("{file}: {e}"),
    })?;
    toml_subset::parse(&String::from_utf8_lossy(&bytes))
        .map_err(|e| Failure::from(format!("{file}: {e}")))
}

/// `console.log(flags.json ? JSON.stringify(data, null, 2) : human)`.
pub(crate) fn out(json: bool, data: &impl Serialize, human: impl FnOnce() -> String) -> String {
    let text = if json {
        serde_json::to_string_pretty(data).expect("model output serializes")
    } else {
        human()
    };
    format!("{text}\n")
}

/// The check report, and exit code 1 when there are defects.
pub(crate) fn report_defects(
    json: bool,
    name: &Value,
    defects: &[Defect],
    summary: impl FnOnce() -> String,
) -> (String, u8) {
    let output = out(
        json,
        &json!({"ok": defects.is_empty(), "defects": defects}),
        || {
            let name = config::js_string(name);
            if defects.is_empty() {
                return format!("{name}: OK ({})", summary());
            }
            let plural = if defects.len() == 1 { "" } else { "s" };
            std::iter::once(format!("{name}: {} defect{plural}", defects.len()))
                .chain(
                    defects
                        .iter()
                        .map(|d| format!("  {:<14} {}", d.code, d.message)),
                )
                .collect::<Vec<_>>()
                .join("\n")
        },
    );
    (output, u8::from(!defects.is_empty()))
}
