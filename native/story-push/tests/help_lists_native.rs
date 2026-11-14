use std::{path::PathBuf, process::Command};

#[test]
fn story_help_lists_the_native_plan_and_adopt_commands() {
    let legacy =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs");
    let output = Command::new(env!("CARGO_BIN_EXE_story"))
        .arg("help")
        .env("STORY_TIME_LEGACY_CLI", legacy)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let help = String::from_utf8(output.stdout).unwrap();
    for command in [
        "plan   --from",
        "plan   --snapshot",
        "adopt  preview",
        "issues --state open",
        "metrics --from",
    ] {
        let line = help
            .lines()
            .find(|line| line.trim_start().starts_with(command))
            .unwrap_or_else(|| panic!("story help does not list `{command}`:\n{help}"));
        let description = line.trim_start()[command.len()..].trim();
        assert!(!description.is_empty(), "no description for `{command}`");
    }
}
