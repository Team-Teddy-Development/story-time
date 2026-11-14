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

#[derive(Clone, Copy)]
enum Home {
    Fixture,
    Unset,
    Empty,
}

fn run(root: &Path, native: bool, home: Home, setup: fn(&Path), args: &[&str]) -> Run {
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
    match home {
        Home::Fixture => cmd.env("HOME", root.join("home")),
        Home::Unset => &mut cmd,
        Home::Empty => cmd.env("HOME", ""),
    };
    let output: Output = cmd.args(&args).current_dir(root).output().unwrap();
    Run {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        code: output.status.code(),
        tree: tree(root),
    }
}

const NATIVE_ONLY_USAGE: &str = "
                            [--source DIR]

  --source DIR  the story-time checkout or release whose skills/ to copy
                (default: the checkout this binary was built from)";

/// The usage text is Node's with two named differences: the program name (with its
/// continuation lines aligned under it) and the --source flag that only the Rust installer has.
fn as_node_usage(native: &str) -> String {
    native
        .replace(NATIVE_ONLY_USAGE, "")
        .replace(
            "Usage: story-install-skills",
            "Usage: node scripts/install.mjs",
        )
        .replace(
            &format!("\n{}[", " ".repeat(28)),
            &format!("\n{}[", " ".repeat(32)),
        )
}

fn compare(setup: fn(&Path), args: &[&str]) -> Run {
    compare_home(Home::Fixture, setup, args)
}

fn compare_home(home: Home, setup: fn(&Path), args: &[&str]) -> Run {
    let root = fixture_root();
    let node = run(&root, false, home, setup, args);
    let mut native = run(&root, true, home, setup, args);
    fs::remove_dir_all(&root).unwrap();
    native.stdout = as_node_usage(&native.stdout);
    native.stderr = as_node_usage(&native.stderr);
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

fn untouched(run: &Run) {
    let keys: Vec<&PathBuf> = run.tree.keys().collect();
    assert_eq!(
        keys,
        [Path::new("home"), Path::new("project")],
        "relative folders created"
    );
}

#[test]
fn unset_home_falls_back_to_the_account_home_like_node() {
    for agent in ["codex", "claude", "all"] {
        let run = compare_home(Home::Unset, empty, &["--agent", agent, "--dry-run"]);
        untouched(&run);
        let text = format!("{}{}", run.stdout, run.stderr);
        assert!(
            !text.contains(" .claude") && !text.contains(" .agents"),
            "{text}"
        );
        assert!(
            text.contains("/.claude/skills/") || text.contains("/.agents/skills/"),
            "{text}"
        );
    }
}

#[test]
fn empty_home_never_installs_into_relative_folders() {
    let root = fixture_root();
    let run = run(
        &root,
        true,
        Home::Empty,
        empty,
        &["--agent", "all", "--dry-run"],
    );
    fs::remove_dir_all(&root).unwrap();
    untouched(&run);
    let text = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !text.contains(" .claude") && !text.contains(" .agents"),
        "{text}"
    );
}

fn skills_source(root: &Path) {
    let skill = root.join("release/skills/alpha");
    fs::create_dir_all(skill.join("scripts/test")).unwrap();
    fs::write(skill.join("SKILL.md"), "alpha").unwrap();
    fs::write(skill.join("scripts/test/a.test.mjs"), "test").unwrap();
}

#[test]
fn source_selects_the_skills_folder() {
    let root = fixture_root();
    let run = run(
        &root,
        true,
        Home::Fixture,
        skills_source,
        &["--agent", "claude", "--source", "release"],
    );
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(
        run.tree
            .get(Path::new("home/.claude/skills/alpha/SKILL.md"))
            .and_then(|(_, b)| b.clone()),
        Some(b"alpha".to_vec())
    );
    assert!(!run
        .tree
        .keys()
        .any(|k| k.starts_with("home") && k.ends_with("scripts/test")));
    let installed: Vec<&PathBuf> = run
        .tree
        .keys()
        .filter(|k| k.starts_with("home/.claude/skills") && k.components().count() == 4)
        .collect();
    assert_eq!(installed, [Path::new("home/.claude/skills/alpha")]);
}

