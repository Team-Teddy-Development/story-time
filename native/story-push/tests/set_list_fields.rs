use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("story-set-lists-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("backlog")).unwrap();
        for id in ["ZZ-001", "ST-001", "ST-002"] {
            fs::write(
                path.join(format!("backlog/{id}.md")),
                format!("---\nid: {id}\ntitle: {id}\nstatus: draft\ntype: story\ndepends_on: []\n---\n\n## Story\nAs a user, I want x, so that y.\n"),
            )
            .unwrap();
        }
        Self(path)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn story(dir: &Path, args: &[&str]) -> Output {
    let legacy =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs");
    let output = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(args)
        .env("STORY_TIME_LEGACY_CLI", legacy)
        .current_dir(dir)
        .output()
        .unwrap();
    output
}

fn depends_on_line(dir: &Path) -> String {
    fs::read_to_string(dir.join("backlog/ZZ-001.md"))
        .unwrap()
        .lines()
        .find(|line| line.starts_with("depends_on:"))
        .unwrap()
        .to_string()
}

#[test]
fn an_empty_inline_list_clears_depends_on() {
    let sandbox = Sandbox::new("empty");
    story(&sandbox.0, &["set", "ZZ-001", "depends_on=ST-001"]);
    let set = story(&sandbox.0, &["set", "ZZ-001", "depends_on=[]"]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    assert_eq!(depends_on_line(&sandbox.0), "depends_on: []");
    let lint = story(&sandbox.0, &["lint", "ZZ-001"]);
    let report = String::from_utf8_lossy(&lint.stdout);
    assert!(
        !report.contains("\"[]\""),
        "dangling dependency on \"[]\": {report}"
    );
}

#[test]
fn an_inline_list_sets_each_dependency() {
    let sandbox = Sandbox::new("list");
    let set = story(
        &sandbox.0,
        &["set", "ZZ-001", "depends_on=[ST-001, ST-002]"],
    );
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    assert_eq!(depends_on_line(&sandbox.0), "depends_on: [ST-001, ST-002]");
}
