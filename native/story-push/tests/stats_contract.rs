use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-stats-{}-{}",
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
    fn story(&self, relative: &str, frontmatter: &str) -> &Self {
        self.write(
            relative,
            &format!("---\n{frontmatter}\n---\n\n## Story\nAs a user, I want x, so that y.\n"),
        )
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
        cmd.arg("stats")
            .args(args)
            .envs(env.iter().copied())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn compare(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let before = self.files();
        let node = self.run(false, args, env);
        assert_eq!(before, self.files(), "Node changed files for {args:?}");
        let native = self.run(true, args, env);
        assert_eq!(before, self.files(), "native changed files for {args:?}");
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&node.stdout),
            "stdout differs for {args:?} {env:?}; native stderr: {}; node stderr: {}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr)
        );
        assert_eq!(
            native.status.code(),
            node.status.code(),
            "exit code differs for {args:?} {env:?}; native stderr: {}; node stderr: {}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr)
        );
        native
    }
    fn both_forms(&self, args: &[&str], env: &[(&str, &str)], code: i32) {
        for extra in [&[][..], &["--json"][..]] {
            let all: Vec<&str> = args.iter().chain(extra).copied().collect();
            let out = self.compare(&all, env);
            assert_eq!(out.status.code(), Some(code), "{all:?} {env:?} exit code");
            if code != 0 {
                assert!(
                    out.stdout.is_empty(),
                    "{all:?} printed to stdout on failure"
                );
            }
        }
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

fn rich_backlog(s: &Sandbox) {
    for (id, rest) in [
        ("ST-001", "status: draft\nsize: 3\nupdated: 2000-01-01"),
        ("ST-002", "status: ready\nsize: 5\nupdated: 2999-12-31"),
        ("ST-003", "status: ready\nsize: 0.1\nupdated: garbage"),
        ("ST-004", "status: ready\nsize: 0.2\nupdated: 2000"),
        (
            "ST-005",
            "status: in-progress\nsize: \"8\"\nupdated: 2000-01-01T10:00",
        ),
        (
            "ST-006",
            "status: in-progress\ntype: spike\nupdated: 2000-01-01T10:00+05:30",
        ),
        ("ST-007", "status: draft\ntype: bug\nupdated: 2000-02-31"),
        ("ST-008", "status: done\nsize: 13\nupdated: 2000-01-01"),
        ("ST-009", "status: split\nupdated: 2000-01-01"),
        ("ST-010", "status: archived\nsize: 2"),
        ("ST-011", "size: [1, 2]\nupdated: +002000-01-01"),
        (
            "ST-012",
            "status: 10\nsize: 1e3\nupdated: 2000-01-01T24:00Z",
        ),
        ("ST-013", "status: 2\nupdated: 2000-13-01"),
        (
            "ST-014",
            "status: Done\nsize: -0\nupdated: 2000-01-01T10:00:00.5Z",
        ),
        ("ST-015", "status: [a, b]\nupdated: 0"),
        ("ST-016", "status: draft\nupdated: [2000-01-01]"),
        ("ST-017", "status: draft\nupdated: 2000-11"),
        ("ST-018", "status: draft\nupdated: 2000-01-01Z"),
        ("ST-019", "status: draft\nupdated: 2000-01-01T10:60"),
        (
            "ST-020",
            "status: draft\nupdated: 2999-01-01T23:59:59.999-23:59",
        ),
        ("ST-021", "status: draft\nupdated: true"),
        ("ST-022", "status: draft\nupdated: \"\""),
    ] {
        s.story(
            &format!("backlog/{id}.md"),
            &format!("id: {id}\ntitle: {id}\n{rest}"),
        );
    }
    s.write("backlog/README.md", "# Backlog\n");
    s.write("backlog/EP-001.map.md", "# Map\n");
}

#[test]
fn summary_and_json_match_node() {
    let s = Sandbox::new();
    rich_backlog(&s);
    s.both_forms(&[], &[], 0);
    s.both_forms(&["positional", "--unknown", "value", "--all"], &[], 0);
}

#[test]
fn stale_days_values_match_node() {
    let s = Sandbox::new();
    rich_backlog(&s);
    for days in [
        "0",
        "30",
        "abc",
        "1e9",
        "-1",
        " 7 ",
        "0x10",
        "0b11",
        "-0x10",
        "Infinity",
        "-Infinity",
        "",
        " ",
        "100000",
        "1_0",
        ".5",
        "5.",
        "+5",
        "infinity",
        "1e400",
    ] {
        s.both_forms(&["--stale-days", days], &[], 0);
    }
    s.both_forms(&["--stale-days", "1", "--stale-days", "100000"], &[], 0);
}

#[test]
fn usage_errors_match_node() {
    let s = Sandbox::new();
    rich_backlog(&s);
    for args in [&["--stale-days"][..], &["--dir"][..], &["--config"][..]] {
        let out = s.compare(args, &[]);
        assert_eq!(out.status.code(), Some(2), "{args:?} is a usage error");
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn empty_missing_and_broken_backlogs_match_node() {
    let s = Sandbox::new();
    s.both_forms(&[], &[], 0);
    fs::create_dir_all(s.0.join("backlog")).unwrap();
    s.both_forms(&[], &[], 0);
    s.story("other/ST-001.md", "id: ST-001\ntitle: x\nstatus: done");
    s.both_forms(&["--dir", "other"], &[], 0);
    s.both_forms(&[], &[("STORY_TIME_DIR", "other")], 0);
    s.write("other/ST-002.md", "# no frontmatter\n");
    s.both_forms(&["--dir", "other"], &[], 1);
    s.write("file", "not a directory\n");
    s.both_forms(&["--dir", "file"], &[], 1);
    s.write(".story-time/settings.json", "{");
    s.both_forms(&[], &[], 1);
}

#[test]
fn missing_and_non_string_ids_match_node() {
    for (first, second, code) in [
        ("id: ST-1\nstatus: draft", "title: No id\nstatus: draft", 1),
        ("id: ST-1\nstatus: draft", "id: 5\nstatus: draft", 1),
        ("title: No id\nstatus: draft", "id: ST-1\nstatus: draft", 0),
    ] {
        let s = Sandbox::new();
        s.story("backlog/ST-001.md", first);
        s.story("backlog/ST-002.md", second);
        s.both_forms(&[], &[], code);
        let alone = Sandbox::new();
        alone.story("backlog/ST-001.md", second);
        alone.story("backlog/ST-002.md", "id: [a, b]\nstatus: draft");
        alone.both_forms(&["--dir", "missing"], &[], 0);
        let single = Sandbox::new();
        single.story(
            "backlog/ST-001.md",
            "title: No id\nstatus: draft\nupdated: 2000-01-01",
        );
        single.both_forms(&[], &[], 0);
    }
}
