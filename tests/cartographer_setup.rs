use std::path::PathBuf;
use std::process::{Command, Output};

fn repository() -> PathBuf {
    let path = std::env::current_dir().expect("repository working directory");
    assert!(path.join("skills/story-time/scripts/story.mjs").is_file());
    path
}

fn story(args: &[&str]) -> Output {
    Command::new("node")
        .current_dir(repository())
        .arg("skills/story-time/scripts/story.mjs")
        .args(args)
        .output()
        .expect("run repository story CLI")
}

#[test]
fn onboarding_story_has_estimate_and_ranking_inputs() {
    let result = story(&["lint", "ST-002"]);
    let stdout = String::from_utf8(result.stdout).expect("UTF-8 CLI output");
    let stderr = String::from_utf8(result.stderr).expect("UTF-8 CLI errors");
    assert!(result.status.success(), "lint failed: {stdout}{stderr}");
    assert!(
        !stdout.contains("value.scored"),
        "ranking inputs absent: {stdout}"
    );
    assert!(
        !stdout.contains("size.estimated"),
        "estimate absent: {stdout}"
    );
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
}

#[test]
fn actual_cli_reads_repository_configuration() {
    let result = story(&["config", "--json"]);
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let stdout = String::from_utf8(result.stdout).expect("UTF-8 config");
    for expected in [
        "\"tracker\": \"github\"",
        "\"sync\": \"ask\"",
        "\"repo\": \"Team-Teddy-Development/story-time\"",
        "\"prefix\": \"ST\"",
        "\"wipLimit\": 1",
        "\"testCommand\": \"npm test && rustc --edition=2021",
        "scripts/story-policy.rs",
        "test/story-policy-contract.rs",
    ] {
        assert!(stdout.contains(expected), "missing setting {expected}");
    }
}
