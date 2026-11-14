use std::{fs, path::PathBuf};

const ORIGINAL: &str = "The Rust dispatcher passes only non-push commands to the retained Node CLI. Configure its location through `STORY_TIME_LEGACY_CLI` or the `legacyCli` field in `~/.config/story-time/runtime.json`.";
const HEADING: &str = "## Superseded in part (2026-11-14)";
const NOTE: &str = "Since ST-096 (PR #206), the native `story` binary runs every command and never starts the Node CLI. It reads neither `STORY_TIME_LEGACY_CLI` nor `~/.config/story-time/runtime.json`.";

fn flowed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn adr_0003_marks_the_node_fallback_as_removed_and_keeps_the_original_decision() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/adr/0003-native-story-push.md");
    let raw = fs::read_to_string(&path).unwrap();
    let text = flowed(&raw);
    assert!(
        text.contains(ORIGINAL),
        "original decision text is gone: {ORIGINAL}"
    );
    let heading = raw
        .lines()
        .position(|line| line == HEADING)
        .unwrap_or_else(|| panic!("missing dated superseding note: {HEADING}"));
    let decision = raw.lines().position(|line| line == "## Decision").unwrap();
    assert!(
        heading < decision,
        "the superseding note must come before the decision it supersedes"
    );
    assert!(text.contains(NOTE), "superseding note does not say: {NOTE}");
}
