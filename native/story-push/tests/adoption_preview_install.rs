use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};

struct Installation(PathBuf);
impl Drop for Installation {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn disposable_skill_install_routes_before_gitea_and_native_commands_need_no_node() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp = std::env::temp_dir().join(format!("ST022-installed-{}", std::process::id()));
    fs::create_dir(&temp).unwrap();
    let _cleanup = Installation(temp.clone());
    let install = Command::new("node")
        .arg(source.join("scripts/install.mjs"))
        .args(["--agent", "codex", "--scope", "project", "--project"])
        .arg(&temp)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let skill = fs::read_to_string(temp.join(".agents/skills/story-setup/SKILL.md")).unwrap();
    assert!(
        skill.split('\n').count() <= 250,
        "installed setup exceeds its 250-line contract"
    );
    let routing = skill
        .find("## New, adopt, or upgrade")
        .expect("installed setup must detect existing sources first");
    assert!(routing < skill.find("## Gitea setup branch").unwrap());
    assert!(skill[routing..].contains("story adopt preview"));
    assert!(skill.contains("already_adopted"));
    assert!(!skill.contains("story adopt apply"));
    let binary = temp.join("story");
    fs::copy(env!("CARGO_BIN_EXE_story"), &binary).unwrap();
    for kind in ["new", "adopt", "upgrade"] {
        let project = temp.join(kind);
        fs::create_dir(&project).unwrap();
        let run = |args: &[&str]| {
            Command::new(&binary)
                .args(args)
                .current_dir(&project)
                .env_clear()
                .env("PATH", "/nonexistent")
                .output()
                .unwrap()
        };
        if kind == "new" {
            fs::write(
                project.join("answers.json"),
                json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"backlog"})
                    .to_string(),
            )
            .unwrap();
            let out = run(&["init", "--from", "answers.json"]);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(project.join(".story-time/settings.json").is_file());
        } else {
            fs::create_dir(project.join("backlog")).unwrap();
            fs::write(
                project.join("backlog/PAY-001.md"),
                "---\nid: PAY-001\nstatus: done\n---\n\n## Story\nRetain.\n",
            )
            .unwrap();
            fs::write(
                project.join(".story-time.json"),
                json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"backlog"})
                    .to_string(),
            )
            .unwrap();
            if kind == "upgrade" {
                fs::create_dir(project.join(".story-time")).unwrap();
                fs::write(
                    project.join(".story-time/lifecycle.json"),
                    r#"{"schema_version":99}"#,
                )
                .unwrap();
            }
            let out = run(&["adopt", "preview"]);
            let r: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(out.status.code(), Some(if kind == "adopt" { 0 } else { 2 }));
            assert_eq!(
                r["status"],
                if kind == "adopt" {
                    "preview"
                } else {
                    "conflict"
                }
            );
            assert!(out.stderr.is_empty());
        }
    }
}
