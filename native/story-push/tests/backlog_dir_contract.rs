use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const COMMANDS: &[&[&str]] = &[
    &["show", "ST-001"],
    &["export", "ST-001"],
    &["list"],
    &["lint"],
    &["stats"],
    &["rank"],
    &["push", "--all", "--dry-run", "--json"],
];

struct Sandbox(PathBuf);
impl Sandbox {
    fn new(backlog_dir: Option<&str>) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-backlog-dir-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".story-time")).unwrap();
        let dir = backlog_dir.map_or(String::new(), |d| format!(r#","backlogDir":"{d}""#));
        fs::write(
            path.join(".story-time/settings.json"),
            format!(r#"{{"tracker":"github","sync":"ask","github":{{"repo":"o/r"}}{dir}}}"#),
        )
        .unwrap();
        for (n, (folder, title)) in [
            ("backlog", "From default"),
            ("stories", "From config"),
            ("flag", "From flag"),
            ("env", "From env"),
            ("", "From root"),
        ]
        .into_iter()
        .enumerate()
        {
            fs::create_dir_all(path.join(folder)).unwrap();
            for i in 1..=n + 1 {
                fs::write(
                    path.join(folder).join(format!("ST-00{i}.md")),
                    format!("---\nid: ST-00{i}\ntitle: {title} {i}\nstatus: draft\nremote: {n}{i}\n---\n"),
                )
                .unwrap();
            }
        }
        Self(path.canonicalize().unwrap())
    }
    fn run(&self, native: bool, args: &[&str], env: &[(&str, &str)]) -> Output {
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
            .envs(env.iter().copied())
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
fn every_command_resolves_the_backlog_directory_like_node() {
    let configured = Sandbox::new(Some("stories"));
    let default = Sandbox::new(None);
    let cases: &[(&Sandbox, &[&str], &[(&str, &str)], &str)] = &[
        (&default, &[], &[], "From default"),
        (&configured, &[], &[], "From config"),
        (&configured, &[], &[("STORY_TIME_DIR", "env")], "From env"),
        (&configured, &[], &[("STORY_TIME_DIR", "")], "From root"),
        (
            &configured,
            &["--dir", "flag"],
            &[("STORY_TIME_DIR", "env")],
            "From flag",
        ),
        (&configured, &["--dir", "./env/../flag"], &[], "From flag"),
    ];
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    for (sandbox, dir, env, title) in cases {
        for command in COMMANDS {
            let args: Vec<&str> = command.iter().chain(dir.iter()).copied().collect();
            let node = sandbox.run(false, &args, env);
            let native = sandbox.run(true, &args, env);
            assert_eq!(
                (
                    text(&native.stdout),
                    text(&native.stderr),
                    native.status.code()
                ),
                (text(&node.stdout), text(&node.stderr), node.status.code()),
                "{args:?} {env:?} differs from Node"
            );
            assert_eq!(
                node.status.code(),
                Some(if command[0] == "lint" { 1 } else { 0 }),
                "{args:?} {env:?}: {}",
                text(&node.stderr)
            );
            if ["show", "export", "list"].contains(&command[0]) {
                assert!(
                    text(&node.stdout).contains(title),
                    "{args:?} {env:?} not {title}"
                );
            }
        }
    }
}
