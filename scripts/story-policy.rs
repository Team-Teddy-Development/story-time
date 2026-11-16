use std::{collections::BTreeSet, env, process::Command};

type Check = Result<(), String>;

fn story_id(value: &str) -> bool {
    value
        .strip_prefix("ST-")
        .is_some_and(|digits| digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn story_file(path: &str) -> Option<String> {
    let id = path.strip_prefix("backlog/")?.strip_suffix(".md")?;
    story_id(id).then(|| id.to_string())
}

fn unique_ids(
    added: &BTreeSet<String>,
    existing: &BTreeSet<String>,
    older: &[(u64, BTreeSet<String>)],
) -> Check {
    for id in added {
        if existing.contains(id) {
            return Err(format!("{id} already exists on the base branch"));
        }
        for (number, ids) in older {
            if ids.contains(id) {
                return Err(format!("{id} is also added by older open PR #{number}"));
            }
        }
    }
    Ok(())
}

fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let result = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Could not run {program}: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "{program} failed with {}: {}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    String::from_utf8(result.stdout).map_err(|e| format!("{program} returned invalid UTF-8: {e}"))
}

fn required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("Missing {name}"))
}

fn repository() -> Result<String, String> {
    let repo = required("REPO")?;
    let parts: Vec<_> = repo.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
    {
        return Err("REPO must be owner/repository".into());
    }
    Ok(repo)
}

fn ids() -> Check {
    let repo = repository()?;
    let base = format!("origin/{}", required("BASE")?);
    let head = required("HEAD_SHA")?;
    if head.len() != 40 || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("HEAD_SHA must be a 40-character Git commit SHA".into());
    }
    let pr: u64 = required("PR")?.parse().map_err(|_| "PR must be a number")?;
    let merge_base = output("git", &["merge-base", &base, &head])?;
    let changed = output(
        "git",
        &[
            "diff",
            "--name-only",
            "--diff-filter=A",
            merge_base.trim(),
            &head,
            "--",
            "backlog/",
        ],
    )?;
    let added: BTreeSet<_> = changed.lines().filter_map(story_file).collect();
    if added.is_empty() {
        println!("PASS: no new story IDs.");
        return Ok(());
    }
    let files = output(
        "git",
        &["ls-tree", "-r", "--name-only", &base, "--", "backlog/"],
    )?;
    let existing = files.lines().filter_map(story_file).collect();
    unique_ids(&added, &existing, &[])?;
    let endpoint = format!("repos/{repo}/pulls?state=open&per_page=100");
    let pulls = output(
        "gh",
        &["api", "--paginate", &endpoint, "--jq", ".[].number"],
    )?;
    let mut older = Vec::new();
    for line in pulls.lines() {
        let number: u64 = line
            .parse()
            .map_err(|_| "GitHub returned an invalid PR number")?;
        if number >= pr {
            continue;
        }
        let endpoint = format!("repos/{repo}/pulls/{number}/files?per_page=100");
        let paths = output(
            "gh",
            &[
                "api",
                "--paginate",
                &endpoint,
                "--jq",
                ".[] | select(.status == \"added\") | .filename",
            ],
        )?;
        older.push((number, paths.lines().filter_map(story_file).collect()));
    }
    unique_ids(&added, &existing, &older)?;
    println!("PASS: {} new story IDs are unique.", added.len());
    Ok(())
}

fn main() {
    let result = match env::args().nth(1).as_deref() {
        Some("story-ids") => ids(),
        _ => Err("Usage: story-policy story-ids".into()),
    };
    if let Err(message) = result {
        eprintln!("FAIL: {}", message.replace(['\r', '\n'], " "));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn nested_story_paths_are_not_story_files() {
        assert_eq!(story_file("backlog/nested/ST-001.md"), None);
    }
    #[test]
    fn base_branch_collision_is_rejected() {
        assert!(unique_ids(&set(&["ST-001"]), &set(&["ST-001"]), &[]).is_err());
    }
    #[test]
    fn older_pull_request_collision_is_rejected() {
        assert!(unique_ids(&set(&["ST-001"]), &set(&[]), &[(2, set(&["ST-001"]))]).is_err());
    }
    #[test]
    fn distinct_ids_are_accepted() {
        assert!(unique_ids(
            &set(&["ST-002"]),
            &set(&["ST-001"]),
            &[(2, set(&["ST-003"]))]
        )
        .is_ok());
    }
}
