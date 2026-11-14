use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

type Snapshot = BTreeMap<PathBuf, (u32, Vec<u8>)>;

/// A fixture built once in `template`, copied byte for byte into `run` before each CLI run, so
/// Node and native see the same tree (git objects included) at the same path.
struct Sandbox {
    root: PathBuf,
    template: PathBuf,
    run: PathBuf,
    git_only: PathBuf,
}
impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "story-new-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let template = root.join("template");
        let git_only = root.join("bin");
        fs::create_dir_all(&template).unwrap();
        fs::create_dir_all(&git_only).unwrap();
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|dir| dir.join("git"))
            .find(|p| p.is_file())
            .expect("git on PATH");
        symlink(git, git_only.join("git")).unwrap();
        let root = root.canonicalize().unwrap();
        Self {
            template: root.join("template"),
            run: root.join("project"),
            git_only: root.join("bin"),
            root,
        }
    }
    fn write(&self, relative: &str, text: &str) -> &Self {
        let path = self.template.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }
    fn story(&self, relative: &str, id: &str) -> &Self {
        self.write(
            relative,
            &format!("---\nid: {id}\ntitle: Existing {id}\nstatus: draft\ntype: story\n---\n\n## Story\nAs a user, I want x, so that y.\n"),
        )
    }
    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.template)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }
    fn reset(&self) {
        let _ = fs::remove_dir_all(&self.run);
        copy_tree(&self.template, &self.run);
    }
    fn snapshot(&self) -> Snapshot {
        fn walk(base: &Path, dir: &Path, into: &mut Snapshot) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let meta = fs::symlink_metadata(&path).unwrap();
                let relative = path.strip_prefix(base).unwrap().to_owned();
                let mode = meta.permissions().mode();
                if meta.is_dir() {
                    into.insert(relative, (mode, Vec::new()));
                    walk(base, &path, into);
                } else {
                    into.insert(relative, (mode, fs::read(&path).unwrap()));
                }
            }
        }
        let mut files = Snapshot::new();
        walk(&self.run, &self.run, &mut files);
        files
    }
    fn exec(&self, native: bool, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut cmd = if native {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_story"));
            cmd.env_clear().env("PATH", &self.git_only);
            cmd
        } else {
            let mut cmd = Command::new("node");
            cmd.arg(legacy())
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap());
            cmd
        };
        cmd.arg("new")
            .args(args)
            .envs(env.iter().copied())
            .env("HOME", &self.root)
            .current_dir(&self.run)
            .output()
            .unwrap()
    }
    /// Runs Node and native on fresh copies of the fixture and asserts identical stdout, stderr,
    /// exit code, and resulting tree. A run that straddles a UTC midnight is repeated.
    fn compare(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        loop {
            let day = today();
            self.reset();
            let node = self.exec(false, args, env);
            let node_tree = self.snapshot();
            self.reset();
            let native = self.exec(true, args, env);
            let native_tree = self.snapshot();
            if day != today() {
                continue;
            }
            let context = format!(
                "{args:?} {env:?}; native stderr: {}; node stderr: {}",
                String::from_utf8_lossy(&native.stderr),
                String::from_utf8_lossy(&node.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&native.stdout),
                String::from_utf8_lossy(&node.stdout),
                "stdout differs for {context}"
            );
            assert_eq!(
                String::from_utf8_lossy(&native.stderr),
                String::from_utf8_lossy(&node.stderr),
                "stderr differs for {context}"
            );
            assert_eq!(
                native.status.code(),
                node.status.code(),
                "exit code differs for {context}"
            );
            assert_eq!(
                printable(&native_tree),
                printable(&node_tree),
                "file changes differ for {context}"
            );
            assert_eq!(native_tree, node_tree, "file modes differ for {context}");
            return native;
        }
    }
    fn both_forms(&self, args: &[&str], env: &[(&str, &str)]) -> String {
        let text = self.compare(args, env);
        assert_eq!(text.status.code(), Some(0), "{args:?} {env:?} exit code");
        let all: Vec<&str> = args.iter().copied().chain(["--json"]).collect();
        assert_eq!(self.compare(&all, env).status.code(), Some(0));
        String::from_utf8(text.stdout).unwrap()
    }
    fn fails(&self, args: &[&str], env: &[(&str, &str)], code: i32) {
        let out = self.compare(args, env);
        assert_eq!(out.status.code(), Some(code), "{args:?} {env:?} exit code");
        assert!(out.stdout.is_empty(), "{args:?} printed to stdout");
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    fs::set_permissions(to, fs::metadata(from).unwrap().permissions()).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}

