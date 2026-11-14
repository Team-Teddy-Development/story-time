use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

type Tree = BTreeMap<PathBuf, (u32, Option<Vec<u8>>)>;
type Fixture = fn(&Path);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-split-{}-{}",
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
        cmd.arg("split")
            .args(args)
            .envs(env.iter().copied())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    /// Builds the fixture, runs Node, rebuilds it at the same path, runs native, and requires
    /// equal stdout, exit code, and file tree (bytes and modes). Node's `split` stamps today's UTC
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

const CHILDREN: &[(&str, &str)] = &[
    (
        "full.json",
        r#"[
  {"title": "Pay by card", "statement": "As a buyer, I want to pay by card.", "acceptance": ["Given a card, when I pay, then it clears", "Given a decline, when I pay, then I see why"], "notes": "From the split.", "type": "story", "persona": "buyer", "value": 5, "time_criticality": 2, "risk_reduction": 1.5, "size": 3, "depends_on": ["ST-001"]},
  {"title": "Pay: by invoice"},
  {"title": 5, "acceptance": [null, [1, null], {}, true, ""], "notes": 0, "depends_on": ["ST-002", null, [3]], "size": "3", "value": null, "persona": null, "type": null, "statement": {"a": 1}, "risk_reduction": true}
]"#,
    ),
    (
        "two.json",
        r#"[{"title": "A", "notes": "n"}, {"title": "B", "statement": ""}]"#,
    ),
    ("scalars.json", r#"["text", 7, [], false]"#),
    ("one.json", r#"[{"title": "Only"}]"#),
    ("object.json", r#"{"title": "Not a list"}"#),
    ("nullspec.json", r#"[{"title": "A"}, null]"#),
    (
        "badac.json",
        r#"[{"title": "A", "acceptance": "not a list"}, {"title": "B"}]"#,
    ),
    ("bad.json", "[{\"title\": "),
];

fn stories(root: &Path) {
    put(
        root,
        "backlog/ST-001.md",
        "---\nid: ST-001\ntitle: Checkout\nstatus: ready\ntype: bug\nepic: EP-001\npersona: shopper\nsize: 8\nzeta: kept\ncreated: 2026-11-01\nupdated: 2026-11-02\n---\n## Story\nAs a shopper, I want to check out.\n\n## Changelog\n- 2026-11-01 created  \n",
    );
    put(
        root,
        "backlog/ST-002.md",
        "---\r\nid: ST-002\r\ntitle: No changelog\r\nstatus: draft\r\nchildren: [ST-000]\r\n---\r\n## Notes\r\nKeep.\r\n",
    );
    put(
        root,
        "backlog/ST-003.md",
        "---\nid: ST-003\nstatus: done\n---\n",
    );
    put(
        root,
        "backlog/ST-004.md",
        "---\nid: not-valid\nstatus: ready\n---\n",
    );
    put(
        root,
        "backlog/ST-005.md",
        "---\nid: ST-020\ntitle: Moved\n---\n",
    );
    put(
        root,
        "backlog/ST-006.md",
        "---\nid: ST-006\nstatus: split\n---\n",
    );
    put(
        root,
        "backlog/PAY-7.md",
        "---\nid: PAY-7\ntitle: Pay\n---\n",
    );
    put(root, "backlog/notes.md", "not a story\n");
    fs::set_permissions(
        root.join("backlog/ST-001.md"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    for (name, text) in CHILDREN {
        put(root, name, text);
    }
}

#[test]
fn splits_a_story_like_node() {
    let s = Sandbox::new();
    for (id, from) in [
        ("ST-001", "full.json"),
        ("ST-002", "full.json"),
        ("ST-002", "two.json"),
        ("ST-005", "two.json"),
        ("PAY-7", "scalars.json"),
    ] {
        for json in [false, true] {
            let mut args = vec![id, "--from", from];
            if json {
                args.push("--json");
            }
            let (out, tree) = s.compare(stories, &args, &[], true);
            assert_eq!(out.status.code(), Some(0), "{args:?}");
            let written = if id == "ST-005" { "ST-020" } else { id };
            let parent = &tree[&PathBuf::from(format!("backlog/{written}.md"))];
            let text = String::from_utf8(parent.1.clone().unwrap()).unwrap();
            assert!(text.contains("status: split\n"), "{text}");
        }
    }
    let (out, tree) = s.compare(stories, &["ST-001", "--from", "two.json"], &[], true);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "ST-001 split into ST-021, ST-022.\n"
    );
    assert!(tree.contains_key(Path::new("backlog/ST-022.md")));
}

#[test]
fn failures_and_rollback_match_node() {
    let s = Sandbox::new();
    for (args, code) in [
        (&[][..], 2),
        (&["ST-001"][..], 2),
        (&["--from", "two.json"][..], 2),
        (&["ST-001", "--from", ""][..], 2),
        (&["ST-001", "--from"][..], 2),
        (&["ST-001", "--from", "missing.json"][..], 1),
        (&["ST-099", "--from", "one.json"][..], 1),
        (&["ST-001", "--from", "object.json"][..], 1),
        (&["ST-099", "--from", "two.json"][..], 1),
        (&["st-001", "--from", "two.json"][..], 1),
        (&["ST-003", "--from", "two.json"][..], 1),
        (&["ST-006", "--from", "nullspec.json"][..], 1),
        (&["ST-001", "--from", "nullspec.json"][..], 1),
        (&["ST-001", "--from", "badac.json"][..], 1),
        (&["ST-004", "--from", "two.json", "--json"][..], 1),
    ] {
        let (out, _) = s.compare(stories, args, &[], true);
        assert_eq!(out.status.code(), Some(code), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?} printed to stdout");
    }
    let (out, tree) = s.compare(stories, &["ST-004", "--from", "full.json"], &[], true);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "Split failed at \"mark ST-004 split\": Invalid story id: not-valid\n"
    );
    assert!(!tree.contains_key(Path::new("backlog/ST-021.md")));
    let (out, _) = s.compare(stories, &["ST-001", "--from", "bad.json"], &[], false);
    assert_eq!(out.status.code(), Some(1));
}
