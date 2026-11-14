use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const OLD: &str = "The CLI ignores files that aren't story files.";
const MAP_RULE: &str = "No command reads a `.map.md` file as a story.";
const ID_RULE: &str =
    "`story list`, `lint`, and `stats` read only files named for a story ID, such as `ST-012.md`.";
const STRAY_RULE: &str = "`story plan` and `story adopt` read every other `.md` file in the backlog directory as a story and stop with an error on one that is not.";

fn flowed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn story(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_story"))
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/nonexistent")
        .output()
        .unwrap()
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn story_map_names_the_backlog_file_rules_the_cli_follows() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-map/SKILL.md");
    let text = flowed(&fs::read_to_string(&path).unwrap());
    assert!(
        !text.contains(OLD),
        "overbroad claim is still present: {OLD}"
    );
    for rule in [MAP_RULE, ID_RULE, STRAY_RULE] {
        assert!(text.contains(rule), "missing rule: {rule}");
    }

    let scratch = Scratch(std::env::temp_dir().join(format!("ST097-{}", std::process::id())));
    let root = &scratch.0;
    fs::create_dir_all(root.join(".story-time")).unwrap();
    fs::create_dir_all(root.join("backlog")).unwrap();
    fs::write(
        root.join(".story-time/settings.json"),
        r#"{"tracker":"github","sync":"ask","prefix":"ST","backlogDir":"backlog","github":{"repo":"acme/shop"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("backlog/ST-001.md"),
        "---\nid: ST-001\ntitle: Pay\nstatus: ready\nsize: 3\nepic: EP-001\n---\n\n## Story\nx\n",
    )
    .unwrap();
    fs::write(root.join("backlog/EP-001.map.md"), "# EP-001 map\n").unwrap();

    let list: serde_json::Value =
        serde_json::from_slice(&story(root, &["list", "--json"]).stdout).unwrap();
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["ST-001"], "list skips the map");
    let stats: serde_json::Value =
        serde_json::from_slice(&story(root, &["stats", "--json"]).stdout).unwrap();
    assert_eq!(stats["total"], 1, "stats skips the map");
    let lint = String::from_utf8(story(root, &["lint"]).stdout).unwrap();
    assert!(lint.starts_with("ST-001"), "{lint}");
    assert!(!lint.contains("map"), "lint skips the map: {lint}");
    for args in [&["plan", "--snapshot"][..], &["adopt", "preview"]] {
        let out = story(root, args);
        assert!(out.status.success(), "{args:?} accepts a map file: {out:?}");
    }

    fs::write(root.join("backlog/notes.md"), "# notes\n").unwrap();
    let listed = story(root, &["list", "--json"]);
    assert!(listed.status.success());
    assert!(!String::from_utf8_lossy(&listed.stdout).contains("notes"));
    let plan = story(root, &["plan", "--snapshot"]);
    assert_eq!(plan.status.code(), Some(2), "{plan:?}");
    assert!(String::from_utf8_lossy(&plan.stdout).contains("malformed_story: backlog/notes.md"));
    let adopt = story(root, &["adopt", "preview"]);
    assert_eq!(adopt.status.code(), Some(2), "{adopt:?}");
    let report: serde_json::Value = serde_json::from_slice(&adopt.stdout).unwrap();
    assert_eq!(report["status"], "conflict");
    assert_eq!(report["diagnostics"][0]["code"], "unsupported_story");
    assert_eq!(report["diagnostics"][0]["path"], "backlog/notes.md");
}