fn printable(tree: &Snapshot) -> BTreeMap<&Path, String> {
    tree.iter()
        .filter(|(path, _)| !path.starts_with(".git"))
        .map(|(path, (_, bytes))| (path.as_path(), String::from_utf8_lossy(bytes).into_owned()))
        .collect()
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn legacy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs")
}

/// main holds ST-001 and ST-002 (plus an untracked ST-004 in the working tree); branch
/// `feature` holds ST-009; remote-tracking `origin/wide` holds ST-012 and PAY-040; a tag-only
/// commit holds ST-050, which allocation must ignore.
fn git_backlog() -> Sandbox {
    let s = Sandbox::new();
    s.git(&["init", "-q", "-b", "main"]);
    s.write(
        ".story-time/settings.json",
        r#"{"backlogDir":"docs/backlog"}"#,
    );
    s.story("docs/backlog/ST-001.md", "ST-001")
        .story("docs/backlog/ST-002.md", "ST-002")
        .write("docs/backlog/README.md", "# Backlog\n");
    s.commit("base");
    s.git(&["checkout", "-q", "-b", "feature"]);
    s.story("docs/backlog/ST-009.md", "ST-009");
    s.commit("feature");
    s.git(&["checkout", "-q", "-b", "wide", "main"]);
    s.story("docs/backlog/ST-012.md", "ST-012")
        .story("docs/backlog/PAY-040.md", "PAY-040");
    s.commit("wide");
    s.git(&["update-ref", "refs/remotes/origin/wide", "HEAD"]);
    s.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/wide",
    ]);
    s.git(&["checkout", "-q", "-b", "tagged", "main"]);
    s.story("docs/backlog/ST-050.md", "ST-050");
    s.commit("tagged");
    s.git(&["tag", "v1"]);
    s.git(&["checkout", "-q", "main"]);
    s.git(&["branch", "-q", "-D", "wide", "tagged"]);
    s.story("docs/backlog/ST-004.md", "ST-004");
    s
}

#[test]
fn allocation_sees_ids_on_other_branches_and_matches_node() {
    let s = git_backlog();
    let out = s.both_forms(&["--title", "Native port"], &[]);
    assert!(out.starts_with("ST-013 created: "), "{out}");
    let out = s.both_forms(&["--title", "Payment", "--prefix", "PAY"], &[]);
    assert!(out.starts_with("PAY-041 created: "), "{out}");
    s.both_forms(&["--title", "From env"], &[("STORY_TIME_PREFIX", "PAY")]);
    s.both_forms(&["--title", "New prefix", "--prefix", "NEW"], &[]);
    s.both_forms(&["--title", "Other dir", "--dir", "elsewhere/backlog"], &[]);
}

