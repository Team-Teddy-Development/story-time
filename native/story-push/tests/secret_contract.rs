#![cfg(unix)]

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const FAKE: &str = "test-only-secret-value";
const OLD_FAKE: &str = "test-only-old-secret";
const PROVIDERS: [(&str, &str, &str); 4] = [
    ("github", ".github.env", "GH_TOKEN"),
    ("ado", ".ado.env", "AZURE_DEVOPS_EXT_PAT"),
    ("gitea", ".gitea.env", "GITEA_TOKEN"),
    ("jev", ".jev.env", "TYPESAFE_API_KEY"),
];

type Tree = BTreeMap<PathBuf, (u32, Option<Vec<u8>>)>;

struct Case<'a> {
    args: &'a [&'a str],
    stdin: &'a str,
    env: &'a [(&'a str, &'a str)],
    git: bool,
    same_stderr: bool,
}

/// A temp project at a fixed path; Node runs first, then the tree is rebuilt and native runs at the same path.
struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-secret-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self(path)
    }
    fn reset(&self, setup: &dyn Fn(&Path)) {
        let _ = fs::remove_dir_all(&self.0);
        fs::create_dir_all(self.0.join("backlog")).unwrap();
        setup(&self.0);
    }
    fn tree(&self) -> Tree {
        fn walk(root: &Path, dir: &Path, into: &mut Tree) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
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
        walk(&self.0, &self.0, &mut tree);
        tree
    }
    fn run(&self, native: bool, case: &Case) -> Output {
        let path = std::env::var_os("PATH").unwrap();
        let mut cmd = if native {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_story"));
            cmd.env_clear().env(
                "PATH",
                if case.git {
                    path.as_os_str()
                } else {
                    "/nonexistent".as_ref()
                },
            );
            cmd
        } else {
            let mut cmd = Command::new("node");
            cmd.arg(legacy()).env_clear().env("PATH", path);
            cmd
        };
        let mut child = cmd
            .arg("secret")
            .args(case.args)
            .envs(case.env.iter().copied())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(case.stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
    /// Runs Node and then native on identical fixtures; requires equal stdout, stderr, exit code, and resulting tree.
    fn compare(&self, case: &Case, setup: &dyn Fn(&Path)) -> (Output, Tree) {
        self.reset(setup);
        let node = self.run(false, case);
        let node_tree = self.tree();
        self.reset(setup);
        let native = self.run(true, case);
        let native_tree = self.tree();
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        let args = case.args;
        assert_eq!(
            text(&native.stdout),
            text(&node.stdout),
            "stdout differs for {args:?}; native stderr: {}",
            text(&native.stderr)
        );
        if case.same_stderr {
            assert_eq!(
                text(&native.stderr),
                text(&node.stderr),
                "stderr differs for {args:?}"
            );
        } else {
            assert_eq!(native.stderr.is_empty(), node.stderr.is_empty());
        }
        assert_eq!(
            native.status.code(),
            node.status.code(),
            "exit code differs for {args:?}"
        );
        assert_eq!(native_tree, node_tree, "file tree differs for {args:?}");
        for out in [&node, &native] {
            for stream in [&out.stdout, &out.stderr] {
                let s = text(stream);
                assert!(
                    !s.contains(FAKE) && !s.contains(OLD_FAKE),
                    "secret printed for {args:?}: {s}"
                );
            }
        }
        (native, native_tree)
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

fn case<'a>(args: &'a [&'a str], stdin: &'a str) -> Case<'a> {
    Case {
        args,
        stdin,
        env: &[],
        git: false,
        same_stderr: true,
    }
}

fn write(root: &Path, rel: &str, text: &str, mode: u32) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
}

fn env_file(variable: &str, value: &str) -> String {
    format!("# story-time: tracker token. Never commit this file.\n{variable}={value}\n")
}

#[test]
fn set_writes_a_private_file_and_gitignore_like_node() {
    let s = Sandbox::new();
    for (name, file, variable) in PROVIDERS {
        let stdin = format!("{FAKE}\n");
        let (out, tree) = s.compare(&case(&["set", name], &stdin), &|_| {});
        assert_eq!(out.status.code(), Some(0));
        let (mode, bytes) = &tree[&PathBuf::from(".story-time").join(file)];
        assert_eq!(*mode, 0o600, "{name} file mode");
        assert_eq!(bytes.as_deref(), Some(env_file(variable, FAKE).as_bytes()));
        assert_eq!(
            tree[&PathBuf::from(".story-time/.gitignore")].1.as_deref(),
            Some(&b"# story-time: tracker tokens stay on this machine.\n*.env\n"[..])
        );
    }
}

#[test]
fn set_replaces_an_existing_token_and_keeps_or_extends_gitignore() {
    let s = Sandbox::new();
    let stdin = format!("  {FAKE}  \r\nsecond line\n");
    let setups: [(&str, &str); 4] = [
        ("*.env\n", "*.env\n"),
        ("node_modules\r\n*.env\r\n", "node_modules\r\n*.env\r\n"),
        ("node_modules\n\n  \n", "node_modules\n*.env\n"),
        ("# keep\n*.envx\n", "# keep\n*.envx\n*.env\n"),
    ];
    for (before, after) in setups {
        let (out, tree) = s.compare(&case(&["set", "gitea"], &stdin), &|root| {
            write(root, ".story-time/.gitignore", before, 0o644);
            write(
                root,
                ".story-time/.gitea.env",
                &env_file("GITEA_TOKEN", OLD_FAKE),
                0o644,
            );
        });
        assert_eq!(out.status.code(), Some(0));
        let (mode, bytes) = &tree[&PathBuf::from(".story-time/.gitea.env")];
        assert_eq!(*mode, 0o600);
        assert_eq!(
            bytes.as_deref(),
            Some(env_file("GITEA_TOKEN", FAKE).as_bytes())
        );
        assert_eq!(
            tree[&PathBuf::from(".story-time/.gitignore")].1.as_deref(),
            Some(after.as_bytes())
        );
    }
}

#[test]
fn status_reports_present_absent_loose_and_environment_like_node() {
    let s = Sandbox::new();
    let absent = |_: &Path| {};
    for args in [
        &["status"][..],
        &["status", "--json"],
        &["--json", "status", "github"],
    ] {
        s.compare(&case(args, ""), &absent);
    }
    let present = |root: &Path| {
        write(
            root,
            ".story-time/.github.env",
            &env_file("GH_TOKEN", FAKE),
            0o600,
        );
        write(
            root,
            ".story-time/.ado.env",
            "AZURE_DEVOPS_EXT_PAT = \"short\"\n",
            0o600,
        );
        write(
            root,
            ".story-time/.gitea.env",
            &env_file("GITEA_TOKEN", FAKE),
            0o644,
        );
        write(root, ".story-time/.jev.env", "# nothing\nOTHER=x\n", 0o600);
    };
    let env = [("GH_TOKEN", OLD_FAKE), ("TYPESAFE_API_KEY", OLD_FAKE)];
    for args in [&["status"][..], &["status", "--json"]] {
        let (out, _) = s.compare(&case(args, ""), &present);
        assert_eq!(out.status.code(), Some(0));
        s.compare(
            &Case {
                env: &env,
                ..case(args, "")
            },
            &present,
        );
    }
}

#[test]
fn status_flags_a_token_file_tracked_by_git_like_node() {
    let s = Sandbox::new();
    let tracked = |root: &Path| {
        write(
            root,
            ".story-time/.github.env",
            &env_file("GH_TOKEN", FAKE),
            0o600,
        );
        for args in [
            &["init", "-q"][..],
            &["add", "-f", ".story-time/.github.env"],
        ] {
            let ok = Command::new("git")
                .args(args)
                .current_dir(root)
                .env("HOME", root)
                .status()
                .unwrap();
            assert!(ok.success());
        }
    };
    let (out, _) = s.compare(
        &Case {
            git: true,
            ..case(&["status"], "")
        },
        &tracked,
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("is committed to git"));
}

#[test]
fn clear_removes_or_reports_like_node() {
    let s = Sandbox::new();
    for (name, file, variable) in PROVIDERS {
        let rel = format!(".story-time/{file}");
        let (out, tree) = s.compare(&case(&["clear", name], ""), &|root| {
            write(root, &rel, &env_file(variable, FAKE), 0o600);
        });
        assert_eq!(out.status.code(), Some(0));
        assert!(!tree.contains_key(&PathBuf::from(&rel)));
        s.compare(&case(&["clear", name], ""), &|_| {});
    }
}

#[test]
fn usage_and_value_failures_match_node() {
    let s = Sandbox::new();
    let stdin = format!("{FAKE}\n");
    let cases: [(&[&str], &str, i32); 10] = [
        (&[], "", 2),
        (&["rotate", "github"], "", 2),
        (&["set"], "", 2),
        (&["clear"], "", 2),
        (&["set", "jira"], &stdin, 2),
        (&["clear", "linear"], "", 2),
        (&["set", "github", "inline-arg"], &stdin, 2),
        (&["set", "github"], "", 1),
        (&["set", "ado"], "\n\n", 1),
        (&["set", "jev"], "test-only secret value\n", 1),
    ];
    for (args, stdin, code) in cases {
        let (out, tree) = s.compare(&case(args, stdin), &|_| {});
        assert_eq!(out.status.code(), Some(code), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert_eq!(tree.len(), 1, "{args:?} changed files: {tree:?}");
    }
    s.compare(&case(&["status", "--config"], ""), &|_| {});
    let invalid_json = Case {
        same_stderr: false,
        ..case(&["status"], "")
    };
    s.compare(&invalid_json, &|root| {
        write(root, ".story-time/settings.json", "{ not json", 0o644)
    });
}
