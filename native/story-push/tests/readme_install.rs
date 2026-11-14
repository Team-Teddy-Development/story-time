use std::{fs, path::PathBuf};

const RUST_INSTALLER: &str =
    "cargo run --locked --manifest-path native/story-push/Cargo.toml --bin story-install-skills --";

fn section(heading: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    let raw = fs::read_to_string(&path).unwrap();
    let start = raw
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("missing section: {heading}"));
    let body = &raw[start + heading.len() + 2..];
    let end = body.find("\n### ").unwrap_or(body.len());
    body[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn installer_section_runs_the_rust_installer() {
    let installer = section("### Installer (Claude Code, Codex, or both)");
    assert!(
        !installer.contains("install.mjs"),
        "Installer section still names scripts/install.mjs"
    );
    for agent in ["codex", "claude", "all"] {
        let command = format!("{RUST_INSTALLER} --agent {agent}");
        assert!(
            installer.contains(&command),
            "Installer section does not show: {command}"
        );
    }
    assert!(
        installer.contains("| `--source DIR` |"),
        "Installer flags table does not list --source DIR"
    );
}

#[test]
fn claude_code_section_runs_the_rust_installer() {
    let claude = section("### Claude Code");
    assert!(
        !claude.contains("install.mjs"),
        "Claude Code section still names scripts/install.mjs"
    );
    let command = format!("{RUST_INSTALLER} --agent claude");
    assert!(
        claude.contains(&command),
        "Claude Code section does not show: {command}"
    );
}
