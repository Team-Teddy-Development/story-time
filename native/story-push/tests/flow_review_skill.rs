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
fn flow_review_reports_blocked_aging_and_over_wip_items_with_an_owner_and_next_action() {
    let text = skill("flow-review");
    for source in ["story config --json", "story list --json", "team.wipLimit"] {
        position(&text, source);
    }
    let rows = table_rows(&text, "| Kind | Story | Owner | Next action | Evidence |");
    for kind in ["`blocked`", "`aging`", "`over-wip`"] {
        assert!(
            rows.iter()
                .any(|row| row.starts_with(&format!("| {kind} |"))),
            "the report table has a {kind} row"
        );
    }
    position(&text, "Every row names an owner and a next action");
}

#[test]
fn flow_review_records_each_scope_swap_with_its_capacity_effect() {
    let text = skill("flow-review");
    position(
        &text,
        "| In | Out | Points in | Points out | Capacity effect |",
    );
    position(&text, "Record every scope swap");
}

#[test]
fn flow_review_changes_no_status_or_acceptance_and_routes_pulls_to_story_pick() {
    let text = skill("flow-review");
    position(&text, "Never change a story's status or acceptance");
    position(&text, "Pull decisions go to the **story-pick** skill");
    position(&text, "**story-accept** skill");
    for line in text.lines().map(str::trim_start) {
        for write in ["story set", "story push", "story new", "story note"] {
            assert!(
                !line.starts_with(write),
                "flow-review runs no write command: {line}"
            );
        }
    }
}

#[test]
fn flow_review_reports_missing_sprint_and_flow_data_as_unknown() {
    let text = skill("flow-review");
    for gap in [
        "No sprint record is stored",
        "No flow event history is stored",
        "No aging limit is configured",
        "Report a missing value as `unknown`",
    ] {
        position(&text, gap);
    }
}

#[test]
fn the_router_sends_flow_requests_to_flow_review() {
    assert!(skill("story-time").contains("**flow-review** skill"));
}
