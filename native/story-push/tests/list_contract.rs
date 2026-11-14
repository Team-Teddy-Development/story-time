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
            "story-list-{}-{}",
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
        cmd.arg("list")
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

fn rich_backlog(s: &Sandbox, dir: &str) {
    s.story(
        &format!("{dir}/ST-010.md"),
        "id: ST-010\ntitle: Tenth\nstatus: ready\ntype: story\nsize: 5",
    );
    s.story(&format!("{dir}/ST-002.md"), "id: ST-002\ntitle: Second, with \"quotes\" \u{1F600}\nstatus: done\nsize: 3\ndepends_on: [ST-001, ST-010]");
    s.story(
        &format!("{dir}/ST-001.md"),
        "id: ST-001\ntitle: First\nstatus: draft\nvalue: 8\ncreated: 2026-11-14",
    );
    s.story(
        &format!("{dir}/ST-9.md"),
        "id: ST-9\ntitle: Nine without padding\nstatus: archived\nsize: 13",
    );
    s.story(
        &format!("{dir}/ST-02.md"),
        "id: ST-02\ntitle: Ties with ST-002\nstatus: archived",
    );
    s.story(
        &format!("{dir}/PAY-1.md"),
        "id: PAY-1\ntitle: Another prefix\nstatus: in-progress\nsize: [1, 2]",
    );
    s.story(
        &format!("{dir}/AB1-3.md"),
        "id: AB1-3\ntitle: Digits in prefix\nstatus: split\nsize: \"L\"",
    );
    s.story(
        &format!("{dir}/AB10-1.md"),
        "id: AB10-1\ntitle: Bigger prefix number\nstatus: refined",
    );
    s.story(
        &format!("{dir}/EP-001.md"),
        "id: EP-001\ntitle: An epic story file\nstatus: draft",
    );
    s.write(
        &format!("{dir}/EP-001.map.md"),
        "# Epic map\n\nNot a story.\n",
    );
    s.write(&format!("{dir}/README.md"), "# Backlog\n");
    s.write(&format!("{dir}/notes.md"), "no frontmatter at all\n");
    s.write(&format!("{dir}/st-004.md"), "lowercase name is not an id\n");
    s.write(&format!("{dir}/ST-003.txt"), "not markdown\n");
    s.write(&format!("{dir}/ST-005.md.bak"), "backup\n");
    s.write(&format!("{dir}/nested/ST-006.md"), "---\nid: ST-006\n---\n");
}

#[test]
fn default_and_json_output_match_node() {
    let s = Sandbox::new();
    rich_backlog(&s, "backlog");
    s.both_forms(&[], &[], 0);
    s.both_forms(&["positional", "--unknown", "value", "--all"], &[], 0);
}

#[test]
fn status_filter_matches_node() {
    let s = Sandbox::new();
    rich_backlog(&s, "backlog");
    for status in [
        "archived",
        "done",
        "draft",
        "in-progress",
        "bogus",
        "",
        "Done",
    ] {
        s.both_forms(&["--status", status], &[], 0);
    }
    s.both_forms(&["--status", "draft", "--status", "archived"], &[], 0);
}

