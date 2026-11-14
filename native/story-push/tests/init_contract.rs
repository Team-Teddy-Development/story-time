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
            .env("HOME", &self.0);
        if native {
            cmd.env("PATH", "/nonexistent");
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
        (r#"{"jev":{"model":"jev-"}}"#, "jev.model"),
        (r#"{"team":{"wipLimit":0}}"#, "team.wipLimit"),
        (r#"{"team":{"wipLimit":0.0}}"#, "team.wipLimit"),
        (r#"{"team":{"wipLimit":-1e0}}"#, "team.wipLimit"),
        (r#"{"team":{"wipLimit":1.5}}"#, "team.wipLimit"),
        (r#"{"team":{"sprintDays":0e0}}"#, "team.sprintDays"),
        (r#"{"team":{"sprintDays":-10.0}}"#, "team.sprintDays"),
        (r#"{"team":{"sprintDays":10.5}}"#, "team.sprintDays"),
        (
            r#"{"github":{"repo":"shop"}}"#,
            "github.repo must be owner/repo",
        ),
        (r#"{"tracker":"github"}"#, "github.repo"),
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
fn literal_integral_team_numbers_match_retained_init_and_config() {
    for input in [
        r#"{"team":{"sprintDays":10.0,"wipLimit":1e0}}"#,
        r#"{"team":{"sprintDays":1e1,"wipLimit":2.0}}"#,
    ] {
        let s = Sandbox::new();
        fs::write(s.0.join("answers.json"), input).unwrap();
        let native = settings(&s.init(&["--from", "answers.json", "--json"], true));
        assert!(native["team"]["sprintDays"].as_u64().is_some());
        assert!(native["team"]["wipLimit"].as_u64().is_some());
        let saved: Value =
            serde_json::from_slice(&fs::read(s.0.join(".story-time/settings.json")).unwrap())
                .unwrap();
        assert_eq!(saved, native);
        let retained = s
            .command(false)
            .args(["config", "--json"])
            .output()
            .unwrap();
        assert_eq!(settings(&retained), native, "retained reader {input}");
        let original = Sandbox::new();
        fs::write(original.0.join("answers.json"), input).unwrap();
        let retained_init = Command::new("node")
            .arg(legacy())
            .args(["init", "--from", "answers.json", "--json"])
            .current_dir(&original.0)
            .env_remove("STORY_TIME_CONFIG")
            .output()
            .unwrap();
        assert_eq!(settings(&retained_init), native, "retained init {input}");
    }
}

#[test]
fn settings_numbers_are_written_as_node_writes_them() {
    let input = r#"{"team":{"velocity":1e400,"sprintDays":9007199254740993,"wipLimit":18446744073709551615},"jira":{"a":[-1e400,1e21,5e-324,-0]}}"#;
    let (native, node) = (Sandbox::new(), Sandbox::new());
    for s in [&native, &node] {
        fs::write(s.0.join("answers.json"), input).unwrap();
    }
    let args = ["init", "--from", "answers.json", "--json"];
    let written = native.init(&args[1..], true);
    success(&written);
    let retained = Command::new("node")
        .arg(legacy())
        .args(args)
        .current_dir(&node.0)
        .env_remove("STORY_TIME_CONFIG")
        .output()
        .unwrap();
    success(&retained);
    assert_eq!(
        String::from_utf8_lossy(&written.stdout),
        String::from_utf8_lossy(&retained.stdout)
    );
    let file = ".story-time/settings.json";
    assert_eq!(
        fs::read_to_string(native.0.join(file)).unwrap(),
        fs::read_to_string(node.0.join(file)).unwrap()
    );
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

fn tree(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, (u32, Vec<u8>)> {
    use std::os::unix::fs::PermissionsExt;
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        into: &mut std::collections::BTreeMap<PathBuf, (u32, Vec<u8>)>,
    ) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let bytes = if meta.is_dir() {
                walk(root, &path, into);
                Vec::new()
            } else {
                fs::read(&path).unwrap()
            };
            into.insert(
                path.strip_prefix(root).unwrap().to_owned(),
                (meta.permissions().mode(), bytes),
            );
        }
    }
    let mut files = std::collections::BTreeMap::new();
    walk(root, root, &mut files);
    files
}

fn same_as_node(args: &[&str], fixture: &[(&str, &str)]) -> Output {
    let s = Sandbox::new();
    let args: Vec<String> = args
        .iter()
        .map(|a| a.replace("{sandbox}", s.0.to_str().unwrap()))
        .collect();
    let build = || {
        for entry in fs::read_dir(&s.0).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                fs::remove_dir_all(&path).unwrap();
            } else {
                fs::remove_file(&path).unwrap();
            }
        }
        for (name, text) in fixture {
            fs::write(s.0.join(name), text).unwrap();
        }
    };
    build();
    let node = Command::new("node")
        .arg(legacy())
        .arg("init")
        .args(&args)
        .current_dir(&s.0)
        .env_remove("STORY_TIME_CONFIG")
        .env_remove("STORY_TIME_DIR")
        .output()
        .unwrap();
    let node_tree = tree(&s.0);
    build();
    let native = Command::new(
        std::env::var_os("STORY_TIME_TEST_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_story").into()),
    )
    .arg("init")
    .args(&args)
    .current_dir(&s.0)
    .env_clear()
    .env("PATH", "/nonexistent")
    .env("HOME", &s.0)
    .output()
    .unwrap();
    let context = format!(
        "init {args:?}: node {:?} {} / native {:?} {}",
        node.status.code(),
        String::from_utf8_lossy(&node.stderr),
        native.status.code(),
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), node.status.code(), "{context}");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&node.stdout),
        "{context}"
    );
    assert_eq!(
        String::from_utf8_lossy(&native.stderr),
        String::from_utf8_lossy(&node.stderr),
        "{context}"
    );
    assert_eq!(tree(&s.0), node_tree, "{context}");
    native
}

#[test]
fn a_flag_value_that_starts_with_dashes_is_read_as_node_reads_it() {
    let answers = r#"{"tracker":"github","github":{"repo":"acme/shop"},"prefix":"PAY"}"#;
    for args in [&["--from", "--x", "--json"][..], &["--from", "--x"]] {
        let out = same_as_node(args, &[("--x", answers)]);
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("github"));
    }
    let out = same_as_node(
        &["--config", "--y/.story-time/settings.json", "--from", "--x"],
        &[("--x", answers)],
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("--y/.story-time/settings.json"));
}

#[test]
fn unknown_flags_and_positionals_are_ignored_as_node_ignores_them() {
    for args in [
        &["--zzz", "value", "--json"][..],
        &["extra", "--json"],
        &["-h", "--json"],
        &["--help", "value"],
        &["--from", "answers.json", "--help"],
        &["--from"],
        &["--config"],
        &["--unknown"],
    ] {
        same_as_node(args, &[("answers.json", "{}")]);
    }
}

#[test]
fn a_config_outside_story_time_creates_the_metadata_directory_as_node_does() {
    for target in [
        "settings.json",
        "sub/dir/settings.json",
        "custom/../settings.json",
        "{sandbox}/abs/settings.json",
    ] {
        for args in [&["--config", target][..], &["--config", target, "--json"]] {
            let out = same_as_node(args, &[]);
            assert!(out.status.success(), "init {args:?}");
        }
    }
}
