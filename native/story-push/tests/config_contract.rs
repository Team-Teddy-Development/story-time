use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::write(
            path.join("backlog/ST-001.md"),
            "---\nid: ST-001\ntitle: Fixture\nstatus: draft\n---\n",
        )
        .unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, relative: &str, text: &str) -> &Self {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }
    fn files(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    into.insert(path.clone(), Vec::new());
                    walk(&path, into);
                } else {
                    into.insert(path.clone(), fs::read(&path).unwrap());
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&self.0, &mut files);
        files
    }
    fn run(&self, native: bool, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut cmd = if native {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_story"));
            cmd.env_clear().env("PATH", "/nonexistent");
            cmd
        } else {
            let mut cmd = Command::new("node");
            cmd.arg(legacy())
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap());
            cmd
        };
        cmd.arg("config")
            .args(args)
            .envs(env.iter().copied())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn compare(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let before = self.files();
        let node = self.run(false, args, env);
        assert_eq!(before, self.files(), "Node changed files for {args:?}");
        let native = self.run(true, args, env);
        assert_eq!(before, self.files(), "native changed files for {args:?}");
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&node.stdout),
            "stdout differs for {args:?} {env:?}; native stderr: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(
            native.status.code(),
            node.status.code(),
            "exit code differs for {args:?} {env:?}; native stderr: {}; node stderr: {}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr)
        );
        native
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn legacy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs")
}

const RICH: &str = r#"{
  "sync": "ask",
  "$schema": "https://example.com/schema.json",
  "tracker": "gitea",
  "gitea": {"repo": "shop", "owner": "acme", "baseUrl": "http://gitea.example.com:3000//"},
  "team": {"wipLimit": 2, "velocity": 12.5, "sprintDays": 10.0},
  "project": {"testCommand": "cargo test", "stack": ["rust", "node"], "testFramework": ""},
  "definitionOfDone": ["merged", "deployed"],
  "jira": {"b": 1, "10": 2, "a": {"2": true, "1": null}, "2": [1.0, -0.0, 3.25]},
  "linear": {"team": "ENG"},
  "jev": {"model": "jev-1.13.0"},
  "ado": {"organization": "https://dev.azure.com/acme", "project": "Shop", "areaPath": "Shop\\Web", "iterationPath": "", "extra": 1},
  "github": {"repo": "acme/shop", "token": "ignored"},
  "backlogDir": "stories",
  "prefix": "PAY"
}"#;

fn successes(s: &Sandbox, args: &[&str], env: &[(&str, &str)]) {
    for extra in [&[][..], &["--json"][..]] {
        let all: Vec<&str> = args.iter().chain(extra).copied().collect();
        let out = s.compare(&all, env);
        assert_eq!(out.status.code(), Some(0), "{all:?} should succeed");
        assert!(!out.stdout.is_empty());
    }
}

