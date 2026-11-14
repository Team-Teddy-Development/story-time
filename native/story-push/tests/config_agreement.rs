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
    fn new(settings: &Value) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-config-agreement-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::write(path.join(".story-time/settings.json"), settings.to_string()).unwrap();
        Self(path)
    }
    fn story(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .args(args)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    /// Each command with the exit code it uses to reject settings; plan reports a conflict on stdout with exit 2.
    fn commands(&self) -> [(&'static str, Output, i32); 4] {
        let settings = ".story-time/settings.json";
        [
            ("config", self.story(&["config", "--config", settings]), 1),
            (
                "push",
                self.story(&["push", "--all", "--dry-run", "--config", settings]),
                1,
            ),
            ("plan", self.story(&["plan", "--snapshot"]), 2),
            (
                "init",
                self.story(&[
                    "init",
                    "--from",
                    settings,
                    "--config",
                    "out/settings.json",
                    "--force",
                ]),
                1,
            ),
        ]
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn pushable(extra: Value) -> Value {
    let mut settings = json!({"tracker": "github", "sync": "ask", "github": {"repo": "acme/shop"}});
    settings
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    settings
}

#[test]
fn config_push_plan_and_init_accept_the_same_settings() {
    for (name, settings) in [
        ("empty backlogDir", pushable(json!({"backlogDir": ""}))),
        ("team not an object", pushable(json!({"team": "x"}))),
        ("project not an object", pushable(json!({"project": []}))),
        (
            "integral float and null team numbers",
            pushable(json!({"team": {"sprintDays": 10.0, "wipLimit": null}})),
        ),
        (
            "falsy unused tracker settings",
            pushable(json!({"gitea": "", "ado": 0})),
        ),
        ("jev null", pushable(json!({"jev": null}))),
        ("jev true", pushable(json!({"jev": true}))),
        (
            "ado process null",
            json!({"tracker": "ado", "sync": "ask", "ado": {"organization": "https://dev.azure.com/acme", "project": "shop", "process": null}}),
        ),
    ] {
        let s = Sandbox::new(&settings);
        for (command, out, _) in s.commands() {
            assert_eq!(
                out.status.code(),
                Some(0),
                "{command} rejects {name}: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn config_push_plan_and_init_reject_the_same_settings_naming_the_same_field() {
    let gitea = |base_url: &str, owner: &str| json!({"tracker": "gitea", "sync": "ask", "gitea": {"baseUrl": base_url, "owner": owner, "repo": "shop"}});
    for (name, settings, field) in [
        (
            "github without repo",
            json!({"tracker": "github", "sync": "ask"}),
            "github.repo",
        ),
        (
            "empty jev model suffix",
            pushable(json!({"jev": {"model": "jev-"}})),
            "jev.model",
        ),
        (
            "ado areaPath not a string",
            json!({"tracker": "ado", "sync": "ask", "ado": {"organization": "https://dev.azure.com/acme", "project": "shop", "areaPath": 5}}),
            "ado.areaPath",
        ),
        (
            "gitea URL with credentials",
            gitea("https://robot:hunter2@gitea.example.com", "acme"),
            "gitea.baseUrl",
        ),
        (
            "gitea URL with query",
            gitea("https://gitea.example.com/?token=hunter2", "acme"),
            "gitea.baseUrl",
        ),
        (
            "gitea URL with fragment",
            gitea("https://gitea.example.com/#hunter2", "acme"),
            "gitea.baseUrl",
        ),
        (
            "gitea owner is a parent segment",
            gitea("https://gitea.example.com", ".."),
            "gitea.owner",
        ),
        (
            "github repo is a parent segment",
            pushable(json!({"github": {"repo": "acme/.."}})),
            "github.repo",
        ),
    ] {
        let s = Sandbox::new(&settings);
        for (command, out, rejects) in s.commands() {
            let stderr = String::from_utf8_lossy(if command == "plan" {
                &out.stdout
            } else {
                &out.stderr
            });
            assert_eq!(out.status.code(), Some(rejects), "{command} accepts {name}");
            assert!(
                stderr.contains(field),
                "{command} does not name {field} for {name}: {stderr}"
            );
            assert!(
                !stderr.contains("hunter2"),
                "{command} echoes a secret for {name}: {stderr}"
            );
            assert!(!s.0.join("out").exists(), "init wrote settings for {name}");
        }
    }
}