#[test]
fn every_flag_matches_node() {
    let s = git_backlog();
    s.both_forms(
        &[
            "--title",
            "Full story",
            "--statement",
            "As a buyer, I want receipts, so that I can expense them.",
            "--ac",
            "Given a paid order, when I open it, then I see a receipt",
            "--ac",
            "Given no order, when I open receipts, then I see an empty list",
            "--type",
            "bug",
            "--epic",
            "EP-001",
            "--persona",
            "buyer",
            "--size",
            "3",
            "--value",
            "8",
            "--time-criticality",
            "5",
            "--risk-reduction",
            "2",
            "--timebox",
            "2d",
            "--question",
            "ignored outside a spike",
        ],
        &[],
    );
    for kind in ["story", "bug", "chore"] {
        s.both_forms(&["--title", "Typed", "--type", kind], &[]);
    }
    s.both_forms(
        &[
            "--title",
            "Spike it",
            "--type",
            "spike",
            "--question",
            "Which queue?",
            "--ac",
            "Decision recorded",
            "--ac",
            "Prototype thrown away",
            "--statement",
            "ignored for a spike",
            "--timebox",
            "1d",
        ],
        &[],
    );
    s.both_forms(&["--title", "Bare spike", "--type", "spike"], &[]);
    s.both_forms(
        &[
            "--title",
            "Later flag wins",
            "--epic",
            "EP-1",
            "--epic",
            "EP-2",
        ],
        &[],
    );
}

#[test]
fn values_are_quoted_and_numbers_converted_like_node() {
    let s = git_backlog();
    s.both_forms(
        &[
            "--title",
            "  Spaces: and # hash  ",
            "--epic",
            "true",
            "--persona",
            "042",
            "--statement",
            "  padded statement  \n\n",
            "--ac",
            "",
            "--ac",
            "[bracketed], \"quoted\"",
        ],
        &[],
    );
    s.both_forms(&["--title", "Empty statement", "--statement", ""], &[]);
    s.both_forms(
        &[
            "--title",
            "Numbers",
            "--size",
            "abc",
            "--value",
            "",
            "--time-criticality",
            "0x10",
            "--risk-reduction",
            " 1e3 ",
        ],
        &[],
    );
    s.both_forms(
        &[
            "--title",
            "More numbers",
            "--size",
            "-0",
            "--value",
            "Infinity",
            "--time-criticality",
            ".5",
            "--risk-reduction",
            "-1.50",
        ],
        &[],
    );
    s.both_forms(
        &[
            "--title",
            "Odd numbers",
            "--size",
            "1_000",
            "--value",
            "0b101",
            "--time-criticality",
            "5.",
            "--risk-reduction",
            "inf",
        ],
        &[],
    );
    s.both_forms(
        &["--title", "Unicode \u{1F600} title", "--epic", "épique"],
        &[],
    );
}

#[test]
fn failures_match_node() {
    let s = git_backlog();
    s.fails(&[], &[], 1);
    s.fails(&["--epic", "EP-1"], &[], 1);
    s.fails(&["--title", "   "], &[], 1);
    s.fails(&["--title", "x", "--type", "epic"], &[], 1);
    s.fails(&["--title", "x", "--type", ""], &[], 1);
    s.fails(&["--title"], &[], 2);
    s.fails(&["--title", "x", "--ac"], &[], 2);
    s.fails(&["--title", "x", "--prefix", "st"], &[], 1);
    s.fails(&["--title", "x", "--prefix", ""], &[], 1);
    s.fails(&["--title", "x"], &[("STORY_TIME_PREFIX", "")], 1);
}

#[test]
fn backlogs_outside_git_match_node() {
    let s = Sandbox::new();
    s.write(".story-time/settings.json", r#"{"prefix":"REC"}"#)
        .story("backlog/REC-005.md", "REC-005")
        .story("backlog/ST-099.md", "ST-099")
        .write("backlog/REC-007.txt", "not a story\n")
        .write("backlog/rec-010.md", "lowercase\n");
    let out = s.both_forms(&["--title", "Outside git"], &[]);
    assert!(out.starts_with("REC-006 created: "), "{out}");
    s.both_forms(&["--title", "Fresh", "--dir", "fresh/nested"], &[]);
    s.both_forms(
        &["--title", "Configured elsewhere"],
        &[("STORY_TIME_DIR", "alt")],
    );
}