#[test]
fn missing_skills_folder_fails_clearly_and_writes_nothing() {
    for args in [
        &["--agent", "all", "--source", "{root}/gone"][..],
        &["--agent", "codex", "--source", "project", "--dry-run"],
    ] {
        let root = fixture_root();
        let run = run(&root, true, Home::Fixture, empty, args);
        let missing = if args[3].starts_with('{') {
            root.join("gone/skills")
        } else {
            root.join("project/skills")
        };
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(run.code, Some(1), "{}", run.stderr);
        assert!(run.stdout.is_empty(), "{}", run.stdout);
        assert_eq!(
            run.stderr,
            format!(
                "No story-time skills folder at {}.\nPass --source DIR with the story-time checkout or release that holds skills/.\n",
                missing.display()
            )
        );
        untouched(&run);
    }
}

fn installed_beside_others(root: &Path) {
    installed(root);
    for dir in [
        "home/.claude/skills/my-own",
        "home/.agents/skills/my-own",
        "project/.claude/skills/my-own",
        "project/.agents/skills/story-time",
        "codex/skills/spike",
        "codex/skills/my-own",
    ] {
        let dir = root.join(dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "keep or remove").unwrap();
    }
}

#[test]
fn uninstall_removes_only_story_time_skills() {
    for args in [
        &["--agent", "all", "--uninstall"][..],
        &["--agent", "codex", "--uninstall"],
        &["--agent", "codex", "--codex-home", "codex", "--uninstall"],
        &[
            "--agent",
            "all",
            "--scope",
            "project",
            "--project",
            "project",
            "--uninstall",
        ],
        &["--agent", "claude", "--uninstall", "--force"],
    ] {
        let run = compare(installed_beside_others, args);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        assert!(run.stdout.starts_with("Removed story-time skills for "));
        assert!(run.tree.keys().any(|k| k.ends_with("my-own/SKILL.md")));
    }
    let run = compare(installed_beside_others, &["--agent", "all", "--uninstall"]);
    assert!(!run
        .tree
        .contains_key(Path::new("home/.claude/skills/story-time")));
    assert!(!run
        .tree
        .contains_key(Path::new("home/.agents/skills/spike")));
}

#[test]
fn uninstall_dry_run_lists_what_it_would_remove_and_writes_nothing() {
    for args in [
        &["--agent", "all", "--uninstall", "--dry-run"][..],
        &[
            "--agent",
            "codex",
            "--scope",
            "project",
            "--uninstall",
            "--dry-run",
        ],
    ] {
        let run = compare(installed_beside_others, args);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        assert!(run.stdout.starts_with("would remove "), "{}", run.stdout);
    }
}

#[test]
fn uninstall_with_nothing_installed_succeeds() {
    for args in [
        &["--agent", "all", "--uninstall"][..],
        &["--agent", "codex", "--uninstall", "--dry-run"],
    ] {
        let run = compare(empty, args);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        untouched(&run);
    }
}

#[test]
fn help_prints_the_usage_and_writes_nothing() {
    for args in [
        &["--help"][..],
        &["--help", "--agent", "bogus"],
        &["--agent", "all", "--uninstall", "--help"],
    ] {
        let run = compare(installed, args);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        assert!(run.stdout.starts_with("Usage: node scripts/install.mjs"));
        assert!(run.stderr.is_empty());
    }
}

#[test]
fn usage_errors_match_node() {
    for (args, code) in [
        (&[][..], 2),
        (&["--scope", "project"][..], 2),
        (&["--agent", ""][..], 2),
        (&["bogus"][..], 2),
        (&["--agent", "all", "extra"][..], 2),
        (&["--bogus"][..], 2),
        (&["--"][..], 2),
        (&["--agent=all"][..], 2),
        (&["--agent"][..], 2),
        (&["--agent", "all", "--scope"][..], 2),
        (&["--agent", "all", "--project"][..], 2),
        (&["--agent", "all", "--codex-home"][..], 2),
        (&["--help", "--agent"][..], 2),
        (&["--agent", "bogus"][..], 1),
        (&["--agent", "bogus", "--uninstall"][..], 1),
        (&["--agent", "bogus", "--scope", "bogus"][..], 1),
        (&["--agent", "all", "--scope", "bogus"][..], 1),
        (&["--agent", "claude", "--scope", ""][..], 1),
        (
            &["--agent", "codex", "--scope", "bogus", "--uninstall"][..],
            1,
        ),
    ] {
        let run = compare(installed, args);
        assert_eq!(run.code, Some(code), "{args:?}: {}", run.stderr);
        assert!(run.stdout.is_empty(), "{args:?}: {}", run.stdout);
        assert!(!run.stderr.is_empty(), "{args:?}");
    }
}
