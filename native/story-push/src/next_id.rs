//! Node's `FileSystemStoryRepository.nextId` with `gitReservedIds`: one above the highest number
//! for `prefix` among the backlog files and the backlog folder on every local and remote-tracking
//! branch, so two branches never hand out the same id.

use crate::{lenient_story::Js, valid_id};
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

/// The next free id for `prefix` in `dir`. Node interpolates the prefix into a regular
/// expression; this matches it literally, which agrees for every prefix without regex syntax.
pub(crate) fn next_id(dir: &Path, prefix: &str) -> Result<String, String> {
    let mut ids = reserved_ids(dir)?;
    ids.extend(local_ids(dir)?);
    let highest = ids
        .iter()
        .filter_map(|id| number(id, prefix))
        .fold(0.0, f64::max);
    Ok(format_id(prefix, highest + 1.0))
}

/// `formatId`: the number zero-padded to three digits.
pub(crate) fn format_id(prefix: &str, n: f64) -> String {
    format!("{prefix}-{:0>3}", Js::Num(n).string())
}

fn number(id: &str, prefix: &str) -> Option<f64> {
    let digits = id.strip_prefix(prefix)?.strip_prefix('-')?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().unwrap())
}

fn local_ids(dir: &Path) -> Result<Vec<String>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut ids = Vec::new();
    for entry in entries {
        let name = entry
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .file_name();
        if let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".md")) {
            if valid_id(id) {
                ids.push(id.to_owned());
            }
        }
    }
    Ok(ids)
}

fn git(dir: &Path, args: &[&str]) -> std::io::Result<Output> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

/// `gitReservedIds`: nothing outside a git repository (or without git); otherwise the ids in
/// the backlog folder of every branch, as of the last fetch.
fn reserved_ids(dir: &Path) -> Result<Vec<String>, String> {
    if !git(dir, &["rev-parse", "--show-toplevel"]).is_ok_and(|o| o.status.success()) {
        return Ok(Vec::new());
    }
    const LIST: [&str; 4] = [
        "for-each-ref",
        "--format=%(refname)",
        "refs/heads",
        "refs/remotes",
    ];
    let refs = git(dir, &LIST).map_err(|e| e.to_string())?;
    if !refs.status.success() {
        return Err(format!(
            "Command failed: git -C {} {}\n{}",
            dir.display(),
            LIST.join(" "),
            String::from_utf8_lossy(&refs.stderr)
        ));
    }
    let mut ids = Vec::new();
    for reference in String::from_utf8_lossy(&refs.stdout)
        .split('\n')
        .filter(|r| !r.is_empty() && !r.ends_with("/HEAD"))
    {
        let Ok(names) = git(dir, &["ls-tree", "--name-only", reference, "--", "."]) else {
            continue;
        };
        if !names.status.success() {
            continue;
        }
        for name in String::from_utf8_lossy(&names.stdout).split('\n') {
            if let Some(id) = name.strip_suffix(".md").filter(|id| valid_id(id)) {
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_owned());
                }
            }
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_pad_to_three_digits_and_grow_past_them() {
        assert_eq!(format_id("ST", 7.0), "ST-007");
        assert_eq!(format_id("PAY", 1234.0), "PAY-1234");
    }
}
