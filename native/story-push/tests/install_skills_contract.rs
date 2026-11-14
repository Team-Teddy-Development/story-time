use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
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

fn prepare(root: &Path, setup: fn(&Path)) {
    let _ = fs::remove_dir_all(root);
    fs::create_dir_all(root.join("home")).unwrap();
    fs::create_dir_all(root.join("project")).unwrap();
    setup(root);
}

fn run(root: &Path, home: Home, setup: fn(&Path), args: &[&str]) -> Run {
    prepare(root, setup);
    let args: Vec<String> = args
        .iter()
        .map(|a| a.replace("{root}", &root.display().to_string()))
        .collect();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_story-install-skills"));
    cmd.env_clear().env("PATH", "/nonexistent");
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

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The skills a real install copies, in the sorted order the installer plans them.
fn skills() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(repo().join("skills"))
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().unwrap().is_dir())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// The account home the installer falls back to when HOME is unset: the passwd entry, not $HOME.
fn account_home() -> String {
    let probe = fixture_root();
    let uid = fs::metadata(&probe).unwrap().uid().to_string();
    fs::remove_dir_all(&probe).unwrap();
    fs::read_to_string("/etc/passwd")
        .unwrap()
        .lines()
        .map(|line| line.split(':').collect::<Vec<_>>())
        .find(|f| f.len() > 5 && f[2] == uid)
        .map(|f| f[5].to_string())
        .expect("account in /etc/passwd")
}

/// Node's program name, split so the doc check that no live file says to run it ignores this record.
macro_rules! node_usage_head {
    () => {
        concat!("Usage: node ", "scripts/install.mjs")
    };
}

/// Node's usage text, recorded from scripts/install.mjs before its removal.
const NODE_USAGE: &str = concat!(
    node_usage_head!(),
    " --agent codex|claude|all [--scope user|project]
                                [--project DIR] [--codex-home DIR] [--force] [--dry-run] [--uninstall]

  codex  user    ~/.agents/skills      (or $CODEX_HOME/skills with --codex-home)
  codex  project <project>/.agents/skills
  claude user    ~/.claude/skills
  claude project <project>/.claude/skills"
);

const NATIVE_ONLY_USAGE: &str = "
                            [--source DIR]

  --source DIR  the story-time checkout or release whose skills/ to copy
                (default: the checkout this binary was built from)";

/// The Rust usage text is Node's with two named differences: the program name (with its
/// continuation lines aligned under it) and the --source flag that only the Rust installer has.
fn as_node_usage(native: &str) -> String {
    native
        .replace(NATIVE_ONLY_USAGE, "")
        .replace("Usage: story-install-skills", node_usage_head!())
        .replace(
            &format!("\n{}[", " ".repeat(28)),
            &format!("\n{}[", " ".repeat(32)),
        )
}

/// What a case does to the fixture tree. Roots are relative to the fixture root.
enum Effect {
    None,
    Install(&'static [&'static str]),
    Replace(&'static [&'static str]),
    Uninstall(&'static [&'static str]),
}

struct Want {
    code: i32,
    stdout: String,
    stderr: String,
    effect: Effect,
}

fn ok(stdout: impl Into<String>, effect: Effect) -> Want {
    Want {
        code: 0,
        stdout: stdout.into(),
        stderr: String::new(),
        effect,
    }
}

fn fail(code: i32, stderr: impl Into<String>) -> Want {
    Want {
        code,
        stdout: String::new(),
        stderr: format!("{}\n", stderr.into()),
        effect: Effect::None,
    }
}

fn installed_into(roots: &[&str], codex: bool) -> String {
    let mut out = format!("Installed {} story-time skills into:\n", skills().len());
    for root in roots {
        out += &format!("  {root}\n");
    }
    if codex {
        out += "Codex: restart codex, then try \"$story-time\" or \"$story-pick\".\n";
    }
    out
}

fn plan(agent: &str, root: &str) -> String {
    skills()
        .iter()
        .map(|skill| format!("{agent:<6} {skill:<20} -> {root}/{skill}\n"))
        .collect()
}

fn clashes(paths: &[&str]) -> String {
    let mut out = String::from("A skill already exists at each of these paths:\n");
    for path in paths {
        out += &format!("  {path}\n");
    }
    out + "Re-run with --force to replace them (local edits will be lost)."
}

