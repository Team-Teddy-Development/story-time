const SKILL: &str = include_str!("../../../skills/story-create/SKILL.md");

#[test]
fn checks_shipped_work_before_filing() {
    let (_, rest) = SKILL.split_once("## 2. Check for duplicates").unwrap();
    let (check, _) = rest.split_once("## 3. Draft").unwrap();
    for instruction in [
        "Before running `story new` or calling `story_create`",
        "gh pr list --state merged --search \"$keywords\" --repo \"$tracker_repo\"",
        "Run `story list --json` and compare titles and statements, including `done`\n   and `archived` stories",
        "Search the repository code for the route, screen, or command",
        "Merged PR search: not run",
        "Do not report no evidence",
    ] {
        assert!(check.contains(instruction), "missing: {instruction}");
    }
    let (_, outcomes) = check.split_once("offer these three outcomes:").unwrap();
    let choices: Vec<_> = outcomes
        .split("\n\n")
        .nth(1)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("- **"))
        .map(|line| line.split_once("**").unwrap().0)
        .collect();
    assert_eq!(
        choices,
        ["Skip filing.", "File as `archived`.", "File as `draft`."]
    );
    assert!(outcomes.contains(
        "Record the evidence in Notes. Recommend this outcome\n  when no choice has been made"
    ));
}

#[test]
fn batch_rows_show_evidence_and_outcome_before_writing() {
    let (_, batch) = SKILL
        .split_once("When creating several stories from one source")
        .unwrap();
    let (batch, _) = batch.split_once("## 5. Report").unwrap();
    assert!(batch.contains("evidence links, and proposed outcome in\neach row"));
    assert!(batch.contains("Wait for approval before writing"));
}
