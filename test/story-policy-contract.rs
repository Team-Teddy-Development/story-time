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
fn story_cli_reads_auto_configuration_without_weakening_verification() {
    let result = Command::new("node")
        .args(["skills/story-time/scripts/story.mjs", "config", "--json"])
        .env_remove("STORY_TIME_CONFIG")
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("\"tracker\": \"github\""));
    assert!(text.contains("\"sync\": \"auto\""));
    for preserved in [
        "\"backlogDir\": \"backlog\"",
        "\"prefix\": \"ST\"",
        "\"sprintDays\": 10",
        "\"wipLimit\": 1",
        "scripts/story-policy.rs",
        "test/story-policy-contract.rs",
        "The PR declares its stories on a Stories: line",
        "Acceptance criteria have test or real CLI evidence",
        "New and replacement executable implementations and tests use Rust",
        "Record acceptance evidence and the exact source commit with the story.",
        "Verify the actual CLI or skill installation path affected by the change.",
        "Keep signing keys outside the agent session; a local project binding does not grant Cartographer admission.",
        "Acceptance criteria describe observable outcomes and the test plan names their verification commands",
        "Name the affected CLI commands or skills and the expected observable result.",
    ] {
        assert!(text.contains(preserved), "missing preserved setting: {preserved}");
    }
}

#[test]
fn setup_defaults_to_explicit_auto_without_changing_existing_configs() {
    let setup = fs::read_to_string("skills/story-setup/SKILL.md").unwrap();
    assert!(setup.contains("`auto` (the default)"));
    assert!(setup.contains("For Jira, Linear, or files, sync is always `off`."));
    assert!(setup.contains("Preserve an existing explicit sync policy"));
    assert!(setup.contains("Write `sync` explicitly"));
    assert!(!setup.contains("`off` (the default)"));
    assert!(!setup.contains("\"sync\": \"ask\""));
    assert!(!setup.contains("\"sync\":\"ask\""));
    let gitea = fs::read_to_string("docs/gitea-story-time.md").unwrap();
    assert!(gitea.contains("\"sync\": \"auto\""));
    assert!(gitea.contains("Missing `sync` means `off`."));
}

#[test]
fn real_init_saves_setup_sync_modes_for_each_tracker() {
    let script = std::env::current_dir()
        .unwrap()
        .join("skills/story-time/scripts/story.mjs");
    for (name, answers, mode) in [
        (
            "github",
            r#"{"tracker":"github","sync":"auto","github":{"repo":"acme/shop"}}"#,
            "auto",
        ),
        (
            "gitea",
            r#"{"tracker":"gitea","sync":"auto","gitea":{"baseUrl":"https://gitea.example.com","owner":"acme","repo":"shop"}}"#,
            "auto",
        ),
        (
            "ado",
            r#"{"tracker":"ado","sync":"auto","ado":{"organization":"https://dev.azure.com/acme","project":"Shop"}}"#,
            "auto",
        ),
        ("files", r#"{"tracker":"files","sync":"off"}"#, "off"),
        ("jira", r#"{"tracker":"jira","sync":"off"}"#, "off"),
        ("linear", r#"{"tracker":"linear","sync":"off"}"#, "off"),
        (
            "explicit-off",
            r#"{"tracker":"github","sync":"off","github":{"repo":"acme/shop"}}"#,
            "off",
        ),
        (
            "explicit-ask",
            r#"{"tracker":"github","sync":"ask","github":{"repo":"acme/shop"}}"#,
            "ask",
        ),
        (
            "legacy-omitted",
            r#"{"tracker":"github","github":{"repo":"acme/shop"}}"#,
            "off",
        ),
    ] {
        let root = Fixture(
            std::env::temp_dir().join(format!("story-setup-{}-{name}", std::process::id())),
        );
        fs::create_dir(&root.0).unwrap();
        let input = root.0.join("answers.json");
        let settings = root.0.join(".story-time/settings.json");
        fs::write(&input, answers).unwrap();
        let init = Command::new("node")
            .arg(&script)
            .args(["init", "--from"])
            .arg(&input)
            .arg("--config")
            .arg(&settings)
            .current_dir(&root.0)
            .env_remove("STORY_TIME_CONFIG")
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        let read = Command::new("node")
            .arg(&script)
            .args(["config", "--json", "--config"])
            .arg(&settings)
            .current_dir(&root.0)
            .env_remove("STORY_TIME_CONFIG")
            .output()
            .unwrap();
        assert!(
            read.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&read.stderr)
        );
        assert!(
            String::from_utf8_lossy(&read.stdout).contains(&format!("\"sync\": \"{mode}\"")),
            "{name}"
        );
        assert!(
            fs::read_to_string(&settings)
                .unwrap()
                .contains(&format!("\"sync\": \"{mode}\"")),
            "{name}"
        );
    }
}

#[test]
fn cartographer_preparation_keeps_distinct_story_ids_and_no_runtime_authority() {
    let bootstrap = fs::read_to_string("backlog/ST-001.md").unwrap();
    assert!(bootstrap.contains("title: Track our own feature work through GitHub stories"));
    let onboarding = fs::read_to_string("backlog/ST-002.md").unwrap();
    assert!(onboarding.contains("id: ST-002\n"));
    assert!(onboarding.contains("Given operator-published admission"));
    let binding = fs::read_to_string("docs/cartographer/project-binding.json").unwrap();
    assert!(binding.contains("\"onboardingStory\": \"ST-002\""));
    for field in [
        "authorizesRuntimeActivation",
        "authorizesDeployment",
        "authorizesPublication",
    ] {
        assert!(binding.contains(&format!("\"{field}\": false")));
    }
    let result = Command::new("node")
        .args([
            "skills/story-time/scripts/story.mjs",
            "show",
            "ST-002",
            "--json",
        ])
        .env_remove("STORY_TIME_CONFIG")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("Manage story-time through Cartographer")
    );
    let result = link("Stories: ST-002", false);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