/// The fixture tree after the effect: each skill under each root is removed, copied from the
/// checkout's skills/ without scripts/test (modes kept), or both. New parent folders get the
/// mode a plain mkdir gives, which is the mode of the fixture's own home folder.
fn expected_tree(before: &Tree, effect: &Effect) -> Tree {
    let dir_mode = before[Path::new("home")].0;
    let mut tree = before.clone();
    let (roots, install, remove) = match effect {
        Effect::None => return tree,
        Effect::Install(roots) => (roots, true, false),
        Effect::Replace(roots) => (roots, true, true),
        Effect::Uninstall(roots) => (roots, false, true),
    };
    for root in roots.iter() {
        for skill in skills() {
            let dest = Path::new(root).join(&skill);
            if remove {
                tree.retain(|k, _| !k.starts_with(&dest));
            }
            if !install {
                continue;
            }
            for ancestor in dest.ancestors().skip(1) {
                if !ancestor.as_os_str().is_empty() {
                    tree.entry(ancestor.to_path_buf())
                        .or_insert((dir_mode, None));
                }
            }
            let source = repo().join("skills").join(&skill);
            tree.insert(dest.clone(), (fs::metadata(&source).unwrap().mode(), None));
            for (rel, entry) in self::tree(&source) {
                if !format!("/{skill}/{}", rel.display()).contains("/scripts/test") {
                    tree.insert(dest.join(rel), entry);
                }
            }
        }
    }
    tree
}

/// Checks the Rust installer against the values recorded from scripts/install.mjs before its
/// removal; commit 447aff8 ran Node against these same values.
fn expect(home: Home, setup: fn(&Path), args: &[&str], want: Want) {
    let root = fixture_root();
    prepare(&root, setup);
    let before = tree(&root);
    let account = account_home();
    let fill = |text: &str| {
        text.replace("{root}", &root.display().to_string())
            .replace("{home}", &account)
    };
    let tree_want = expected_tree(&before, &want.effect);
    let got = run(&root, home, setup, args);
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(
        as_node_usage(&got.stdout),
        fill(&want.stdout),
        "stdout for {args:?}"
    );
    assert_eq!(
        as_node_usage(&got.stderr),
        fill(&want.stderr),
        "stderr for {args:?}"
    );
    assert_eq!(got.code, Some(want.code), "exit code for {args:?}");
    let only = |a: &Tree, b: &Tree| -> Vec<PathBuf> {
        a.iter()
            .filter(|(k, v)| b.get(*k) != Some(v))
            .map(|(k, _)| k.clone())
            .collect()
    };
    assert!(
        got.tree == tree_want,
        "tree for {args:?}: unexpected or changed {:?}; missing or changed {:?}",
        only(&got.tree, &tree_want),
        only(&tree_want, &got.tree)
    );
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
    expect(
        Home::Fixture,
        empty,
        &["--agent", "codex"],
        ok(
            installed_into(&["{root}/home/.agents/skills"], true),
            Effect::Install(&["home/.agents/skills"]),
        ),
    );
    expect(
        Home::Fixture,
        empty,
        &["--agent", "claude"],
        ok(
            installed_into(&["{root}/home/.claude/skills"], false),
            Effect::Install(&["home/.claude/skills"]),
        ),
    );
    expect(
        Home::Fixture,
        empty,
        &["--agent", "all"],
        ok(
            installed_into(
                &["{root}/home/.claude/skills", "{root}/home/.agents/skills"],
                true,
            ),
            Effect::Install(&["home/.claude/skills", "home/.agents/skills"]),
        ),
    );
}

#[test]
fn installs_at_project_scope_into_the_given_or_current_directory() {
    expect(
        Home::Fixture,
        empty,
        &[
            "--agent",
            "codex",
            "--scope",
            "project",
            "--project",
            "{root}/project",
        ],
        ok(
            installed_into(&["{root}/project/.agents/skills"], true),
            Effect::Install(&["project/.agents/skills"]),
        ),
    );
    expect(
        Home::Fixture,
        empty,
        &[
            "--agent",
            "all",
            "--scope",
            "project",
            "--project",
            "./project/../project/",
        ],
        ok(
            installed_into(
                &[
                    "{root}/project/.claude/skills",
                    "{root}/project/.agents/skills",
                ],
                true,
            ),
            Effect::Install(&["project/.claude/skills", "project/.agents/skills"]),
        ),
    );
    expect(
        Home::Fixture,
        empty,
        &["--agent", "claude", "--scope", "project"],
        ok(
            installed_into(&["{root}/.claude/skills"], false),
            Effect::Install(&[".claude/skills"]),
        ),
    );
}

