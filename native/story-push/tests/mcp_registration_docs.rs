use std::{fs, path::PathBuf};

fn doc(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    fs::read_to_string(&path)
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const REQUIRED: [&str; 8] = [
    "claude mcp add --transport stdio --scope local story-time -- story mcp --root <repo>",
    "claude mcp add --transport stdio --scope local story-time -- story mcp --root <repo> --write",
    "codex mcp add story-time -- story mcp --root <repo>",
    "codex mcp add story-time -- story mcp --root <repo> --write",
    "[mcp_servers.story-time]",
    "args = [\"mcp\", \"--root\", \"<repo>\", \"--write\"]",
    "default_tools_approval_mode = \"writes\"",
    "Codex asks before a write only when `default_tools_approval_mode = \"writes\"` is set",
];

#[test]
fn readme_gives_mcp_registration_commands_and_codex_writes_setting() {
    let text = doc("README.md");
    for needle in REQUIRED {
        assert!(
            text.contains(needle),
            "README.md does not contain: {needle}"
        );
    }
}

#[test]
fn skill_gives_mcp_registration_commands_and_codex_writes_setting() {
    let text = doc("skills/story-time/SKILL.md");
    for needle in REQUIRED {
        assert!(
            text.contains(needle),
            "skills/story-time/SKILL.md does not contain: {needle}"
        );
    }
}
