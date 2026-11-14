use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Sandbox(PathBuf);
impl Sandbox {
    fn bare(tag: &str, settings: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("story-push-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::write(path.join(".story-time/settings.json"), settings).unwrap();
        path
    }
    fn empty(backlog: bool) -> Self {
        let path = Self::bare(
            if backlog { "empty" } else { "missing" },
            r#"{"tracker":"gitea","sync":"ask","gitea":{"baseUrl":"http://127.0.0.1:1","owner":"o","repo":"r"}}"#,
        );
        if backlog {
            fs::create_dir(path.join("backlog")).unwrap();
        }
        Self(path.canonicalize().unwrap())
    }
    fn new() -> Self {
        let path = Self::bare(
            "flags",
            r#"{"tracker":"github","sync":"ask","github":{"repo":"o/r"}}"#,
        );
        for (folder, title, remote) in [("backlog", "From default", 3), ("--x", "From dash dir", 4)]
        {
            fs::create_dir_all(path.join(folder)).unwrap();
            for i in 1..=2 {
                fs::write(
                    path.join(folder).join(format!("ST-00{i}.md")),
                    format!("---\nid: ST-00{i}\ntitle: {title} {i}\nstatus: draft\nremote: {remote}{i}\n---\n"),
                )
                .unwrap();
            }
        }
        Self(path.canonicalize().unwrap())
    }
    fn run(&self, native: bool, args: &[&str]) -> Output {
        let mut cmd = if native {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_story"));
            cmd.env_clear().env("PATH", "/nonexistent");
            cmd
        } else {
            let mut cmd = Command::new("node");
            cmd.arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../skills/story-time/scripts/story.mjs"),
            )
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap());
            cmd
        };
        cmd.args(args)
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn push_parses_flags_like_node() {
    let sandbox = Sandbox::new();
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let cases: &[(&[&str], Option<&str>)] = &[
        (
            &["push", "--dir", "--x", "--all", "--dry-run", "--json"],
            Some("\"remoteId\": \"41\""),
        ),
        (&["push", "--dry-run", "--dir", "--x", "ST-002"], Some("42")),
        (&["push", "--all", "ST-001", "--dry-run"], None),
        (&["push", "--label", "ST-001", "--all", "--dry-run"], None),
        (&["push", "--dryrun", "ST-001"], None),
        (&["push", "--all", "--dry-run", "--dir=x", "y"], None),
        (&["push", "--dry-run", "--dir"], None),
        (&["push", "--dry-run"], None),
    ];
    let refusals: &[(&[&str], &str)] = &[
        (
            &["push", "--all", "ST-001", "--dry-run"],
            "Use --all or explicit story IDs, not both.",
        ),
        (
            &["push", "--label", "ST-001", "--all", "--dry-run"],
            "Unknown push flag --label.",
        ),
        (
            &["push", "--dryrun", "ST-001"],
            "Unknown push flag --dryrun.",
        ),
        (
            &["push", "--all", "--dry-run", "--dir=x", "y"],
            "Unknown push flag --dir=x.",
        ),
    ];
    for (args, expected) in cases {
        let node = sandbox.run(false, args);
        let native = sandbox.run(true, args);
        assert_eq!(
            (
                text(&native.stdout),
                text(&native.stderr),
                native.status.code()
            ),
            (text(&node.stdout), text(&node.stderr), node.status.code()),
            "{args:?} differs from Node"
        );
        match expected {
            Some(needle) => {
                assert_eq!(
                    node.status.code(),
                    Some(0),
                    "{args:?}: {}",
                    text(&node.stderr)
                );
                assert!(
                    text(&node.stdout).contains(needle),
                    "{args:?}: {}",
                    text(&node.stdout)
                );
            }
            None => assert_eq!(node.status.code(), Some(2), "{args:?}"),
        }
        if let Some(refusal) = refusals.iter().find(|(a, _)| a == args) {
            assert_eq!(text(&node.stderr), format!("{}\n", refusal.1), "{args:?}");
        }
    }
}

#[test]
fn push_all_on_an_empty_or_missing_backlog_agrees_with_node() {
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let cases: &[(&[&str], i32, &str)] = &[
        (&["push", "--all", "--dry-run"], 0, "Action  ID  Remote\n"),
        (
            &["push", "--all", "--dry-run", "--json"],
            0,
            "{\n  \"ok\": true,\n  \"plan\": []\n}\n",
        ),
        (&["push", "--all"], 0, "Nothing to push.\n"),
        (
            &["push", "--all", "--json"],
            0,
            "{\n  \"ok\": true,\n  \"pushed\": [],\n  \"skipped\": []\n}\n",
        ),
        (&["push", "--dry-run"], 2, ""),
        (&["push", "--json"], 2, ""),
    ];
    for backlog in [true, false] {
        let sandbox = Sandbox::empty(backlog);
        for (args, code, stdout) in cases {
            let node = sandbox.run(false, args);
            let native = sandbox.run(true, args);
            let seen = |o: &Output| (text(&o.stdout), text(&o.stderr), o.status.code());
            assert_eq!(seen(&native), seen(&node), "backlog={backlog} {args:?}");
            let expected_stderr = if *code == 2 {
                "Usage: story push <id...> | --all [--dry-run]\n"
            } else {
                ""
            };
            assert_eq!(
                seen(&node),
                (
                    (*stdout).to_owned(),
                    expected_stderr.to_owned(),
                    Some(*code)
                ),
                "backlog={backlog} {args:?}"
            );
            assert_eq!(
                fs::read_dir(sandbox.0.join(".story-time")).unwrap().count(),
                1,
                "backlog={backlog} {args:?} wrote under .story-time"
            );
            assert_eq!(sandbox.0.join("backlog").exists(), backlog);
        }
    }
}