#[test]
fn codex_home_selects_the_legacy_skills_folder() {
    expect(
        Home::Fixture,
        empty,
        &["--agent", "codex", "--codex-home", "{root}/old/../codex/"],
        ok(
            installed_into(&["{root}/codex/skills"], true),
            Effect::Install(&["codex/skills"]),
        ),
    );
    expect(
        Home::Fixture,
        empty,
        &["--agent", "all", "--codex-home", "codex"],
        ok(
            installed_into(&["{root}/home/.claude/skills", "codex/skills"], true),
            Effect::Install(&["home/.claude/skills", "codex/skills"]),
        ),
    );
}

#[test]
fn dry_run_prints_the_plan_and_writes_nothing() {
    expect(
        Home::Fixture,
        empty,
        &["--agent", "all", "--dry-run"],
        ok(
            plan("claude", "{root}/home/.claude/skills")
                + &plan("codex", "{root}/home/.agents/skills"),
            Effect::None,
        ),
    );
    expect(
        Home::Fixture,
        empty,
        &[
            "--agent",
            "codex",
            "--scope",
            "project",
            "--project",
            "project",
            "--dry-run",
        ],
        ok(plan("codex", "{root}/project/.agents/skills"), Effect::None),
    );
    expect(
        Home::Fixture,
        empty,
        &[
            "--agent",
            "codex",
            "--codex-home",
            "{root}/codex",
            "--dry-run",
        ],
        ok(plan("codex", "{root}/codex/skills"), Effect::None),
    );
}

#[test]
fn existing_skills_are_refused_without_force_and_replaced_with_it() {
    expect(
        Home::Fixture,
        installed,
        &["--agent", "all"],
        fail(
            1,
            clashes(&[
                "{root}/home/.claude/skills/story-time",
                "{root}/home/.agents/skills/spike",
            ]),
        ),
    );
    expect(
        Home::Fixture,
        installed,
        &["--agent", "codex", "--dry-run"],
        fail(1, clashes(&["{root}/home/.agents/skills/spike"])),
    );
    expect(
        Home::Fixture,
        installed,
        &[
            "--agent",
            "claude",
            "--scope",
            "project",
            "--project",
            "project",
        ],
        fail(1, clashes(&["{root}/project/.claude/skills/spike"])),
    );
    expect(
        Home::Fixture,
        installed,
        &["--agent", "all", "--force"],
        ok(
            installed_into(
                &["{root}/home/.claude/skills", "{root}/home/.agents/skills"],
                true,
            ),
            Effect::Replace(&["home/.claude/skills", "home/.agents/skills"]),
        ),
    );
    expect(
        Home::Fixture,
        installed,
        &["--agent", "all", "--force", "--dry-run"],
        ok(
            plan("claude", "{root}/home/.claude/skills")
                + &plan("codex", "{root}/home/.agents/skills"),
            Effect::None,
        ),
    );
    expect(
        Home::Fixture,
        installed,
        &[
            "--agent",
            "claude",
            "--scope",
            "project",
            "--project",
            "project",
            "--force",
        ],
        ok(
            installed_into(&["{root}/project/.claude/skills"], false),
            Effect::Replace(&["project/.claude/skills"]),
        ),
    );
}

