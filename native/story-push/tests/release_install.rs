use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Sandbox {
    root: PathBuf,
    commit: String,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("story-install-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let source = root.join("source");
        for (path, text) in [
            ("skills/story-time/SKILL.md", "story-time committed\n"),
            (
                "skills/story-time/scripts/story.mjs",
                "console.log('cli');\n",
            ),
            ("skills/story-time/scripts/test/cli.test.mjs", "test\n"),
            ("skills/gap-sweep/SKILL.md", "gap-sweep committed\n"),
            ("native/story-push/Cargo.toml", "[package]\n"),
        ] {
            write(&source.join(path), text);
        }
        git(&source, &["init", "-q"]);
        git(&source, &["add", "."]);
        git(
            &source,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "commit",
                "-q",
                "-m",
                "c",
            ],
        );
        let commit = String::from_utf8(git(&source, &["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim()
            .to_string();
        write(
            &source.join("skills/gap-sweep/SKILL.md"),
            "uncommitted edit\n",
        );

        let fake = root.join("fake-bin/cargo");
        write(
            &fake,
            "#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in --path) p=\"$2\"; shift;; --root) r=\"$2\"; shift;; esac; shift; done\nmkdir -p \"$r/bin\" && printf 'built from %s\\n' \"$p\" > \"$r/bin/story\"\n",
        );
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();

        let home = root.join("home");
        write(&home.join(".local/bin/story"), "old binary\n");
        write(
            &home.join(".config/story-time/runtime.json"),
            "{\"legacyCli\":\"/old/story.mjs\"}\n",
        );
        Self { root, commit }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn run(&self, args: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.root.join("fake-bin").display(),
            std::env::var("PATH").unwrap()
        );
        Command::new(env!("CARGO_BIN_EXE_story-install"))
            .args(args)
            .arg("--source")
            .arg(self.root.join("source"))
            .env("HOME", self.home())
            .env("PATH", path)
            .output()
            .unwrap()
    }

    fn install(&self, args: &[&str]) -> Output {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn release(&self) -> PathBuf {
        self.home()
            .join(".local/share/story-time/releases")
            .join(&self.commit)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn git(dir: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    output
}

fn legacy_cli(home: &Path) -> String {
    let runtime: Value =
        serde_json::from_str(&read(&home.join(".config/story-time/runtime.json"))).unwrap();
    runtime["legacyCli"].as_str().unwrap().to_string()
}

fn tree(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut entries = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                entries.push((path.clone(), read(&path)));
            }
        }
    }
    entries.sort();
    entries
}

#[test]
fn binary_skills_and_runtime_all_come_from_the_committed_release() {
    let sandbox = Sandbox::new("install");
    sandbox.install(&["--agent", "all"]);
    let home = sandbox.home();
    let release = sandbox.release();

    assert_eq!(
        read(&home.join(".local/bin/story")).trim(),
        format!("built from {}", release.join("native/story-push").display())
    );
    for skills in [".claude/skills", ".agents/skills"] {
        let skills = home.join(skills);
        assert_eq!(
            read(&skills.join("gap-sweep/SKILL.md")),
            "gap-sweep committed\n"
        );
        assert_eq!(
            read(&skills.join("story-time/SKILL.md")),
            "story-time committed\n"
        );
        assert!(!skills.join("story-time/scripts/test").exists());
    }
    let cli = PathBuf::from(legacy_cli(&home));
    assert_eq!(cli, release.join("skills/story-time/scripts/story.mjs"));
    assert!(cli.is_file());
}

#[test]
fn rollback_restores_the_previous_binary_and_runtime() {
    let sandbox = Sandbox::new("rollback");
    sandbox.install(&["--agent", "claude"]);
    let home = sandbox.home();
    assert_ne!(read(&home.join(".local/bin/story")), "old binary\n");

    sandbox.install(&["--rollback"]);
    assert_eq!(read(&home.join(".local/bin/story")), "old binary\n");
    assert_eq!(legacy_cli(&home), "/old/story.mjs");
}

#[test]
fn dry_run_prints_the_plan_and_writes_nothing() {
    let sandbox = Sandbox::new("dry");
    let before = tree(&sandbox.home());
    let output = sandbox.install(&["--agent", "all", "--dry-run"]);
    let plan = String::from_utf8(output.stdout).unwrap();
    assert!(plan.contains(&sandbox.commit), "{plan}");
    assert!(plan.contains("cargo install"), "{plan}");
    assert!(plan.contains(".claude/skills"), "{plan}");
    assert_eq!(tree(&sandbox.home()), before);
}

#[test]
fn a_failed_build_changes_nothing_and_leaves_nothing_to_roll_back() {
    let sandbox = Sandbox::new("failed");
    write(
        &sandbox.root.join("fake-bin/cargo"),
        "#!/bin/sh\necho 'build failed' >&2\nexit 1\n",
    );
    let failed = sandbox.run(&["--agent", "all"]);
    assert!(!failed.status.success());
    let home = sandbox.home();
    assert_eq!(read(&home.join(".local/bin/story")), "old binary\n");
    assert_eq!(legacy_cli(&home), "/old/story.mjs");

    let rollback = sandbox.run(&["--rollback"]);
    assert!(!rollback.status.success());
    assert!(String::from_utf8_lossy(&rollback.stderr).contains("No saved install"));
}
