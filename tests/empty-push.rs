use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(sync: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-empty-push-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join(".story-time")).unwrap();
        fs::create_dir(path.join("backlog")).unwrap();
        fs::write(path.join(".story-time/settings.json"), format!(r#"{{"tracker":"gitea","sync":"{sync}","gitea":{{"baseUrl":"http://127.0.0.1:1","owner":"test","repo":"empty"}}}}"#)).unwrap();
        Self(path)
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new("node")
            .arg(
                std::env::current_dir()
                    .unwrap()
                    .join("skills/story-time/scripts/story.mjs"),
            )
            .args(args)
            .current_dir(&self.0)
            .env_remove("GITEA_TOKEN")
            .env_remove("STORY_TIME_CONFIG")
            .env_remove("STORY_TIME_DIR")
            .env_remove("NODE_OPTIONS")
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn empty_all_dry_run_returns_empty_plan_without_credentials_or_writes() {
    let fixture = Fixture::new("ask");
    let settings = fs::read(fixture.0.join(".story-time/settings.json")).unwrap();
    let result = fixture.run(&["push", "--all", "--dry-run", "--json"]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let compact: String = String::from_utf8(result.stdout)
        .unwrap()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(compact, r#"{"ok":true,"plan":[]}"#);
    assert_eq!(
        fs::read(fixture.0.join(".story-time/settings.json")).unwrap(),
        settings
    );
    assert_eq!(fs::read_dir(fixture.0.join("backlog")).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(fixture.0.join(".story-time")).unwrap().count(),
        1
    );
}

#[test]
fn missing_target_remains_a_usage_error() {
    let fixture = Fixture::new("ask");
    let result = fixture.run(&["push", "--dry-run", "--json"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("Usage: story push"));
}

#[test]
fn disabled_sync_remains_rejected_for_empty_all_dry_run() {
    let fixture = Fixture::new("off");
    let result = fixture.run(&["push", "--all", "--dry-run", "--json"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("sync"));
}
