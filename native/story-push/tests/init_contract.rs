use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-init-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn command(&self, native: bool) -> Command {
        let binary = std::env::var_os("STORY_TIME_TEST_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_story").into());
        let mut cmd = Command::new(binary);
        cmd.current_dir(&self.0)
            .env_remove("STORY_TIME_CONFIG")
            .env_remove("STORY_TIME_DIR")
            .env_remove("STORY_TIME_LEGACY_CLI")
            .env("HOME", &self.0);
        if native {
            cmd.env("PATH", "/nonexistent");
        } else {
            cmd.env("STORY_TIME_LEGACY_CLI", legacy());
        }
        cmd
    }
    fn init(&self, args: &[&str], native: bool) -> Output {
        self.command(native)
            .arg("init")
            .args(args)
            .output()
            .unwrap()
    }
    fn answers(&self, value: &Value) {
        fs::write(self.0.join("answers.json"), value.to_string()).unwrap();
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn legacy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs")
}
fn success(out: &Output) {
    assert!(
        out.status.success(),
        "exit {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
}
fn settings(out: &Output) -> Value {
    success(out);
    serde_json::from_slice(&out.stdout).unwrap()
}
fn ignored(s: &Sandbox, root: &str) -> String {
    fs::read_to_string(s.0.join(root).join(".story-time/.gitignore")).unwrap()
}

#[test]
fn external_config_reproduction_with_node_available() {
    let s = Sandbox::new();
    s.answers(&json!({"tracker":"github","github":{"repo":"acme/shop"}}));
    let out = s.init(
        &[
            "--from",
            "answers.json",
            "--config",
            "settings.json",
            "--json",
        ],
        false,
    );
    let config = settings(&out);
    assert_eq!(config["github"], json!({"repo":"acme/shop"}));
    assert!(ignored(&s, "").lines().any(|l| l == "*.env"));
    let show = s
        .command(false)
        .args(["config", "--config", "settings.json", "--json"])
        .output()
        .unwrap();
    assert_eq!(settings(&show), config);
}

#[test]
fn canonical_control_with_node_available() {
    let s = Sandbox::new();
    let out = s.init(&["--json"], false);
    assert_eq!(settings(&out)["tracker"], "files");
    assert!(ignored(&s, "").contains("*.env"));
}

#[test]
fn defaults_are_normalized_without_node_or_runtime_config() {
    let s = Sandbox::new();
    fs::write(s.0.join(".story-time.json"), "broken historical config").unwrap();
    let actual = settings(&s.init(&["--json"], true));
    assert_eq!(
        actual,
        json!({"tracker":"files","sync":"off","backlogDir":"backlog","prefix":"ST","team":{"sprintDays":10,"wipLimit":1},"project":{"stack":[]},"definitionOfDone":[],"definitionOfReady":[]})
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(s.0.join(".story-time/settings.json")).unwrap())
            .unwrap(),
        actual
    );
    assert!(ignored(&s, "").contains("*.env"));
}

#[test]
fn force_replaces_malformed_existing_settings_and_preserves_ignore_rules() {
    let s = Sandbox::new();
    fs::create_dir(s.0.join(".story-time")).unwrap();
    fs::write(s.0.join(".story-time/settings.json"), "malformed").unwrap();
    fs::write(s.0.join(".story-time/.gitignore"), "# local\nprivate/\n").unwrap();
    let out = s.init(&[], true);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
    assert_eq!(
        fs::read_to_string(s.0.join(".story-time/settings.json")).unwrap(),
        "malformed"
    );
    assert_eq!(ignored(&s, ""), "# local\nprivate/\n");
    let out = s.init(&["--force"], true);
    success(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Wrote"));
    assert_eq!(ignored(&s, ""), "# local\nprivate/\n*.env\n");
    success(&s.init(&["--force"], true));
    assert_eq!(ignored(&s, ""), "# local\nprivate/\n*.env\n");
    fs::write(
        s.0.join(".story-time/.gitignore"),
        "# preserve bytes\r\n*.env\r\n\n",
    )
    .unwrap();
    success(&s.init(&["--force"], true));
    assert_eq!(ignored(&s, ""), "# preserve bytes\r\n*.env\r\n\n");
}

#[test]
fn explicit_path_beats_environment_and_custom_roots_are_relative_to_cwd() {
    for target in [
        "custom/settings.json",
        "custom/.story-time/settings.json",
        "custom/../settings.json",
    ] {
        let s = Sandbox::new();
        let out = s
            .command(true)
            .args(["init", "--config", target, "--json"])
            .env("STORY_TIME_CONFIG", "wrong/settings.json")
            .output()
            .unwrap();
        settings(&out);
        assert!(!s.0.join("wrong").exists());
        let expected = if target.contains("..") {
            "settings.json"
        } else {
            target
        };
        assert!(s.0.join(expected).exists());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(s.0.join(expected)).unwrap()).unwrap()
                ["tracker"],
            "files"
        );
        let root = if target.contains("..") { "" } else { "custom" };
        assert!(ignored(&s, root).contains("*.env"));
    }
    let s = Sandbox::new();
    let out = s
        .command(true)
        .args(["init", "--json"])
        .env("STORY_TIME_CONFIG", "env/settings.json")
        .output()
        .unwrap();
    settings(&out);
    assert!(ignored(&s, "env").contains("*.env"));
    assert!(!s.0.join(".story-time").exists());
}

