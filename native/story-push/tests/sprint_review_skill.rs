use std::{fs, path::PathBuf};

fn skill(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills")
        .join(name)
        .join("SKILL.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn position(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("missing instruction: {needle}"))
}

fn table_rows<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
    let start = position(text, header);
    text[start..]
        .lines()
        .skip(2)
        .take_while(|line| line.starts_with('|'))
        .collect()
}

#[test]
fn sprint_review_records_the_goal_outcome() {
    let text = skill("sprint-review");
    position(&text, "Record the goal outcome");
    let rows = table_rows(&text, "| Sprint goal | Outcome | Evidence |");
    assert!(!rows.is_empty(), "the goal outcome table has a row");
    for outcome in ["`met`", "`partly met`", "`not met`", "`unknown`"] {
        position(&text, outcome);
    }
}

#[test]
fn sprint_review_links_each_piece_of_feedback_to_a_backlog_item() {
    let text = skill("sprint-review");
    position(&text, "Link every piece of feedback to a backlog item");
    let rows = table_rows(&text, "| Feedback | From | Backlog item | Handed to |");
    for owner in ["**story-create**", "**story-edit**"] {
        assert!(
            rows.iter().any(|row| row.contains(owner)),
            "feedback is handed to {owner}"
        );
    }
}

#[test]
fn sprint_review_gives_every_unfinished_story_an_explicit_disposition() {
    let text = skill("sprint-review");
    position(
        &text,
        "Every unfinished story gets one explicit disposition",
    );
    let rows = table_rows(&text, "| Story | Disposition | Handed to | Reason |");
    for (disposition, owner) in [
        ("`carry over`", "**sprint-planning**"),
        ("`return to backlog`", "**refinement**"),
        ("`split`", "**story-split**"),
        ("`drop`", "**story-edit**"),
    ] {
        assert!(
            rows.iter()
                .any(|row| row.contains(disposition) && row.contains(owner)),
            "a {disposition} row hands off to {owner}"
        );
    }
}

#[test]
fn sprint_review_preserves_original_estimates_and_sprint_membership() {
    let text = skill("sprint-review");
    position(&text, "Never change an original estimate");
    position(&text, "Never rewrite sprint membership");
}

#[test]
fn sprint_review_marks_no_story_done() {
    let text = skill("sprint-review");
    position(&text, "Never mark a story done");
    position(&text, "Completing a sprint never completes its stories");
    position(&text, "**story-accept** skill");
    for line in text.lines().map(str::trim_start) {
        for write in ["story set", "story push", "story new", "story note"] {
            assert!(
                !line.starts_with(write),
                "sprint-review runs no write command: {line}"
            );
        }
    }
}

#[test]
fn sprint_review_reports_missing_sprint_data_as_unknown() {
    let text = skill("sprint-review");
    position(&text, "No sprint record is stored");
    position(&text, "Report a missing value as `unknown`");
}

#[test]
fn the_router_sends_sprint_review_requests_to_sprint_review() {
    assert!(skill("story-time").contains("**sprint-review** skill"));
}