#[test]
fn usage_errors_match_node() {
    let s = Sandbox::new();
    rich_backlog(&s, "backlog");
    for args in [&["--status"][..], &["--dir"][..], &["--config"][..]] {
        let out = s.compare(args, &[]);
        assert_eq!(out.status.code(), Some(2), "{args:?} is a usage error");
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn empty_and_missing_backlogs_match_node() {
    let s = Sandbox::new();
    s.both_forms(&[], &[], 0);
    fs::create_dir_all(s.0.join("backlog")).unwrap();
    s.both_forms(&[], &[], 0);
    s.write("backlog/EP-001.map.md", "# Map\n");
    s.write("backlog/README.md", "# Backlog\n");
    s.both_forms(&[], &[], 0);
    s.both_forms(&["--status", "draft"], &[], 0);
}

#[test]
fn a_backlog_path_that_is_a_file_fails_like_node() {
    let s = Sandbox::new();
    s.write("backlog", "not a directory\n");
    s.both_forms(&[], &[], 1);
    s.write(
        "stories/ST-001.md/inner.md",
        "a directory named like a story\n",
    );
    s.both_forms(&["--dir", "stories"], &[], 1);
}

#[test]
fn backlog_directory_precedence_matches_node() {
    let s = Sandbox::new();
    s.story(
        "backlog/ST-001.md",
        "id: ST-001\ntitle: Default dir\nstatus: draft",
    );
    s.story(
        "flag/ST-002.md",
        "id: ST-002\ntitle: Flag dir\nstatus: draft",
    );
    s.story("env/ST-003.md", "id: ST-003\ntitle: Env dir\nstatus: draft");
    s.story(
        "configured/ST-004.md",
        "id: ST-004\ntitle: Config dir\nstatus: draft",
    );
    s.story("ST-005.md", "id: ST-005\ntitle: Working dir\nstatus: draft");
    s.both_forms(&[], &[], 0);
    s.both_forms(&["--dir", "flag"], &[], 0);
    s.both_forms(&["--dir", "flag/../env"], &[], 0);
    s.both_forms(&["--dir", s.0.join("flag").to_str().unwrap()], &[], 0);
    s.both_forms(&[], &[("STORY_TIME_DIR", "env")], 0);
    s.both_forms(&[], &[("STORY_TIME_DIR", "")], 0);
    s.both_forms(&["--dir", ""], &[], 0);
    s.both_forms(&["--dir", "flag"], &[("STORY_TIME_DIR", "env")], 0);
    s.write(
        ".story-time/settings.json",
        r#"{"backlogDir":"configured"}"#,
    );
    s.both_forms(&[], &[], 0);
    s.both_forms(&[], &[("STORY_TIME_DIR", "env")], 0);
    s.both_forms(&["--dir", "flag"], &[("STORY_TIME_DIR", "env")], 0);
    s.write("conf/settings.json", r#"{"backlogDir":"../flag"}"#);
    s.both_forms(&["--config", "conf/settings.json"], &[], 0);
    s.both_forms(&[], &[("STORY_TIME_CONFIG", "conf/settings.json")], 0);
    s.write(".story-time/settings.json", r#"{"backlogDir":3}"#);
    s.both_forms(&[], &[], 1);
    s.write(".story-time/settings.json", "{");
    s.both_forms(&[], &[], 1);
}

#[test]
fn lenient_frontmatter_matches_node() {
    let s = Sandbox::new();
    s.write(
        "backlog/ST-001.md",
        "---\nid: ST-001\ntitle: Values # with a comment\nstatus: draft\nsize:\nsize: 007\n\
         value: -0\ntime_criticality: 1.50\nrisk_reduction: 12345678901234567890123\n\
         timebox: 0.0000001\nflag: true\nother: false\nnothing: ~\nalso: null\n\
         list: [a, , 'b''c', \"d\\u00e9\", 3, [x], true]\nempty: []\nquoted: \"a: b # c\"\n\
         single: 'it''s'\nraw: a: b\ntrailing: \"x\"  \nsections: overwritten\n\
         # a comment line\n\n__proto__: [polluted]\ntitle_2: second\n---\n\
         Preamble outside any section.\n## Story\n  Body  \n## 10\nten\n## 2\ntwo\n## b\nbee\n\
         ##  spaced name  \nx\n###  not a heading\n## Story\nreplaced\n## __proto__\nhidden\n",
    );
    s.write(
        "backlog/ST-002.md",
        "---\r\nid: ST-002\r\ntitle: CRLF file\r\nstatus: ready\r\n---\r\n## Story\r\nline\r\n",
    );
    s.write(
        "backlog/ST-003.md",
        "---\nid: ST-003\ntitle: Closing fence glued\nstatus: done\n---## Story\nx\n",
    );
    s.write(
        "backlog/ST-004.md",
        "---\nid: ST-004\ntitle: No body\nstatus: done\n---",
    );
    s.write("backlog/ST-005.md", "---\nid: ST-005\ntitle: \u{1F680} astral \u{00e9}\u{0301} wide\nstatus: draft\nsize: 1\n---\n");
    s.write(
        "backlog/ST-006.md",
        "---\nid: ST-006\ntitle:  \nstatus: draft\n---\n",
    );
    s.write("backlog/ST-007.md", "---\nid: ST-007\ntitle: Huge\nstatus: draft\nsize: 1000000000000000000000\nvalue: 9007199254740993\n---\n");
    s.both_forms(&[], &[], 0);
    s.both_forms(&["--status", "draft"], &[], 0);
}

#[test]
fn malformed_stories_fail_like_node() {
    for (name, text, code) in [
        ("no frontmatter", "# Just a heading\n", 1),
        ("unclosed frontmatter", "---\nid: ST-001\ntitle: x\n", 1),
        ("bad key line", "---\nid: ST-001\nnot a key line\n---\n", 1),
        ("key with space", "---\nid: ST-001\nmy key: x\n---\n", 1),
        (
            "bad quoted JSON",
            "---\nid: ST-001\ntitle: \"a\\q\"\n---\n",
            1,
        ),
        ("bom", "\u{FEFF}---\nid: ST-001\n---\n", 1),
        (
            "carriage return in value",
            "---\nid: ST-001\ntitle: a\rb\n---\n",
            1,
        ),
        ("empty file", "", 1),
    ] {
        let s = Sandbox::new();
        s.story(
            "backlog/ST-000.md",
            "id: ST-000\ntitle: Good\nstatus: draft",
        );
        s.write("backlog/ST-001.md", text);
        for args in [&[][..], &["--status", "archived"][..]] {
            for extra in [&[][..], &["--json"][..]] {
                let all: Vec<&str> = args.iter().chain(extra).copied().collect();
                let out = s.compare(&all, &[]);
                assert_eq!(out.status.code(), Some(code), "{name} {all:?}");
            }
        }
    }
}

#[test]
fn missing_and_non_string_ids_match_node() {
    for (first, second) in [
        ("title: No id\nstatus: draft", "id: ST-1\nstatus: draft"),
        ("id: ST-1\nstatus: draft", "title: No id\nstatus: draft"),
        ("id: ST-1\nstatus: draft", "id: 5\nstatus: draft"),
        ("id: ST-1\nstatus: draft", "id: [a, b]\nstatus: draft"),
        ("id: ST-1\nstatus: draft", "title: No id\nstatus: done"),
        ("id: 5\nstatus: draft", "id: ST-1\nstatus: draft"),
    ] {
        let s = Sandbox::new();
        s.story("backlog/ST-001.md", first);
        s.story("backlog/ST-002.md", second);
        for args in [&[][..], &["--status", "draft"][..]] {
            for extra in [&[][..], &["--json"][..]] {
                let all: Vec<&str> = args.iter().chain(extra).copied().collect();
                s.compare(&all, &[]);
            }
        }
        let alone = Sandbox::new();
        alone.story("backlog/ST-001.md", second);
        alone.both_forms(&[], &[], 0);
    }
}

#[test]
fn sort_order_matches_node() {
    let s = Sandbox::new();
    let alphabet: Vec<char> = "Aab Zz0919-_.~$:!()/+=\tx".chars().collect();
    let mut seed: u64 = 0x5eed;
    let mut next = |n: usize| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as usize) % n
    };
    let mut ids: Vec<String> = [
        "ST-1",
        "ST-001",
        "ST-01",
        "ST-10",
        "ST-9",
        "st-2",
        "St-2",
        "ST-2",
        "ST2",
        "ST_2",
        "ST 2",
        "ST-2a",
        "ST-2A",
        "ST-02a",
        "A-1",
        "a-1",
        "A1-1",
        "AB-1",
        "A-10",
        "A-9",
        "Z",
        "z",
        "x9y",
        "x09y",
        "x10y",
        "e-1",
        "f-1",
        "ST-99999999999999999999",
        "ST-100000000000000000000",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    while ids.len() < 60 {
        let len = 2 + next(6);
        let id: String = (0..len).map(|_| alphabet[next(alphabet.len())]).collect();
        let id = id.trim().to_string();
        if id.len() >= 2
            && !id.starts_with(['\'', '"', '['])
            && !id.contains(" #")
            && !id.chars().all(|c| c.is_ascii_digit())
        {
            ids.push(format!("k{id}"));
        }
    }
    for (index, id) in ids.iter().enumerate() {
        s.write(
            &format!("backlog/ST-{:03}.md", 999 - index),
            &format!(
                "---\nid: {}\nstatus: draft\ntitle: {index}\n---\n",
                serde_json::to_string(id).unwrap()
            ),
        );
    }
    s.both_forms(&[], &[], 0);
}
