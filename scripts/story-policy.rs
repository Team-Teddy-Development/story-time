use std::{collections::BTreeSet, env, path::Path, process::Command};

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

fn references(body: &str) -> (BTreeSet<String>, BTreeSet<u64>) {
    let mut stories = BTreeSet::new();
    let mut issues = BTreeSet::new();
    for line in body.lines() {
        let Some((label, rest)) = line.trim_start().split_once(':') else {
            continue;
        };
        if !label.eq_ignore_ascii_case("story") && !label.eq_ignore_ascii_case("stories") {
            continue;
        }
        for word in
            rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '#' || c == '_'))
        {
            if story_id(word) {
                stories.insert(word.to_string());
            } else if let Some(number) = word.strip_prefix('#').and_then(|s| s.parse::<u64>().ok())
            {
                if number > 0 {
                    issues.insert(number);
                }
            }
        }
    }
    (stories, issues)
}

fn validate_links(
    body: &str,
    incident: bool,
    mut local_exists: impl FnMut(&str) -> bool,
    mut issue_labels: impl FnMut(u64) -> Result<Vec<String>, String>,
) -> Check {
    let (stories, issues) = references(body);
    if stories.is_empty() && issues.is_empty() {
        return if incident {
            Ok(())
        } else {
            Err("Add a Stories: ST-### (#issue) line to the PR body.".into())
        };
    }
    for id in stories {
        if !local_exists(&id) {
            return Err(format!("backlog/{id}.md does not exist"));
        }
    }
    for number in issues {
        let labels = issue_labels(number)?;
        if !labels
            .iter()
            .any(|label| ["story", "bug", "spike", "chore"].contains(&label.as_str()))
        {
            return Err(format!("#{number} is not a story issue"));
        }
    }
    Ok(())
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

fn link() -> Check {
    let repo = repository()?;
    let body = required("PR_BODY")?;
    let incident = env::var("IS_INCIDENT").as_deref() == Ok("true");
    validate_links(
        &body,
        incident,
        |id| Path::new(&format!("backlog/{id}.md")).is_file(),
        |number| {
            let endpoint = format!("repos/{repo}/issues/{number}");
            let labels = output(
                "gh",
                &[
                    "api",
                    &endpoint,
                    "--jq",
                    "if .pull_request then \"PULL_REQUEST\" else .labels[].name end",
                ],
            )?;
            Ok(labels.lines().map(str::to_string).collect())
        },
    )?;
    if incident && references(&body) == (BTreeSet::new(), BTreeSet::new()) {
        println!("PASS: incident exception; file the bug story within one business day.");
    } else {
        println!("PASS: every declared story reference exists.");
    }
    Ok(())
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
        Some("story-link") => link(),
        Some("story-ids") => ids(),
        _ => Err("Usage: story-policy story-link|story-ids".into()),
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
    fn references_only_count_on_declaration_lines() {
        assert_eq!(
            references("Related ST-999 #99\n stories: ST-001 (#12), ST-002\nStory: ST-001"),
            (set(&["ST-001", "ST-002"]), [12].into_iter().collect())
        );
    }
    #[test]
    fn invalid_and_path_traversal_ids_are_not_references() {
        assert_eq!(
            references("Stories: ST-01 ST-001x ../../file #0 #no"),
            (set(&[]), BTreeSet::new())
        );
        assert_eq!(story_file("backlog/nested/ST-001.md"), None);
    }
    #[test]
    fn missing_story_line_is_rejected() {
        assert!(validate_links("Related ST-001", false, |_| true, |_| unreachable!()).is_err());
    }
    #[test]
    fn local_story_is_accepted() {
        assert!(validate_links(
            "Stories: ST-001",
            false,
            |id| id == "ST-001",
            |_| unreachable!()
        )
        .is_ok());
    }
    #[test]
    fn every_local_reference_must_exist() {
        assert!(validate_links(
            "Stories: ST-001 ST-002",
            false,
            |id| id == "ST-001",
            |_| unreachable!()
        )
        .is_err());
    }
    #[test]
    fn story_labeled_issue_is_accepted() {
        for label in ["story", "bug", "spike", "chore"] {
            assert!(
                validate_links("Stories: #12", false, |_| false, |_| Ok(vec![label.into()]))
                    .is_ok()
            );
        }
    }
    #[test]
    fn pull_requests_and_unlabeled_issues_are_rejected() {
        for labels in [vec!["PULL_REQUEST".into()], vec![]] {
            assert!(
                validate_links("Stories: #12", false, |_| false, |_| Ok(labels.clone())).is_err()
            );
        }
    }
    #[test]
    fn api_failure_is_not_treated_as_an_absent_issue() {
        assert_eq!(
            validate_links(
                "Stories: #12",
                false,
                |_| false,
                |_| Err("API unavailable".into())
            ),
            Err("API unavailable".into())
        );
    }
    #[test]
    fn incident_only_exempts_an_absent_reference() {
        assert!(validate_links("", true, |_| false, |_| unreachable!()).is_ok());
        assert!(validate_links("Stories: ST-999", true, |_| false, |_| unreachable!()).is_err());
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
