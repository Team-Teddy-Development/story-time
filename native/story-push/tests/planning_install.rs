use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Installation(PathBuf);
impl Drop for Installation {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn run(binary: &Path, root: &Path, args: &[&str]) -> Value {
    let out = Command::new(binary)
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn documented_commands(text: &str) -> Vec<Vec<String>> {
    let mut bash = false;
    let mut commands = Vec::new();
    for line in text.lines() {
        if line == "```bash" {
            bash = true;
        } else if line == "```" {
            bash = false;
        } else if bash && line.starts_with("story plan ") {
            commands.push(line.split_whitespace().map(str::to_owned).collect());
        }
    }
    commands
}
fn metadata(text: &str, key: &str) -> String {
    let prefix = format!("  {key}: ");
    serde_json::from_str(
        text.lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn real_installer_ships_both_entry_points_and_executes_their_documented_native_interface() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp = std::env::temp_dir().join(format!("ST018-install-{}", std::process::id()));
    fs::create_dir(&temp).unwrap();
    let _cleanup = Installation(temp.clone());
    let out = Command::new("node")
        .arg(source.join("scripts/install.mjs"))
        .args(["--agent", "all", "--scope", "user"])
        .env("HOME", &temp)
        .env_remove("CODEX_HOME")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Installed 26 story-time skills"));
    assert!(
        !temp.join(".cargo/bin/story").exists(),
        "skill installer must not pretend to install native story"
    );
    let binary = temp.join("story");
    fs::copy(env!("CARGO_BIN_EXE_story"), &binary).unwrap();
    let expected = fs::read_to_string(source.join("test/expected-skills.mjs")).unwrap();
    let readme = fs::read_to_string(source.join("README.md")).unwrap();
    for agent in [".agents", ".claude"] {
        let skills = temp.join(agent).join("skills");
        let router = fs::read_to_string(skills.join("story-time/SKILL.md")).unwrap();
        for name in ["refinement", "sprint-planning"] {
            assert!(router.contains(&format!("**{name}** skill")));
            assert!(readme.contains(&format!("`{name}`")));
            assert!(expected.contains(&format!("'{name}'")));
        }
        let alias = fs::read_to_string(skills.join("backlog-groom/SKILL.md")).unwrap();
        assert!(alias.contains("Run the **refinement** skill in backlog mode"));
        assert!(alias.lines().count() < 30);
        assert!(
            documented_commands(&alias).is_empty(),
            "alias cannot own another planner"
        );
        let schema: Value = serde_json::from_slice(
            &fs::read(skills.join("story-time/references/planning-request.schema.json")).unwrap(),
        )
        .unwrap();
        let reference =
            fs::read_to_string(skills.join("story-time/references/planning.md")).unwrap();
        assert_eq!(schema["properties"]["schema_version"]["const"], 1);
        assert_eq!(schema["additionalProperties"], false);
        assert!(reference.contains("does not install the binary"));
        for (name, mode) in [
            ("refinement", "single-story"),
            ("refinement", "backlog"),
            ("sprint-planning", "backlog"),
        ] {
            let entry = fs::read_to_string(skills.join(name).join("SKILL.md")).unwrap();
            assert!(entry.starts_with(&format!("---\nname: {name}\n")));
            assert!(entry.lines().count() <= 250);
            assert!(entry.contains("../story-time/references/planning.md"));
            let yaml = fs::read_to_string(skills.join(name).join("agents/openai.yaml")).unwrap();
            assert!(!metadata(&yaml, "display_name").is_empty());
            assert!((25..=64).contains(&metadata(&yaml, "short_description").len()));
            assert!(metadata(&yaml, "default_prompt").contains(&format!("${name}")));
            let commands = documented_commands(&entry);
            assert_eq!(
                commands.len(),
                3,
                "{name} must expose the complete snapshot/stdout/pair interface"
            );
            assert_eq!(
                commands[0],
                vec!["story", "plan", "--snapshot", ">", "request.json"]
            );
            assert_eq!(commands[1], vec!["story", "plan", "--from", "request.json"]);
            assert_eq!(
                commands[2][..5],
                ["story", "plan", "--from", "request.json", "--output"]
            );
            let project = temp.join(format!("{}-{name}-{mode}", agent.trim_start_matches('.')));
            fs::create_dir_all(project.join(".story-time")).unwrap();
            fs::create_dir(project.join("work")).unwrap();
            let config=json!({"tracker":"jira","sync":"off","prefix":"PAY","backlogDir":"work","team":{"sprintDays":10,"wipLimit":1},"jira":{"preserve":"unchanged"}}).to_string();
            fs::write(project.join(".story-time/settings.json"), &config).unwrap();
            let ready="---\nid: PAY-001\nstatus: ready\nsize: 3\n---\n\n## Story\nPreserve this estimate.\n";
            let draft = "---\nid: PAY-002\nstatus: draft\nsize: 5\n---\n\n## Story\nNot ready.\n";
            fs::write(project.join("work/PAY-001.md"), ready).unwrap();
            fs::write(project.join("work/PAY-002.md"), draft).unwrap();
            let mut request = run(&binary, &project, &[&commands[0][1], &commands[0][2]]);
            let actual_keys: BTreeSet<_> = request.as_object().unwrap().keys().collect();
            let documented_keys: BTreeSet<_> =
                schema["properties"].as_object().unwrap().keys().collect();
            assert_eq!(
                actual_keys, documented_keys,
                "shipped schema must match the actual complete request"
            );
            if mode == "single-story" {
                request["candidate_order"] = json!(["PAY-001"]);
            }
            fs::write(project.join("request.json"), request.to_string()).unwrap();
            let a = run(
                &binary,
                &project,
                &commands[1][1..]
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            );
            assert_eq!(a["assessment"], "provisional");
            assert_eq!(a["selected"], json!([]));
            assert_eq!(a["dispositions"].as_array().unwrap().len(), 2);
            assert_eq!(a["publication"]["tracker"], "jira");
            assert_eq!(a["publication"]["sprint_fields_supported"], false);
            let pair = run(
                &binary,
                &project,
                &commands[2][1..]
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            );
            let output = project.join(&commands[2][5]);
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(&output).unwrap()).unwrap(),
                pair
            );
            assert!(fs::read_to_string(output.with_extension("md"))
                .unwrap()
                .contains("provisional"));
            assert_eq!(
                fs::read_to_string(project.join(".story-time/settings.json")).unwrap(),
                config
            );
            assert_eq!(
                fs::read_to_string(project.join("work/PAY-001.md")).unwrap(),
                ready
            );
            assert_eq!(
                fs::read_to_string(project.join("work/PAY-002.md")).unwrap(),
                draft
            );
            assert_eq!(
                fs::read_dir(project.join(".story-time")).unwrap().count(),
                1
            );
            assert_eq!(fs::read_dir(project.join("work")).unwrap().count(), 2);
        }
    }
}
