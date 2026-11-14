use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("story-push-flags-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::write(
            path.join(".story-time/settings.json"),
            r#"{"tracker":"github","sync":"ask","github":{"repo":"o/r"}}"#,
        )
        .unwrap();
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