#[test]
fn default_settings_file_matches_node() {
    let s = Sandbox::new();
    s.write(".story-time/settings.json", RICH);
    s.write(".story-time.json", r#"{"prefix":"LEGACY"}"#);
    successes(&s, &[], &[]);
    successes(&s, &["--dir", "elsewhere", "positional", "--all"], &[]);
}

#[test]
fn explicit_config_flag_matches_node() {
    let s = Sandbox::new();
    s.write(".story-time/settings.json", r#"{"prefix":"DEFAULT"}"#);
    s.write(
        "conf/team/story.json",
        r#"{"tracker":"github","github":{"repo":"acme/shop"}}"#,
    );
    successes(&s, &["--config", "conf/team/../team/story.json"], &[]);
    successes(
        &s,
        &["--config", "conf/team/story.json"],
        &[("STORY_TIME_CONFIG", ".story-time/settings.json")],
    );
    successes(&s, &["--config", "conf/missing.json"], &[]);
}

#[test]
fn story_time_config_environment_matches_node() {
    let s = Sandbox::new();
    s.write(".story-time/settings.json", r#"{"prefix":"DEFAULT"}"#);
    s.write(
        "env/settings.json",
        r#"{"tracker":"ado","ado":{"organization":"https://dev.azure.com/acme","project":"P","process":"scrum"},"team":{"velocity":30}}"#,
    );
    successes(&s, &[], &[("STORY_TIME_CONFIG", "env/settings.json")]);
    successes(&s, &[], &[("STORY_TIME_CONFIG", "")]);
    successes(
        &s,
        &["--config", ""],
        &[("STORY_TIME_CONFIG", "env/settings.json")],
    );
}

#[test]
fn legacy_story_time_json_fallback_matches_node() {
    let s = Sandbox::new();
    s.write(
        ".story-time.json",
        r#"{"prefix":"OLD","definitionOfReady":["estimated"]}"#,
    );
    successes(&s, &[], &[]);
}

#[test]
fn no_config_file_matches_node() {
    let s = Sandbox::new();
    successes(&s, &[], &[]);
}

#[test]
fn invalid_config_fails_like_node() {
    for (name, text) in [
        ("syntax", "{"),
        ("not object", "[]"),
        ("unknown key", r#"{"colour":"red"}"#),
        ("unknown tracker", r#"{"tracker":"trello"}"#),
        ("tracker type", r#"{"tracker":5}"#),
        ("prefix", r#"{"prefix":"st"}"#),
        ("sync", r#"{"sync":"sometimes"}"#),
        ("sync without pushable tracker", r#"{"sync":"ask"}"#),
        ("backlogDir type", r#"{"backlogDir":3}"#),
        ("definitionOfDone null", r#"{"definitionOfDone":null}"#),
        ("sprintDays", r#"{"team":{"sprintDays":1.5}}"#),
        ("wipLimit", r#"{"team":{"wipLimit":0}}"#),
        ("velocity null", r#"{"team":{"velocity":null}}"#),
        ("stack", r#"{"project":{"stack":"rust"}}"#),
        ("github missing", r#"{"tracker":"github"}"#),
        ("github repo", r#"{"github":{"repo":"acme"}}"#),
        (
            "ado process",
            r#"{"ado":{"organization":"o","project":"p","process":"cmmi"}}"#,
        ),
        (
            "gitea key",
            r#"{"gitea":{"baseUrl":"http://h","owner":"o","repo":"r","token":"t"}}"#,
        ),
        (
            "gitea url",
            r#"{"gitea":{"baseUrl":"ftp://h","owner":"o","repo":"r"}}"#,
        ),
        (
            "gitea owner",
            r#"{"gitea":{"baseUrl":"http://h","owner":"a/b","repo":"r"}}"#,
        ),
        ("jev key", r#"{"jev":{"key":"secret"}}"#),
        ("jev model", r#"{"jev":{"model":"jev-"}}"#),
        ("jev model empty", r#"{"jev":{"model":""}}"#),
    ] {
        let s = Sandbox::new();
        s.write(".story-time/settings.json", text);
        for args in [&[][..], &["--json"][..]] {
            let out = s.compare(args, &[]);
            assert_eq!(out.status.code(), Some(1), "{name} should fail");
            assert!(out.stdout.is_empty(), "{name} printed to stdout");
            let settings = s.0.join(".story-time/settings.json");
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .starts_with(&format!("{}: ", settings.display())),
                "{name} reason does not name the config file: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn edge_values_match_node() {
    for text in [
        r#"{"github":0,"gitea":"","jira":false,"linear":null,"jev":{}}"#,
        r#"{"team":"x","project":[],"backlogDir":"","prefix":"A1"}"#,
        r#"{"team":{"velocity":1e2,"sprintDays":14},"jira":"ab","linear":[true]}"#,
    ] {
        let s = Sandbox::new();
        s.write(".story-time/settings.json", text);
        successes(&s, &[], &[]);
    }
}

#[test]
fn settings_numbers_echo_as_node_prints_them() {
    for text in [
        r#"{"prefix":5e-324}"#,
        r#"{"prefix":-5e-324}"#,
        r#"{"tracker":5e-324}"#,
        r#"{"sync":-5e-324}"#,
        r#"{"prefix":1e21}"#,
    ] {
        let s = Sandbox::new();
        s.write(".story-time/settings.json", text);
        let node = s.run(false, &[], &[]);
        let native = s.compare(&[], &[]);
        assert_eq!(native.status.code(), Some(1), "{text} should fail");
        assert_eq!(
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr),
            "stderr differs for {text}"
        );
    }
}

#[test]
fn usage_errors_match_node() {
    let s = Sandbox::new();
    for args in [&["--config"][..], &["--help"][..], &["--dir"][..]] {
        let out = s.compare(args, &[]);
        assert_eq!(out.status.code(), Some(2), "{args:?} is a usage error");
    }
}

/// Intended divergence (ST-071): native config shares push's tracker URL checks, so it rejects these settings although Node accepts them.
#[test]
fn tracker_url_checks_reject_settings_node_accepts() {
    for (text, field) in [
        (
            r#"{"gitea":{"baseUrl":"https://robot:hunter2@h","owner":"o","repo":"r"}}"#,
            "gitea.baseUrl",
        ),
        (
            r#"{"gitea":{"baseUrl":"https://h/?token=hunter2","owner":"o","repo":"r"}}"#,
            "gitea.baseUrl",
        ),
        (
            r#"{"gitea":{"baseUrl":"https://h/#hunter2","owner":"o","repo":"r"}}"#,
            "gitea.baseUrl",
        ),
        (
            r#"{"gitea":{"baseUrl":"https://h","owner":"..","repo":"r"}}"#,
            "gitea.owner",
        ),
        (
            r#"{"gitea":{"baseUrl":"https://h","owner":"o","repo":"."}}"#,
            "gitea.repo",
        ),
        (r#"{"github":{"repo":"acme/.."}}"#, "github.repo"),
    ] {
        let s = Sandbox::new();
        s.write(".story-time/settings.json", text);
        assert_eq!(
            s.run(false, &["--json"], &[]).status.code(),
            Some(0),
            "Node accepts {text}"
        );
        let native = s.run(true, &["--json"], &[]);
        let stderr = String::from_utf8_lossy(&native.stderr);
        assert_eq!(native.status.code(), Some(1), "native accepts {text}");
        assert!(native.stdout.is_empty());
        assert!(stderr.contains(field), "{text}: {stderr}");
        assert!(
            !stderr.contains("hunter2"),
            "{text} echoes a secret: {stderr}"
        );
    }
}

/// Intended divergence (ST-074): push reads these tracker fields as strings, so native rejects a non-string value that Node's `String(value)` accepts.
#[test]
fn non_string_tracker_fields_reject_settings_node_accepts() {
    for (text, field) in [
        (r#"{"github":{"repo":["acme/shop"]}}"#, "github.repo"),
        (
            r#"{"ado":{"organization":5,"project":"shop"}}"#,
            "ado.organization",
        ),
        (
            r#"{"ado":{"organization":"https://dev.azure.com/acme","project":5}}"#,
            "ado.project",
        ),
        (
            r#"{"gitea":{"baseUrl":"https://h","owner":["o"],"repo":"r"}}"#,
            "gitea.owner",
        ),
        (
            r#"{"gitea":{"baseUrl":"https://h","owner":"o","repo":5}}"#,
            "gitea.repo",
        ),
        (r#"{"prefix":["ST"]}"#, "prefix"),
    ] {
        let s = Sandbox::new();
        s.write(".story-time/settings.json", text);
        assert_eq!(
            s.run(false, &["--json"], &[]).status.code(),
            Some(0),
            "Node accepts {text}"
        );
        let native = s.run(true, &["--json"], &[]);
        let stderr = String::from_utf8_lossy(&native.stderr);
        assert_eq!(native.status.code(), Some(1), "native accepts {text}");
        assert!(native.stdout.is_empty());
        assert!(stderr.contains(field), "{text}: {stderr}");
    }
}
