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
            "story-help-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        let sandbox = Self(path.canonicalize().unwrap());
        sandbox.write(
            "backlog/ST-001.md",
            "---\nid: ST-001\ntitle: A story\nstatus: ready\n---\n## Story\nAs a user, I want X.\n",
        );
        sandbox
    }
    fn write(&self, relative: &str, text: &str) -> &Self {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }
    fn files(&self) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
        fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let mode = fs::metadata(&path).unwrap().permissions().mode();
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
        cmd.args(args)
            .envs(env.iter().copied())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    /// Runs Node and native on the same tree and requires equal stdout, exit code, and files.
    /// Stderr must match too unless `same_stderr` is false, where the message comes from V8.
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
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn legacy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs")
}

#[test]
fn prints_the_usage_like_node() {
    let s = Sandbox::new();
    for args in [
        &[][..],
        &["help"][..],
        &["help", "--json"][..],
        &["help", "extra", "args", "--status", "done"][..],
        &["help", "--dir", "missing", "--force"][..],
        &["help", "--config", "nowhere/settings.json"][..],
    ] {
        let out = s.compare(args, &[], true);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stdout).starts_with("story — agile backlog toolkit"),
            "{args:?}"
        );
    }
    s.write(
        ".story-time/settings.json",
        r#"{"backlogDir":"stories","prefix":"PAY"}"#,
    );
    let out = s.compare(&["help"], &[("STORY_TIME_DIR", "elsewhere")], true);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn failures_match_node() {
    let s = Sandbox::new();
    for args in [&["help", "--dir"][..], &["help", "--status"][..]] {
        let out = s.compare(args, &[], true);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty());
    }
    for text in [r#"{"colour":"red"}"#, r#"{"backlogDir":3}"#] {
        s.write(".story-time/settings.json", text);
        for args in [&[][..], &["help"][..]] {
            let out = s.compare(args, &[], true);
            assert_eq!(out.status.code(), Some(1), "{text} {args:?}");
            assert!(out.stdout.is_empty());
        }
    }
    s.write(".story-time/settings.json", "{");
    let out = s.compare(&["help"], &[], false);
    assert_eq!(out.status.code(), Some(1));
    s.write("alt.json", r#"{"tracker":"nope"}"#);
    let out = s.compare(&["help", "--config", "alt.json"], &[], true);
    assert_eq!(out.status.code(), Some(1));
    let out = s.compare(&["help"], &[("STORY_TIME_CONFIG", "alt.json")], true);
    assert_eq!(out.status.code(), Some(1));
}
