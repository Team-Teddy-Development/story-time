//! The `Stories:` line of a pull request body, and GitHub closing keywords.
//! Shared by `story check-pr` and `story sync-merge`.

/// One reference unit: `ST-123`, `#45`, or `ST-123 (#45)`, optionally `(partial)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reference {
    pub(crate) id: Option<String>,
    pub(crate) issue: Option<u64>,
    pub(crate) partial: bool,
}

impl Reference {
    /// How a finding names this reference, for example `ST-1 (#5)`.
    pub(crate) fn label(&self) -> String {
        match (&self.id, self.issue) {
            (Some(id), Some(n)) => format!("{id} (#{n})"),
            (Some(id), None) => id.clone(),
            (None, Some(n)) => format!("#{n}"),
            (None, None) => String::new(),
        }
    }
}

/// `<prefix>-` followed by 3 or more digits, the grammar of `scripts/story-policy.rs`.
fn reference_id(word: &str, prefix: &str) -> bool {
    word.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|digits| digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn issue_number(word: &str) -> Option<u64> {
    word.strip_prefix('#')
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|digits| digits.parse().ok())
        .filter(|n| *n > 0)
}

fn trim_punctuation(word: &str) -> &str {
    word.trim_end_matches(['.', ';', ':', '!', '?'])
}

/// The references on every line whose label before `:` is `story` or `stories`, in any case.
pub(crate) fn parse(body: &str, prefix: &str) -> Vec<Reference> {
    let mut units = Vec::new();
    for line in body.lines() {
        let Some((label, rest)) = line.trim_start().split_once(':') else {
            continue;
        };
        let label = label.trim();
        if !label.eq_ignore_ascii_case("story") && !label.eq_ignore_ascii_case("stories") {
            continue;
        }
        for segment in rest.split(',') {
            // A `(#N)` joins the ID before it only when nothing else came between them.
            let mut joinable = false;
            for raw in segment.split_whitespace() {
                let word = trim_punctuation(raw);
                if reference_id(word, prefix) {
                    units.push(Reference {
                        id: Some(word.to_string()),
                        issue: None,
                        partial: false,
                    });
                    joinable = true;
                } else if let Some(n) = word
                    .strip_prefix('(')
                    .and_then(|w| w.strip_suffix(')'))
                    .and_then(issue_number)
                {
                    match units.last_mut() {
                        Some(unit) if joinable && unit.issue.is_none() => unit.issue = Some(n),
                        _ => units.push(Reference {
                            id: None,
                            issue: Some(n),
                            partial: false,
                        }),
                    }
                    joinable = false;
                } else if let Some(n) = issue_number(word) {
                    units.push(Reference {
                        id: None,
                        issue: Some(n),
                        partial: false,
                    });
                    joinable = false;
                } else if word.eq_ignore_ascii_case("(partial)") {
                    if let Some(unit) = units.last_mut() {
                        unit.partial = true;
                    }
                } else {
                    joinable = false;
                }
            }
        }
    }
    units
}

const KEYWORDS: [&str; 9] = [
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
];

/// The issue number a closing-keyword target names in `repo`, or `None` for another repository.
/// An empty `repo` accepts only the bare `#N` form.
fn target(word: &str, repo: &str) -> Option<u64> {
    let word = word.trim_end_matches(['.', ',', ';', ')', '!', '?']);
    if let Some(n) = issue_number(word) {
        return Some(n);
    }
    let same = |other: &str| !repo.is_empty() && other.eq_ignore_ascii_case(repo);
    if let Some(rest) = word
        .strip_prefix("https://github.com/")
        .or_else(|| word.strip_prefix("http://github.com/"))
    {
        let (other, number) = rest.split_once("/issues/")?;
        return same(other)
            .then(|| number.parse().ok())
            .flatten()
            .filter(|n| *n > 0);
    }
    let (other, number) = word.split_once('#')?;
    if other.contains('/') && same(other) {
        return issue_number(&format!("#{number}"));
    }
    None
}

/// Issue numbers in `repo` that a GitHub closing keyword targets: `#N`, `owner/repo#N`, or an
/// issue URL, after `close`, `fix`, or `resolve` in any tense with an optional colon.
pub(crate) fn closing_targets(body: &str, repo: &str) -> Vec<u64> {
    let mut found = Vec::new();
    let words: Vec<&str> = body.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let lower = word.to_ascii_lowercase();
        let keyword = lower.strip_suffix(':').unwrap_or(&lower);
        if !KEYWORDS.contains(&keyword) {
            continue;
        }
        if let Some(n) = words.get(i + 1).and_then(|next| target(next, repo)) {
            if !found.contains(&n) {
                found.push(n);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: Option<&str>, issue: Option<u64>, partial: bool) -> Reference {
        Reference {
            id: id.map(str::to_string),
            issue,
            partial,
        }
    }

    #[test]
    fn an_id_with_its_issue_and_partial_is_one_unit() {
        assert_eq!(
            parse("Stories: ST-904 (#905) (partial)", "ST"),
            vec![unit(Some("ST-904"), Some(905), true)]
        );
    }

    #[test]
    fn a_bare_issue_is_an_issue_only_unit() {
        assert_eq!(
            parse("Stories: #344", "ST"),
            vec![unit(None, Some(344), false)]
        );
    }

    #[test]
    fn comma_separated_references_are_separate_units() {
        assert_eq!(
            parse("Stories: ST-001 (#12), ST-002 (#13)", "ST"),
            vec![
                unit(Some("ST-001"), Some(12), false),
                unit(Some("ST-002"), Some(13), false)
            ]
        );
    }

    #[test]
    fn a_lowercase_label_is_accepted() {
        assert_eq!(
            parse("stories: ST-001", "ST"),
            vec![unit(Some("ST-001"), None, false)]
        );
    }

    #[test]
    fn malformed_and_path_traversal_ids_and_issue_zero_are_not_references() {
        assert!(parse("Stories: ST-01 ST-001x ST-../001 #0", "ST").is_empty());
    }

    #[test]
    fn a_body_without_a_stories_line_has_no_references() {
        assert!(parse("Related ST-001\nCloses #4", "ST").is_empty());
    }

    #[test]
    fn the_configured_prefix_decides_what_an_id_is() {
        assert_eq!(
            parse("Stories: XY-001 ST-001", "XY"),
            vec![unit(Some("XY-001"), None, false)]
        );
    }

    #[test]
    fn every_keyword_tense_with_and_without_a_colon_is_a_closing_keyword() {
        for keyword in KEYWORDS {
            for form in [format!("{keyword} #7"), format!("{keyword}: #7")] {
                assert_eq!(closing_targets(&form, "owner/repo"), vec![7], "{form}");
            }
        }
        assert_eq!(closing_targets("Fixes #7.", "owner/repo"), vec![7]);
    }

    #[test]
    fn qualified_and_url_targets_count_only_in_this_repository() {
        let repo = "owner/repo";
        assert_eq!(closing_targets("Closes: owner/repo#5", repo), vec![5]);
        assert_eq!(
            closing_targets("resolves https://github.com/owner/repo/issues/6", repo),
            vec![6]
        );
        assert!(closing_targets("Closes owner/other#5", repo).is_empty());
        assert!(closing_targets("Fixes https://github.com/owner/other/issues/6", repo).is_empty());
        assert!(closing_targets("This closes nothing", repo).is_empty());
    }
}
