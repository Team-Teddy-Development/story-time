use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const RICH: &str = "---
id: ST-001
title: Fix: the parser # trailing comment
status: ready
size: 3
value: 1.50
tags: [b, \"c, d\", 'it''s', 2, true, , null]
custom_note: \"quoted: value \\u00e9\\t\"
created: 2026-11-01
updated: 2026-11-02
zeta: last
alpha: first
---
Preamble text is ignored.
## Story
As a user, I want X.

## 2
Integer-named section.

## Acceptance Criteria
- [ ] Given A, when B, then C

## Changelog
- 2026-11-01 created\x20\x20
- 2026-11-02 set status=ready\t

";

type Tree = BTreeMap<PathBuf, (u32, Option<Vec<u8>>)>;
type Fixture = fn(&Path);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-note-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self(path)
    }
    fn rebuild(&self, fixture: Fixture) {
        let _ = fs::remove_dir_all(&self.0);
        fs::create_dir_all(self.0.join("backlog")).unwrap();
        fixture(&self.0);
    }
    fn files(&self) -> Tree {
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
        let mut files = Tree::new();
        walk(&self.0, &self.0, &mut files);
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
        cmd.arg("note")
            .args(args)
            .envs(env.iter().copied())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    /// Builds the fixture, runs Node, rebuilds it at the same path, runs native, and requires
    /// equal stdout, exit code, and file tree (bytes and modes). Node's `note` stamps today's UTC
    /// date and reads no clock override, so a run that straddles midnight UTC is repeated.
    fn compare(
        &self,
        fixture: Fixture,
        args: &[&str],
        env: &[(&str, &str)],
        same_stderr: bool,
    ) -> (Output, Tree) {
        loop {
            let day = utc_day();
            self.rebuild(fixture);
            let node = self.run(false, args, env);
            let node_tree = self.files();
            self.rebuild(fixture);
            let native = self.run(true, args, env);
            let native_tree = self.files();
            if day != utc_day() {
                continue;
            }
            let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
            assert_eq!(
                text(&native.stdout),
                text(&node.stdout),
                "stdout differs for {args:?} {env:?}; native stderr: {}",
                text(&native.stderr)
            );
            assert_eq!(
                native.status.code(),
                node.status.code(),
                "exit code differs for {args:?} {env:?}; native stderr: {}; node stderr: {}",
                text(&native.stderr),
                text(&node.stderr)
            );
            if same_stderr {
                assert_eq!(
                    text(&native.stderr),
                    text(&node.stderr),
                    "stderr differs for {args:?}"
                );
            } else {
                assert_eq!(native.stderr.is_empty(), node.stderr.is_empty());
            }
            assert_eq!(native_tree, node_tree, "files differ for {args:?} {env:?}");
            return (native, native_tree);
        }
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn utc_day() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn legacy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs")
}

fn put(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn stories(root: &Path) {
    put(root, "backlog/ST-001.md", RICH);
    put(
        root,
        "backlog/ST-002.md",
        &RICH.replace("ST-001", "ST-002").replace('\n', "\r\n"),
    );
    put(
        root,
        "backlog/PAY-7.md",
        "---\nid: PAY-7\ntitle: No sections\n---\n",
    );
    put(
        root,
        "backlog/ST-003.md",
        "---\nid: ST-003\nupdated: ~\nsections: shadowed\n---\n## Notes\nKeep.\n",
    );
    put(
        root,
        "backlog/ST-004.md",
        "---\nid: ST-005\ntitle: Moved\n---\n",
    );
    put(
        root,
        "backlog/ST-006.md",
        "---\ntitle: No id\n---\n## Story\nX\n",
    );
    put(root, "backlog/ST-007.md", "---\nid: [ST-007]\n---\n");
    put(root, "backlog/ST-008.md", "no frontmatter\n");
    fs::create_dir_all(root.join("backlog/ST-009.md")).unwrap();
    fs::set_permissions(
        root.join("backlog/PAY-7.md"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
}

#[test]
fn notes_a_story_like_node() {
    let s = Sandbox::new();
    for id in ["ST-001", "ST-002", "PAY-7", "ST-003", "ST-004"] {
        for json in [false, true] {
            let mut args = vec![id, "  rewrote", "the  story", "\"why\"  "];
            if json {
                args.insert(0, "--json");
            }
            let (out, tree) = s.compare(stories, &args, &[], true);
            assert_eq!(out.status.code(), Some(0), "{args:?}");
            let written = if id == "ST-004" { "ST-005" } else { id };
            let file = &tree[&PathBuf::from(format!("backlog/{written}.md"))];
            let text = String::from_utf8(file.1.clone().unwrap()).unwrap();
            assert!(
                text.contains(&format!("- {} rewrote the  story \"why\"\n", utc_day())),
                "{text}"
            );
        }
    }
    let (out, _) = s.compare(stories, &["ST-001", "--format", "x", "one"], &[], true);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "ST-001 noted.\n");
}

#[test]
fn failures_match_node() {
    let s = Sandbox::new();
    for (args, code) in [
        (&[][..], 2),
        (&["ST-001"][..], 2),
        (&["--json", "ST-001"][..], 2),
        (&["ST-001", "hi", "--dir"][..], 2),
        (&["ST-001", "  ", "\t"][..], 1),
        (&["st-001", "  "][..], 1),
        (&["st-001", "hi"][..], 1),
        (&["ST-099", "hi"][..], 1),
        (&["ST-006", "hi"][..], 1),
        (&["ST-007", "hi"][..], 1),
        (&["ST-008", "hi"][..], 1),
    ] {
        let (out, _) = s.compare(stories, args, &[], true);
        assert_eq!(out.status.code(), Some(code), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?} printed to stdout");
    }
    let (out, _) = s.compare(stories, &["ST-009", "hi"], &[], false);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn backlog_directory_precedence_matches_node() {
    fn dirs(root: &Path) {
        put(
            root,
            ".story-time/settings.json",
            r#"{"backlogDir":"stories","prefix":"PAY"}"#,
        );
        for dir in ["stories", "flag", "env", "."] {
            put(root, &format!("{dir}/ST-001.md"), "---\nid: ST-001\n---\n");
        }
    }
    let s = Sandbox::new();
    for (args, env, dir) in [
        (&["ST-001", "x"][..], &[][..], "stories"),
        (
            &["ST-001", "x"][..],
            &[("STORY_TIME_DIR", "env")][..],
            "env",
        ),
        (&["ST-001", "x"][..], &[("STORY_TIME_DIR", "")][..], ""),
        (
            &["ST-001", "x", "--dir", "./env/../flag"][..],
            &[("STORY_TIME_DIR", "env")][..],
            "flag",
        ),
    ] {
        let (out, tree) = s.compare(dirs, args, env, true);
        assert_eq!(out.status.code(), Some(0));
        let bytes = tree[&Path::new(dir).join("ST-001.md")].1.clone().unwrap();
        assert!(String::from_utf8(bytes).unwrap().contains("## Changelog\n"));
    }
    fn broken(root: &Path) {
        put(root, ".story-time/settings.json", r#"{"backlogDir":3}"#);
        put(root, "backlog/ST-001.md", "---\nid: ST-001\n---\n");
    }
    let (out, _) = s.compare(broken, &["ST-001", "x"], &[], false);
    assert_eq!(out.status.code(), Some(1));
}
