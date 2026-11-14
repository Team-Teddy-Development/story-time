use std::{
    collections::BTreeMap,
    fs,
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
risk_reduction: -0
time_criticality: 00012
remote: 123456789012345678901234567890
parent:
epic: ~
tags: [b, \"c, d\", 'it''s', 2, true, , null, [x]]
depends_on: []
custom_note: \"quoted: value \\u00e9\\t\"
quote_trailing: \"x\"\x20\x20
single: 'a # b'
created: 2026-11-14
updated: 2026-11-14
zeta: last
alpha: first
title: Duplicate title wins
_under: x
# a comment line
small: 0.0000001
big: 1234567890123456789012345
flag: false
plain: 42abc
spaced:    padded value\x20\x20
---
Preamble text is ignored.
## Story
As a user, I want X.

## 2
Integer-named section.

## Acceptance Criteria
- [ ] Given A, when B, then C
##No space is body text

##
## Story
Duplicate section wins.
";

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-show-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
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
        cmd.arg("show")
            .args(args)
            .envs(env.iter().copied())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    /// Runs Node and native on the same tree and requires equal stdout, exit code, and files.
    /// Stderr must match too unless `same_stderr` is false, where the message comes from V8 or the OS.
    fn compare(&self, args: &[&str], env: &[(&str, &str)], same_stderr: bool) -> Output {
        let before = self.files();
        let node = self.run(false, args, env);
        assert_eq!(before, self.files(), "Node changed files for {args:?}");
        let native = self.run(true, args, env);
        assert_eq!(before, self.files(), "native changed files for {args:?}");
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
        native
    }
    fn both(&self, args: &[&str], env: &[(&str, &str)]) -> [Output; 2] {
        let json: Vec<&str> = ["--json"].iter().chain(args).copied().collect();
        [
            self.compare(args, env, true),
            self.compare(&json, env, true),
        ]
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

fn succeeds(outputs: [Output; 2]) {
    for out in outputs {
        assert_eq!(out.status.code(), Some(0));
        assert!(!out.stdout.is_empty());
    }
}

#[test]
fn shows_a_story_like_node() {
    let s = Sandbox::new();
    s.write("backlog/ST-001.md", RICH);
    s.write("backlog/ST-002.md", &RICH.replace('\n', "\r\n"));
    s.write(
        "backlog/PAY-7.md",
        "---\nid: PAY-7\ntitle: No sections\n---\n",
    );
    s.write(
        "backlog/ST-003.md",
        "---\r\nid: ST-003\r\n---\r\n## Notes\r\nA lone \r carriage return\r\n",
    );
    for id in ["ST-001", "ST-002", "PAY-7", "ST-003"] {
        succeeds(s.both(&[id], &[]));
    }
    succeeds(s.both(&["--format", "x", "ST-001", "extra"], &[]));
    let out = s.compare(&["ST-001"], &[], true);
    assert!(String::from_utf8_lossy(&out.stdout).contains("title: Duplicate title wins\n"));
}

#[test]
fn failures_match_node() {
    let s = Sandbox::new();
    s.write("backlog/ST-001.md", "no frontmatter\n");
    s.write(
        "backlog/ST-002.md",
        "---\nid: ST-002\nnot a key line\n---\n",
    );
    s.write("backlog/ST-003.md", "---\ntitle: \"bad\"x\"\n---\n");
    fs::create_dir_all(s.0.join("backlog/ST-004.md")).unwrap();
    for (args, code) in [
        (&["ST-001"][..], 1),
        (&["ST-002"][..], 1),
        (&["ST-009"][..], 1),
        (&["st-001"][..], 1),
        (&["ST"][..], 1),
        (&[][..], 2),
        (&["--json"][..], 2),
        (&["ST-001", "--dir"][..], 2),
    ] {
        for out in s.both(args, &[]) {
            assert_eq!(out.status.code(), Some(code), "{args:?}");
            assert!(out.stdout.is_empty(), "{args:?} printed to stdout");
        }
    }
    for id in ["ST-003", "ST-004"] {
        let out = s.compare(&[id], &[], false);
        assert_eq!(out.status.code(), Some(1), "{id}");
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn backlog_directory_precedence_matches_node() {
    let s = Sandbox::new();
    s.write(
        ".story-time/settings.json",
        r#"{"backlogDir":"stories","prefix":"PAY"}"#,
    );
    s.write(
        "stories/ST-001.md",
        "---\nid: ST-001\ntitle: From config\n---\n",
    );
    s.write("flag/ST-001.md", "---\nid: ST-001\ntitle: From flag\n---\n");
    s.write("env/ST-001.md", "---\nid: ST-001\ntitle: From env\n---\n");
    s.write("ST-001.md", "---\nid: ST-001\ntitle: From root\n---\n");
    for (args, env, title) in [
        (&["ST-001"][..], &[][..], "From config"),
        (
            &["ST-001"][..],
            &[("STORY_TIME_DIR", "env")][..],
            "From env",
        ),
        (&["ST-001"][..], &[("STORY_TIME_DIR", "")][..], "From root"),
        (
            &["ST-001", "--dir", "flag"][..],
            &[("STORY_TIME_DIR", "env")][..],
            "From flag",
        ),
        (
            &["ST-001", "--dir", "./env/../flag"][..],
            &[][..],
            "From flag",
        ),
    ] {
        for out in s.both(args, env) {
            assert_eq!(out.status.code(), Some(0));
            assert!(
                String::from_utf8_lossy(&out.stdout).contains(title),
                "{args:?} {env:?}"
            );
        }
    }
    let alt = Sandbox::new();
    alt.write("conf/story.json", r#"{"backlogDir":"../stories"}"#);
    alt.write("stories/ST-001.md", "---\nid: ST-001\n---\n");
    succeeds(alt.both(&["ST-001", "--config", "conf/story.json"], &[]));
    succeeds(alt.both(&["ST-001"], &[("STORY_TIME_CONFIG", "conf/story.json")]));
}

#[test]
fn invalid_config_fails_like_node() {
    let s = Sandbox::new();
    s.write("backlog/ST-001.md", "---\nid: ST-001\n---\n");
    for text in ["{", r#"{"colour":"red"}"#, r#"{"backlogDir":3}"#] {
        s.write(".story-time/settings.json", text);
        let out = s.compare(&["ST-001"], &[], false);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(out.stdout.is_empty());
    }
}
