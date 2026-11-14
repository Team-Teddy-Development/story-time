use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-setup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn binary() -> PathBuf {
    std::env::var_os("STORY_TIME_TEST_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_story").into())
}
fn skill() -> PathBuf {
    std::env::var_os("STORY_TIME_TEST_SETUP_SKILL")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("skills/story-setup/SKILL.md"))
}

#[test]
fn setup_help_is_native_read_only_and_independent_of_configuration() {
    for args in [["init", "--help"], ["init", "-h"]] {
        let sandbox = Sandbox::new();
        let result = Command::new(binary())
            .args(args)
            .current_dir(&sandbox.0)
            .env_clear()
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        let text = String::from_utf8(result.stdout).unwrap();
        for expected in [
            "Usage: story init",
            "--from",
            "--force",
            "github.repo",
            "owner/repo",
        ] {
            assert!(text.contains(expected), "missing {expected} in {text}");
        }
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 0);
    }
}

fn github_example() -> Value {
    let text = fs::read_to_string(skill()).unwrap();
    let examples: Vec<Value> = text
        .split("```json\n")
        .skip(1)
        .filter_map(|part| serde_json::from_str(part.split("```").next().unwrap()).ok())
        .collect();
    examples
        .into_iter()
        .find(|value| value["tracker"] == "github")
        .expect("story-setup must include a runnable GitHub JSON example")
}

#[test]
fn github_example_round_trips_through_the_real_cli() {
    let config = github_example();
    assert_eq!(config["github"]["repo"], "acme/shop");
    assert!(config["github"].get("owner").is_none());
    let sandbox = Sandbox::new();
    let answers = sandbox.0.join("answers.json");
    fs::write(&answers, config.to_string()).unwrap();
    let init = Command::new(binary())
        .args(["init", "--from"])
        .arg(&answers)
        .current_dir(&sandbox.0)
        .env_remove("STORY_TIME_CONFIG")
        .env_remove("STORY_TIME_DIR")
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let show = Command::new(binary())
        .args(["config", "--json"])
        .current_dir(&sandbox.0)
        .env_remove("STORY_TIME_CONFIG")
        .env_remove("STORY_TIME_DIR")
        .output()
        .unwrap();
    assert!(show.status.success());
    let actual: Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(actual["github"], config["github"]);
    assert_eq!(actual["tracker"], "github");
    assert_eq!(actual["sync"], "off");
    let ignored = fs::read_to_string(sandbox.0.join(".story-time/.gitignore")).unwrap();
    assert!(ignored.lines().any(|line| line == "*.env"));
}

#[test]
fn github_example_bare_repository_mutant_is_rejected_without_writes() {
    let mut config = github_example();
    config["github"]["repo"] = "shop".into();
    config["github"]["owner"] = "acme".into();
    let sandbox = Sandbox::new();
    let answers = sandbox.0.join("answers.json");
    fs::write(&answers, config.to_string()).unwrap();
    let result = Command::new(binary())
        .args(["init", "--from"])
        .arg(&answers)
        .current_dir(&sandbox.0)
        .env_remove("STORY_TIME_CONFIG")
        .env_remove("STORY_TIME_DIR")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("github.repo must be owner/repo"));
    assert!(!sandbox.0.join(".story-time").exists());
}