#[test]
fn invalid_input_has_diagnostics_and_no_writes_even_with_force() {
    for (input, expected) in [
        ("{", "invalid JSON"),
        ("[]", "Config must be a JSON object"),
        ("null", "Config must be a JSON object"),
        (r#"{"unknown":true}"#, "Unknown config key"),
        (r#"{"team":{"wipLimit":0}}"#, "team.wipLimit"),
        (
            r#"{"github":{"repo":"shop"}}"#,
            "github.repo must be owner/repo",
        ),
        (r#"{"tracker":"github"}"#, "github settings"),
        (r#"{"sync":"auto"}"#, "needs tracker"),
        (r#"{"project":{"stack":false}}"#, "project.stack"),
    ] {
        for existing in [false, true] {
            let s = Sandbox::new();
            fs::write(s.0.join("answers.json"), input).unwrap();
            if existing {
                fs::create_dir(s.0.join(".story-time")).unwrap();
                fs::write(s.0.join("settings.json"), "old settings").unwrap();
                fs::write(s.0.join(".story-time/.gitignore"), "old ignore\n").unwrap();
            }
            let out = s.init(
                &[
                    "--from",
                    "answers.json",
                    "--config",
                    "settings.json",
                    "--force",
                ],
                true,
            );
            assert_eq!(out.status.code(), Some(1));
            let diagnostic = String::from_utf8_lossy(&out.stderr);
            assert!(diagnostic.contains(expected), "{input}: {diagnostic}");
            assert_eq!(
                fs::read_dir(&s.0).unwrap().count(),
                if existing { 3 } else { 1 }
            );
            if existing {
                assert_eq!(
                    fs::read_to_string(s.0.join("settings.json")).unwrap(),
                    "old settings"
                );
                assert_eq!(ignored(&s, ""), "old ignore\n");
            } else {
                assert!(!s.0.join("settings.json").exists());
                assert!(!s.0.join(".story-time").exists());
            }
        }
    }
}

#[test]
fn tracker_and_nested_normalization_matches_retained_config_interface() {
    for input in [
        json!({"team":{"sprintDays":null,"wipLimit":null},"project":null,"tracker":null,"sync":null,"prefix":null}),
        json!({"$schema":"ignored","tracker":"files","backlogDir":"","team":{"velocity":3,"other":true},"project":{"testCommand":"","testFramework":null,"other":1}}),
        json!({"tracker":"github","sync":"ask","github":{"repo":"acme/shop","owner":"discard"}}),
        json!({"tracker":"ado","sync":"auto","ado":{"organization":"https://dev.azure.com/acme","project":"shop","areaPath":"","iterationPath":"iteration"}}),
        json!({"tracker":"gitea","gitea":{"baseUrl":"https://gitea.example.com///","owner":"acme","repo":"shop"}}),
        json!({"tracker":"jira","jira":{"project":"SHOP"},"jev":{"model":"jev-1.13.0"}}),
        json!({"tracker":"linear","linear":{"team":"shop"},"prefix":"PAY","definitionOfReady":["ready"],"definitionOfDone":["done"]}),
    ] {
        let s = Sandbox::new();
        s.answers(&input);
        let native = settings(&s.init(&["--from", "answers.json", "--json"], true));
        let retained = s
            .command(false)
            .args(["config", "--json"])
            .output()
            .unwrap();
        assert_eq!(settings(&retained), native, "input {input}");
        let original = Sandbox::new();
        original.answers(&input);
        let retained_init = Command::new("node")
            .arg(legacy())
            .args(["init", "--from", "answers.json", "--json"])
            .current_dir(&original.0)
            .env_remove("STORY_TIME_CONFIG")
            .output()
            .unwrap();
        assert_eq!(
            settings(&retained_init),
            native,
            "original normalization {input}"
        );
    }
}

#[test]
fn help_short_circuits_files_and_missing_flag_values_are_usage_errors() {
    let s = Sandbox::new();
    success(&s.init(&["--from", "missing.json", "--help"], true));
    for args in [vec!["--from"], vec!["--config"], vec!["--unknown"]] {
        let out = s.init(&args, true);
        assert_eq!(out.status.code(), Some(2));
        assert!(!out.stderr.is_empty());
    }
    assert_eq!(fs::read_dir(&s.0).unwrap().count(), 0);
}
