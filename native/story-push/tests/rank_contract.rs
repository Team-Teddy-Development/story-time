use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-rank-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, relative: &str, text: &str) -> &Self {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }
    fn story(&self, id: &str, frontmatter: &str) -> &Self {
        self.write(
            &format!("backlog/{id}.md"),
            &format!(
                "---\nid: {id}\n{frontmatter}\n---\n\n## Story\nAs a user, I want x, so that y.\n"
            ),
        )
    }
    fn files(&self) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
        fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let mode = fs::symlink_metadata(&path).unwrap().permissions().mode();
                if path.is_dir() {
                    into.insert(path.clone(), (mode, Vec::new()));
                    walk(&path, into);
                } else {
                    into.insert(path.clone(), (mode, fs::read(&path).unwrap()));
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&self.0, &mut files);
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
        cmd.arg("rank")
            .args(args)
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn compare(&self, args: &[&str]) -> Output {
        self.compare_streams(args, true)
    }
    /// `with_stderr` is false where Node's stderr is V8's own text, such as a JSON parse error.
    fn compare_streams(&self, args: &[&str], with_stderr: bool) -> Output {
        let before = self.files();
        let node = self.run(false, args);
        assert_eq!(before, self.files(), "Node changed files for {args:?}");
        let native = self.run(true, args);
        assert_eq!(before, self.files(), "native changed files for {args:?}");
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&node.stdout),
            "stdout differs for {args:?}; native stderr: {}; node stderr: {}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr)
        );
        if with_stderr {
            assert_eq!(
                String::from_utf8_lossy(&native.stderr),
                String::from_utf8_lossy(&node.stderr),
                "stderr differs for {args:?}"
            );
        }
        assert_eq!(
            native.status.code(),
            node.status.code(),
            "exit code differs for {args:?}"
        );
        native
    }
    fn both_forms(&self, args: &[&str], code: i32) -> String {
        let mut plain = String::new();
        for extra in [&[][..], &["--json"][..]] {
            let all: Vec<&str> = args.iter().chain(extra).copied().collect();
            let out = self.compare(&all);
            assert_eq!(out.status.code(), Some(code), "{all:?} exit code");
            if extra.is_empty() {
                plain = String::from_utf8(out.stdout).unwrap();
            }
        }
        plain
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

fn scored(status: &str, value: &str, tc: &str, rr: &str, size: &str) -> String {
    format!("status: {status}\nvalue: {value}\ntime_criticality: {tc}\nrisk_reduction: {rr}\nsize: {size}")
}

fn mixed(s: &Sandbox) {
    s.story(
        "ST-001",
        &format!("title: Top\n{}", scored("ready", "8", "5", "3", "2")),
    );
    s.story(
        "ST-002",
        &format!(
            "title: Same score, larger\n{}",
            scored("ready", "8", "8", "0", "2")
        ),
    );
    s.story(
        "ST-003",
        &format!(
            "title: Same score, created later\n{}\ncreated: 9",
            scored("ready", "4", "4", "0", "1")
        ),
    );
    s.story(
        "ST-004",
        &format!(
            "title: Same score, created earlier\n{}\ncreated: 10",
            scored("ready", "4", "4", "0", "1")
        ),
    );
    s.story(
        "ST-005",
        &format!(
            "title: Fractional négatif 名前\n{}",
            scored("ready", "-1", "0", "0.7", "3")
        ),
    );
    s.story(
        "ST-006",
        &format!("title: Refined\n{}", scored("refined", "2", "1", "1", "3")),
    );
    s.story("ST-007", "title: Draft\nstatus: draft");
    s.story(
        "ST-008",
        &format!(
            "title: Blocked\n{}\ndepends_on: [ST-001, ST-404, ST-009, 7]",
            scored("ready", "9", "9", "9", "1")
        ),
    );
    s.story(
        "ST-009",
        &format!("title: Done dep\n{}", scored("done", "1", "1", "1", "1")),
    );
    s.story(
        "ST-010",
        &format!(
            "title: Unblocked\n{}\ndepends_on: [ST-009]",
            scored("ready", "1", "1", "1", "5")
        ),
    );
    s.story(
        "ST-011",
        "title: Unscored\nstatus: ready\nsize: 3\nvalue: 1",
    );
    s.story(
        "ST-012",
        &format!("title: Zero size\n{}", scored("ready", "1", "1", "1", "0")),
    );
    s.story(
        "ST-013",
        &format!(
            "title: Text value\n{}",
            scored("ready", "high", "1", "1", "1")
        ),
    );
    s.story("ST-014", "title: Split\nstatus: split");
    s.story("ST-015", "title: Archived\nstatus: archived");
    s.story(
        "ST-016",
        "status: ready\nvalue: 1\ntime_criticality: 2\nrisk_reduction: 3\nsize: 7",
    );
    s.story("ST-017", "title: No status");
    s.story(
        "ST-018",
        &format!(
            "title: Refined blocked\n{}\ndepends_on: [ST-007]",
            scored("refined", "1", "1", "1", "1")
        ),
    );
}

#[test]
fn every_argument_form_matches_node() {
    let s = Sandbox::new();
    mixed(&s);
    let plain = s.both_forms(&[], 0);
    assert!(plain.contains("Excluded:"), "{plain}");
    for args in [
        &["--include-refined"][..],
        &["--limit", "2"],
        &["--limit", "0"],
        &["--limit", "-1"],
        &["--limit", "1.9"],
        &["--limit", "abc"],
        &["--limit", ""],
        &["--limit", "0x2"],
        &["--limit", "99"],
        &["--limit", "2", "--include-refined"],
        &["--limit", "3", "--limit", "1"],
        &["ST-001"],
    ] {
        s.both_forms(args, 0);
    }
}

#[test]
fn an_empty_backlog_matches_node() {
    let s = Sandbox::new();
    let plain = s.both_forms(&[], 0);
    assert_eq!(plain, "No ready, unblocked, scored stories.\n");
    s.story("ST-001", "title: Draft\nstatus: draft");
    s.both_forms(&["--include-refined"], 0);
}

#[test]
fn errors_match_node() {
    let s = Sandbox::new();
    s.story(
        "ST-001",
        &format!("title: A\n{}", scored("ready", "1", "1", "1", "1")),
    );
    s.compare(&["--limit"]);
    s.story(
        "ST-002",
        "title: Scalar dependency\nstatus: ready\ndepends_on: ST-001",
    );
    s.both_forms(&[], 1);
    s.write(".story-time/settings.json", "{ not json");
    let out = s.compare_streams(&[], false);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("settings.json: "));
}

#[test]
fn settings_and_dir_choose_the_backlog() {
    let s = Sandbox::new();
    s.write(".story-time/settings.json", "{\"backlogDir\": \"work\"}");
    s.write(
        "work/ST-001.md",
        &format!(
            "---\nid: ST-001\ntitle: Elsewhere\n{}\n---\n",
            scored("ready", "1", "2", "3", "4")
        ),
    );
    s.both_forms(&[], 0);
    s.both_forms(&["--dir", "missing"], 0);
}
