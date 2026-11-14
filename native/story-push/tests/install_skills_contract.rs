use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

type Tree = BTreeMap<PathBuf, (u32, Option<Vec<u8>>)>;

struct Run {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    tree: Tree,
}

fn fixture_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "story-install-skills-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("home")).unwrap();
    fs::create_dir_all(root.join("project")).unwrap();
    root.canonicalize().unwrap()
}

fn tree(root: &Path) -> Tree {
    fn walk(root: &Path, dir: &Path, into: &mut Tree) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let key = path.strip_prefix(root).unwrap().to_path_buf();
            let mode = meta.permissions().mode();
            if meta.is_dir() {
                into.insert(key, (mode, None));
                walk(root, &path, into);
            } else {
                into.insert(key, (mode, Some(fs::read(&path).unwrap())));
            }
        }
    }
    let mut files = Tree::new();
    walk(root, root, &mut files);
    files
}

fn run(root: &Path, native: bool, setup: fn(&Path), args: &[&str]) -> Run {
    let _ = fs::remove_dir_all(root);
    fs::create_dir_all(root.join("home")).unwrap();
    fs::create_dir_all(root.join("project")).unwrap();
    setup(root);
    let args: Vec<String> = args
        .iter()
        .map(|a| a.replace("{root}", &root.display().to_string()))
        .collect();
    let mut cmd = if native {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_story-install-skills"));
        cmd.env_clear().env("PATH", "/nonexistent");
        cmd
    } else {
        let mut cmd = Command::new("node");
        cmd.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/install.mjs"))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap());
        cmd
    };
    let output: Output = cmd
        .args(&args)
        .env("HOME", root.join("home"))
        .current_dir(root)
        .output()
        .unwrap();
    Run {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        code: output.status.code(),
        tree: tree(root),
    }
}

fn compare(setup: fn(&Path), args: &[&str]) -> Run {
    let root = fixture_root();
    let node = run(&root, false, setup, args);
    let native = run(&root, true, setup, args);
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(native.stdout, node.stdout, "stdout differs for {args:?}");
    assert_eq!(native.stderr, node.stderr, "stderr differs for {args:?}");
    assert_eq!(native.code, node.code, "exit code differs for {args:?}");
    let only = |a: &Tree, b: &Tree| -> Vec<PathBuf> {
        a.iter()
            .filter(|(k, v)| b.get(*k) != Some(v))
            .map(|(k, _)| k.clone())
            .collect()
    };
    assert!(
        native.tree == node.tree,
        "trees differ for {args:?}: native-only or changed {:?}; node-only or changed {:?}",
        only(&native.tree, &node.tree),
        only(&node.tree, &native.tree)
    );
    native
}

fn empty(_: &Path) {}

fn installed(root: &Path) {
    for (dir, text) in [
        ("home/.agents/skills/spike", "my edits"),
        ("home/.claude/skills/story-time/scripts", "mine"),
        ("project/.claude/skills/spike", "project edits"),
    ] {
        let dir = root.join(dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), text).unwrap();
    }
}

#[test]
fn installs_for_each_agent_at_user_scope() {
    for agent in ["codex", "claude", "all"] {
        let run = compare(empty, &["--agent", agent]);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        assert!(run.stdout.starts_with("Installed "), "{}", run.stdout);
        assert!(run
            .tree
            .keys()
            .any(|k| k.ends_with("skills/story-time/scripts/story.mjs")));
        assert!(!run.tree.keys().any(|k| k.ends_with("scripts/test")));
    }
}

#[test]
fn installs_at_project_scope_into_the_given_or_current_directory() {
    for args in [
        &[
            "--agent",
            "codex",
            "--scope",
            "project",
            "--project",
            "{root}/project",
        ][..],
        &[
            "--agent",
            "all",
            "--scope",
            "project",
            "--project",
            "./project/../project/",
        ],
        &["--agent", "claude", "--scope", "project"],
    ] {
        let run = compare(empty, args);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
    }
}

#[test]
fn codex_home_selects_the_legacy_skills_folder() {
    for args in [
        &["--agent", "codex", "--codex-home", "{root}/old/../codex/"][..],
        &["--agent", "all", "--codex-home", "codex"],
    ] {
        let run = compare(empty, args);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        assert!(run.tree.keys().any(|k| k.starts_with("codex/skills")));
    }
}

#[test]
fn dry_run_prints_the_plan_and_writes_nothing() {
    for args in [
        &["--agent", "all", "--dry-run"][..],
        &[
            "--agent",
            "codex",
            "--scope",
            "project",
            "--project",
            "project",
            "--dry-run",
        ],
        &[
            "--agent",
            "codex",
            "--codex-home",
            "{root}/codex",
            "--dry-run",
        ],
    ] {
        let run = compare(empty, args);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        assert!(run.stdout.contains(" -> "), "{}", run.stdout);
        assert!(!run.tree.keys().any(|k| k.ends_with("skills")));
    }
}

#[test]
fn existing_skills_are_refused_without_force_and_replaced_with_it() {
    for args in [
        &["--agent", "all"][..],
        &["--agent", "codex", "--dry-run"],
        &[
            "--agent",
            "claude",
            "--scope",
            "project",
            "--project",
            "project",
        ],
    ] {
        let run = compare(installed, args);
        assert_eq!(run.code, Some(1));
        assert!(run.stderr.contains("--force"), "{}", run.stderr);
        assert!(run.stdout.is_empty());
    }
    for args in [
        &["--agent", "all", "--force"][..],
        &["--agent", "all", "--force", "--dry-run"],
        &[
            "--agent",
            "claude",
            "--scope",
            "project",
            "--project",
            "project",
            "--force",
        ],
    ] {
        let run = compare(installed, args);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
    }
}
