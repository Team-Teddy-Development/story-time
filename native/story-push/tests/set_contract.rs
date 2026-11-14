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
status: draft
size: 3
value: 1.50
depends_on: [ST-002]
tags: [b, \"c, d\", 2, true]
custom_note: \"quoted: value\"
created: 2026-11-01
updated: 2026-11-02
zeta: last
---
## Story
As a user, I want X.

## Changelog
- 2026-11-01 created\x20\x20
";

const READY: &str = "---
id: ST-010
title: Export invoices
status: refined
type: story
size: 3
value: 5
time_criticality: 3
risk_reduction: 2
state_machine: model/state/invoice.toml
---
## Story
As a billing admin, I want invoices exported, so that I can reconcile them.

## Acceptance Criteria
- [ ] Given an invoice, when I export, then a CSV row is written.
";

type Tree = BTreeMap<PathBuf, (u32, Option<Vec<u8>>)>;
type Fixture = fn(&Path);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-set-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
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
    fn run(&self, native: bool, args: &[&str]) -> Output {
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
        cmd.arg("set")
            .args(args)
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    /// Runs Node, rebuilds the fixture at the same path, runs native, and requires equal stdout,
    /// stderr, exit code, and file tree (bytes and modes). Both stamp today's UTC date, so a run
    /// that straddles midnight UTC is repeated.
    fn compare(&self, fixture: Fixture, args: &[&str]) -> (Output, Tree) {
        loop {
            let day = utc_day();
            self.rebuild(fixture);
            let node = self.run(false, args);
            let node_tree = self.files();
            self.rebuild(fixture);
            let native = self.run(true, args);
            let native_tree = self.files();
            if day != utc_day() {
                continue;
            }
            let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
            assert_eq!(
                text(&native.stdout),
                text(&node.stdout),
                "stdout differs for {args:?}; native stderr: {}",
                text(&native.stderr)
            );
            assert_eq!(
                text(&native.stderr),
                text(&node.stderr),
                "stderr differs for {args:?}"
            );
            assert_eq!(
                native.status.code(),
                node.status.code(),
                "exit code differs for {args:?}"
            );
            assert_eq!(native_tree, node_tree, "files differ for {args:?}");
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
        "backlog/ST-003.md",
        "---\nid: ST-005\ntitle: Moved\nstatus: ready\n---\n",
    );
    put(root, "backlog/ST-010.md", READY);
    put(
        root,
        "backlog/ST-011.md",
        &READY
            .replace("ST-010", "ST-011")
            .replace("state_machine: model/state/invoice.toml\n", ""),
    );
    fs::set_permissions(
        root.join("backlog/ST-002.md"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
}

fn broken(root: &Path) {
    stories(root);
    put(root, "backlog/ST-006.md", "---\ntitle: No id\n---\n");
    put(root, "backlog/ST-008.md", "no frontmatter\n");
}

fn with_machine(root: &Path) {
    stories(root);
    put(root, "model/state/invoice.toml", "entity = \"invoice\"\n");
}

#[test]
fn field_forms_match_node() {
    let s = Sandbox::new();
    let cases: &[&[&str]] = &[
        &["ST-001", "size=5"],
        &["ST-001", "size=1e3", "value= 0x10 "],
        &[
            "ST-001",
            "size=-0",
            "value=.5",
            "time-criticality=+3",
            "risk_reduction=  ",
        ],
        &["ST-001", "size="],
        &["ST-001", "depends_on=[ST-003, ST-004]"],
        &["ST-001", "depends-on= ST-003 ,, ST-004 ", "children=[]"],
        &["ST-001", "depends_on="],
        &["ST-001", "children=[ ]"],
        &["ST-001", "status=in-progress", "type=spike"],
        &["ST-001", "title=Colon: and # hash", "zeta=", "alpha=a=b"],
        &["ST-001", "remote=102", "remote_url=https://example.test/1"],
        &["ST-001", "custom=true", "other=null", "num=12", "blank= x "],
        &["ST-001", "size=2", "size=8"],
        &["ST-001", "9=nine", "b=bee", "1=one"],
        &["ST-001", "title=x", "--note", "poker 3/3/5"],
        &["ST-001", "title=x", "--note", ""],
        &["--json", "ST-001", "size=2", "depends_on=a,b"],
        &["ST-002", "status=done", "--note", "accepted"],
        &["ST-003", "title=Renamed"],
        &["ST-003", "status=ready"],
        &["ST-001", "__proto__=x", "toString=", "constructor=c"],
    ];
    for args in cases {
        let (out, _) = s.compare(stories, args);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
    }
    let (out, tree) = s.compare(stories, &["ST-001", "size=5", "--note", "why"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "ST-001 updated.\n");
    let text = String::from_utf8(tree[Path::new("backlog/ST-001.md")].1.clone().unwrap()).unwrap();
    assert!(
        text.contains(&format!("- {} set size=5 — why\n", utc_day())),
        "{text}"
    );
}

#[test]
fn the_ready_gate_matches_node() {
    let s = Sandbox::new();
    for (fixture, args, code) in [
        (stories as Fixture, &["ST-001", "status=ready"][..], 1),
        (stories, &["ST-001", "status=ready", "--force"][..], 0),
        (stories, &["ST-010", "status=ready"][..], 1),
        (with_machine, &["ST-010", "status=ready"][..], 0),
        (stories, &["ST-011", "status=ready"][..], 0),
        (stories, &["ST-011", "status=ready", "size=40"][..], 1),
        (stories, &["ST-003", "status=ready", "title="][..], 0),
        (broken, &["ST-011", "status=ready"][..], 1),
        (broken, &["ST-011", "status=ready", "--force"][..], 0),
    ] {
        let (out, _) = s.compare(fixture, args);
        assert_eq!(out.status.code(), Some(code), "{args:?}");
    }
}

#[test]
fn failures_match_node() {
    let s = Sandbox::new();
    for (args, code) in [
        (&[][..], 2),
        (&["ST-001"][..], 2),
        (&["ST-001", "size"][..], 2),
        (&["ST-001", "=5"][..], 2),
        (&["ST-001", "size=1", "--note"][..], 2),
        (&["ST-001", "id=ST-009"][..], 1),
        (&["ST-001", "size=2", "updated=x"][..], 1),
        (&["ST-001", "sections=x"][..], 1),
        (&["ST-001", "size=big"][..], 1),
        (&["ST-001", "value=Infinity"][..], 1),
        (&["ST-001", "status=Ready"][..], 1),
        (&["ST-001", "type=epic"][..], 1),
        (&["ST-099", "size=1"][..], 1),
        (&["st-001", "size=1"][..], 1),
        (&["ST-006", "size=1"][..], 1),
        (&["ST-008", "size=1"][..], 1),
    ] {
        let (out, _) = s.compare(broken, args);
        assert_eq!(out.status.code(), Some(code), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?} printed to stdout");
    }
}
