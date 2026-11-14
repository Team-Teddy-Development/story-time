use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const STORY: &str = "---
id: ST-001
title: Pay for an order
status: draft
type: story
created: 2026-11-01
updated: 2026-11-01
---

## Story
As a buyer, I want to pay, so that my order ships.

## Changelog
- 2026-11-01 created
";

type Tree = BTreeMap<PathBuf, (u32, Option<Vec<u8>>)>;

fn snapshot(root: &Path) -> Tree {
    fn walk(root: &Path, dir: &Path, into: &mut Tree) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let mode = meta.permissions().mode() & 0o7777;
            let rel = path.strip_prefix(root).unwrap().to_owned();
            if meta.is_dir() {
                into.insert(rel, (mode, None));
                walk(root, &path, into);
            } else {
                into.insert(rel, (mode, Some(fs::read(&path).unwrap())));
            }
        }
    }
    let mut tree = Tree::new();
    walk(root, root, &mut tree);
    tree
}

fn command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_story"));
    command
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/nonexistent");
    command
}

fn story(root: &Path, args: &[&str]) -> Output {
    command(root, args).output().unwrap()
}

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> Sandbox {
    let root = std::env::temp_dir().join(format!("story-cli-lock-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("backlog")).unwrap();
    fs::write(root.join("backlog/ST-001.md"), STORY).unwrap();
    fs::write(
        root.join("children.json"),
        r#"[{"title": "Pay by card"}, {"title": "Pay by invoice"}]"#,
    )
    .unwrap();
    Sandbox(root)
}

#[test]
fn cli_writes_wait_for_the_repository_lock() {
    let commands: [&[&str]; 4] = [
        &["set", "ST-001", "size=3"],
        &["note", "ST-001", "clarified the statement"],
        &["new", "--title", "Refund an order"],
        &["split", "ST-001", "--from", "children.json"],
    ];
    for args in commands {
        let sandbox = fixture(args[0]);
        let root = sandbox.0.clone();

        let before = snapshot(&root);
        let held = fs::File::open(&root).unwrap();
        held.try_lock().unwrap();
        let refused = story(&root, args);
        assert_eq!(refused.status.code(), Some(1), "{args:?} {refused:?}");
        assert!(refused.stdout.is_empty(), "{args:?} {refused:?}");
        assert_eq!(
            String::from_utf8_lossy(&refused.stderr),
            format!(
                "Another process held the repository write lock on {} for 3 seconds. Wait for it to finish, then rerun the command.\n",
                root.display()
            ),
            "{args:?}"
        );
        assert_eq!(
            snapshot(&root),
            before,
            "{args:?} changed a file under the lock"
        );
        drop(held);

        let done = story(&root, args);
        assert_eq!(done.status.code(), Some(0), "{args:?} {done:?}");
        assert_ne!(
            snapshot(&root),
            before,
            "{args:?} wrote nothing once the lock was free"
        );
    }
}

#[test]
fn a_cli_write_waits_for_a_writer_that_finishes() {
    let commands: [&[&str]; 4] = [
        &["set", "ST-001", "size=3"],
        &["note", "ST-001", "clarified the statement"],
        &["new", "--title", "Refund an order"],
        &["split", "ST-001", "--from", "children.json"],
    ];
    for args in commands {
        let sandbox = fixture(&format!("wait-{}", args[0]));
        let root = sandbox.0.clone();
        let before = snapshot(&root);
        let held = fs::File::open(&root).unwrap();
        held.try_lock().unwrap();
        let child = command(&root, args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(snapshot(&root), before, "{args:?} wrote under the lock");
        drop(held);
        let done = child.wait_with_output().unwrap();
        assert_eq!(done.status.code(), Some(0), "{args:?} {done:?}");
        assert_ne!(
            snapshot(&root),
            before,
            "{args:?} wrote nothing after the wait"
        );
    }
}
