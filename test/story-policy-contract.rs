use std::{fs, path::Path, process::Command, sync::OnceLock};

fn checker() -> &'static Path {
    static BINARY: OnceLock<std::path::PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let binary =
                std::env::temp_dir().join(format!("story-policy-contract-{}", std::process::id()));
            let output = Command::new("rustc")
                .args([
                    "--edition=2021",
                    "-Dwarnings",
                    "scripts/story-policy.rs",
                    "-o",
                ])
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            binary
        })
        .as_path()
}

fn link(body: &str, incident: bool) -> std::process::Output {
    Command::new(checker())
        .arg("story-link")
        .env("REPO", "Team-Teddy-Development/story-time")
        .env("PR_BODY", body)
        .env("IS_INCIDENT", incident.to_string())
        .output()
        .unwrap()
}

#[test]
fn repository_has_github_dogfooding_contract() {
    for path in [
        ".story-time/settings.json",
        ".story-time/.gitignore",
        ".github/workflows/story-link.yml",
        ".github/workflows/story-ids.yml",
        ".github/story-mandate-ruleset.json",
        "scripts/story-policy.rs",
        "AGENTS.md",
    ] {
        assert!(
            Path::new(path).is_file(),
            "missing dogfooding artifact: {}",
            path
        );
    }
    let ignore = fs::read_to_string(".story-time/.gitignore").unwrap();
    assert!(ignore.lines().any(|line| line == "*.env"));
}

#[test]
fn real_cli_accepts_the_bootstrap_story() {
    let result = link("Stories: ST-001", false);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("PASS:"));
}

#[test]
fn real_cli_rejects_missing_and_undeclared_stories() {
    for body in ["Related ST-001", "Stories: ST-999999999999"] {
        let result = link(body, false);
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).starts_with("FAIL:"));
    }
}

#[test]
fn real_cli_incident_exception_does_not_hide_invalid_references() {
    assert!(link("", true).status.success());
    assert_eq!(
        link("Stories: ST-999999999999", true).status.code(),
        Some(1)
    );
}

struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Policy test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Policy test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
fn fixture(name: &str) -> Fixture {
    let path = std::env::temp_dir().join(format!("story-policy-{}-{name}", std::process::id()));
    fs::create_dir(&path).unwrap();
    git(&path, &["init", "-q"]);
    git(&path, &["commit", "-q", "--allow-empty", "-m", "Initial"]);
    Fixture(path)
}
fn ids(dir: &Path) -> std::process::Output {
    let head = git(dir, &["rev-parse", "HEAD"]);
    Command::new(checker())
        .arg("story-ids")
        .current_dir(dir)
        .env("REPO", "Team-Teddy-Development/story-time")
        .env("BASE", "main")
        .env("HEAD_SHA", head)
        .env("PR", "1")
        .output()
        .unwrap()
}
#[test]
fn real_cli_accepts_no_new_story_ids_without_a_github_request() {
    let fixture = fixture("no-new-ids");
    git(
        &fixture.0,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    let result = ids(&fixture.0);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("no new story IDs"));
}
#[test]
fn real_cli_rejects_a_story_added_independently_to_the_base_branch() {
    let fixture = fixture("base-collision");
    git(&fixture.0, &["checkout", "-q", "-b", "feature"]);
    fs::create_dir(fixture.0.join("backlog")).unwrap();
    fs::write(fixture.0.join("backlog/ST-001.md"), "feature story").unwrap();
    git(&fixture.0, &["add", "backlog/ST-001.md"]);
    git(&fixture.0, &["commit", "-q", "-m", "Feature story"]);
    git(&fixture.0, &["checkout", "-q", "-b", "base", "HEAD~1"]);
    fs::create_dir(fixture.0.join("backlog")).unwrap();
    fs::write(fixture.0.join("backlog/ST-001.md"), "different base story").unwrap();
    git(&fixture.0, &["add", "backlog/ST-001.md"]);
    git(&fixture.0, &["commit", "-q", "-m", "Base story"]);
    git(
        &fixture.0,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    git(&fixture.0, &["checkout", "-q", "feature"]);
    let result = ids(&fixture.0);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("ST-001 already exists on the base branch"));
}

#[test]
fn story_cli_reads_github_configuration() {
    let result = Command::new("node")
        .args(["skills/story-time/scripts/story.mjs", "config", "--json"])
        .env_remove("STORY_TIME_CONFIG")
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("\"tracker\": \"github\""));
    assert!(text.contains("\"sync\": \"ask\""));
    assert!(text.contains("\"repo\": \"Team-Teddy-Development/story-time\""));
}
