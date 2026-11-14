use std::{fs, path::PathBuf};

const HEADING: &str = "## Install the native prerequisite";
const CLAIM: &str = "The native `story` binary runs every command, including `story config` and `story plan`, and no `story` command requires Node.";

fn install_section() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/planning/README.md");
    let raw = fs::read_to_string(&path).unwrap();
    let start = raw
        .find(HEADING)
        .unwrap_or_else(|| panic!("missing section: {HEADING}"));
    let body = &raw[start + HEADING.len()..];
    let end = body.find("\n## ").unwrap_or(body.len());
    body[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn planning_install_section_matches_the_native_only_binary() {
    let section = install_section();
    assert!(
        !section.contains("STORY_TIME_LEGACY_CLI"),
        "install section still tells users to set STORY_TIME_LEGACY_CLI"
    );
    assert!(
        !section.contains("Node remains necessary"),
        "install section still says Node is needed for retained commands"
    );
    assert!(
        section.contains(CLAIM),
        "install section does not say: {CLAIM}"
    );
}
