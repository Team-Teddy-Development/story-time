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

#[test]
fn story_create_adopts_the_source_issue_before_it_syncs() {
    let text = skill("story-create");
    let notes = position(&text, "original title and body under `## Notes`");
    let adopt = position(&text, "story set <id> remote=<n>");
    let sync = position(&text, "Then follow `sync`");
    assert!(
        notes < sync && adopt < sync,
        "remote must be set before the sync step pushes"
    );
}
