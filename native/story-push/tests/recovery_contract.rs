use serde_json::{json, Value};
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
            "story-recovery-{}-{}",
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
fn binary() -> PathBuf {
    env!("CARGO_BIN_EXE_story").into()
}

#[test]
fn new_help_runs_without_legacy_runtime_or_writes() {
    for flag in ["--help", "-h"] {
        let sandbox = Sandbox::new();
        let output = Command::new(binary())
            .args(["new", flag])
            .env_clear()
            .env("PATH", "/nonexistent")
            .current_dir(&sandbox.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "Usage: story new",
            "--title",
            "--statement",
            "--ac",
            "--type",
            "--json",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 0);
    }
}

#[test]
fn new_help_does_not_reach_the_legacy_flag_parser() {
    let sandbox = Sandbox::new();
    let legacy =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs");
    let output = Command::new(binary())
        .args(["new", "--help"])
        .env("STORY_TIME_LEGACY_CLI", legacy)
        .current_dir(&sandbox.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("Usage: story new"));
    assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 0);
}

#[test]
fn create_then_push_uses_the_returned_id_instead_of_guessing() {
    let sandbox = Sandbox::new();
    fs::create_dir(sandbox.0.join(".story-time")).unwrap();
    fs::create_dir(sandbox.0.join("backlog")).unwrap();
    fs::write(
        sandbox.0.join(".story-time/settings.json"),
        json!({
            "tracker":"github", "sync":"ask", "prefix":"REC", "github":{"repo":"acme/shop"}
        })
        .to_string(),
    )
    .unwrap();
    fs::write(sandbox.0.join("backlog/REC-067.md"), "---\nid: REC-067\ntitle: Existing\nstatus: draft\ntype: story\n---\n\n## Story\nExisting\n").unwrap();
    let legacy =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs");
    let created = Command::new(binary())
        .args(["new", "--title", "Build a bridge", "--json"])
        .env("STORY_TIME_LEGACY_CLI", legacy)
        .current_dir(&sandbox.0)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let story: Value = serde_json::from_slice(&created.stdout).unwrap();
    let id = story["id"].as_str().unwrap();
    assert_eq!(id, "REC-068");
    let selected = Command::new(binary())
        .args(["push", id, "--dry-run", "--json"])
        .env_clear()
        .env("PATH", "/nonexistent")
        .current_dir(&sandbox.0)
        .output()
        .unwrap();
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );
    assert!(selected.stderr.is_empty());
    let plan: Value = serde_json::from_slice(&selected.stdout).unwrap();
    assert!(plan.to_string().contains(id), "{plan}");
    assert!(
        !fs::read_to_string(sandbox.0.join(format!("backlog/{id}.md")))
            .unwrap()
            .contains("remote:")
    );
}