#[test]
fn unset_home_falls_back_to_the_account_home_like_node() {
    let home = account_home();
    for (agent, roots) in [
        ("codex", &[("codex", ".agents")][..]),
        ("claude", &[("claude", ".claude")]),
        ("all", &[("claude", ".claude"), ("codex", ".agents")]),
    ] {
        let mut existing = vec![];
        let mut planned = String::new();
        for (name, dir) in roots {
            let root = format!("{{home}}/{dir}/skills");
            planned += &plan(name, &root);
            for skill in skills() {
                if Path::new(&format!("{home}/{dir}/skills/{skill}")).exists() {
                    existing.push(format!("{root}/{skill}"));
                }
            }
        }
        let existing: Vec<&str> = existing.iter().map(String::as_str).collect();
        let want = if existing.is_empty() {
            ok(planned, Effect::None)
        } else {
            fail(1, clashes(&existing))
        };
        expect(Home::Unset, empty, &["--agent", agent, "--dry-run"], want);
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
fn empty_home_never_installs_into_relative_folders() {
    let root = fixture_root();
    let run = run(&root, Home::Empty, empty, &["--agent", "all", "--dry-run"]);
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
        let run = run(&root, Home::Fixture, empty, args);
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
    for (args, agents, roots) in [
        (
            &["--agent", "all", "--uninstall"][..],
            "claude, codex",
            &["home/.claude/skills", "home/.agents/skills"][..],
        ),
        (
            &["--agent", "codex", "--uninstall"],
            "codex",
            &["home/.agents/skills"],
        ),
        (
            &["--agent", "codex", "--codex-home", "codex", "--uninstall"],
            "codex",
            &["codex/skills"],
        ),
        (
            &[
                "--agent",
                "all",
                "--scope",
                "project",
                "--project",
                "project",
                "--uninstall",
            ],
            "claude, codex",
            &["project/.claude/skills", "project/.agents/skills"],
        ),
        (
            &["--agent", "claude", "--uninstall", "--force"],
            "claude",
            &["home/.claude/skills"],
        ),
    ] {
        expect(
            Home::Fixture,
            installed_beside_others,
            args,
            ok(
                format!("Removed story-time skills for {agents}.\n"),
                Effect::Uninstall(roots),
            ),
        );
    }
}

#[test]
fn uninstall_dry_run_lists_what_it_would_remove_and_writes_nothing() {
    expect(
        Home::Fixture,
        installed_beside_others,
        &["--agent", "all", "--uninstall", "--dry-run"],
        ok(
            "would remove {root}/home/.claude/skills/story-time\nwould remove {root}/home/.agents/skills/spike\n",
            Effect::None,
        ),
    );
    expect(
        Home::Fixture,
        installed_beside_others,
        &[
            "--agent",
            "codex",
            "--scope",
            "project",
            "--project",
            "project",
            "--uninstall",
            "--dry-run",
        ],
        ok(
            "would remove {root}/project/.agents/skills/story-time\n",
            Effect::None,
        ),
    );
}

#[test]
fn uninstall_with_nothing_installed_succeeds() {
    expect(
        Home::Fixture,
        empty,
        &["--agent", "all", "--uninstall"],
        ok(
            "Removed story-time skills for claude, codex.\n",
            Effect::None,
        ),
    );
    expect(
        Home::Fixture,
        empty,
        &["--agent", "codex", "--uninstall", "--dry-run"],
        ok("", Effect::None),
    );
}

#[test]
fn help_prints_the_usage_and_writes_nothing() {
    for args in [
        &["--help"][..],
        &["--help", "--agent", "bogus"],
        &["--agent", "all", "--uninstall", "--help"],
    ] {
        expect(
            Home::Fixture,
            installed,
            args,
            ok(format!("{NODE_USAGE}\n"), Effect::None),
        );
    }
}

#[test]
fn usage_errors_match_node() {
    let required = format!("--agent is required.\n\n{NODE_USAGE}");
    let unknown_agent = "Unknown agent \"bogus\". Use: claude, codex, all.";
    let unknown_scope = |scope: &str| format!("Unknown scope \"{scope}\". Use: user, project.");
    for (args, want) in [
        (&[][..], fail(2, &required)),
        (&["--scope", "project"], fail(2, &required)),
        (&["--agent", ""], fail(2, &required)),
        (&["bogus"], fail(2, "Unexpected argument \"bogus\".")),
        (
            &["--agent", "all", "extra"],
            fail(2, "Unexpected argument \"extra\"."),
        ),
        (&["--bogus"], fail(2, "Unknown flag --bogus.")),
        (&["--"], fail(2, "Unknown flag --.")),
        (&["--agent=all"], fail(2, "Unknown flag --agent=all.")),
        (&["--agent"], fail(2, "--agent needs a value.")),
        (
            &["--agent", "all", "--scope"],
            fail(2, "--scope needs a value."),
        ),
        (
            &["--agent", "all", "--project"],
            fail(2, "--project needs a value."),
        ),
        (
            &["--agent", "all", "--codex-home"],
            fail(2, "--codex-home needs a value."),
        ),
        (&["--help", "--agent"], fail(2, "--agent needs a value.")),
        (&["--agent", "bogus"], fail(1, unknown_agent)),
        (&["--agent", "bogus", "--uninstall"], fail(1, unknown_agent)),
        (
            &["--agent", "bogus", "--scope", "bogus"],
            fail(1, unknown_agent),
        ),
        (
            &["--agent", "all", "--scope", "bogus"],
            fail(1, unknown_scope("bogus")),
        ),
        (
            &["--agent", "claude", "--scope", ""],
            fail(1, unknown_scope("")),
        ),
        (
            &["--agent", "codex", "--scope", "bogus", "--uninstall"],
            fail(1, unknown_scope("bogus")),
        ),
    ] {
        expect(Home::Fixture, installed, args, want);
    }
}
