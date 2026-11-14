use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

const ROUNDS: usize = 10;
const RACERS: usize = 12;

struct Sandbox(PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project(name: &str) -> Sandbox {
    let root = std::env::temp_dir().join(format!("story-race-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".story-time")).unwrap();
    fs::write(
        root.join(".story-time/settings.json"),
        r#"{"prefix":"RACE","tracker":"files"}"#,
    )
    .unwrap();
    Sandbox(root)
}

fn spawn(root: &Path, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_story"))
        .args(args)
        .env_clear()
        .env("PATH", "/nonexistent")
        .env("HOME", root)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn titles(root: &Path) -> BTreeSet<String> {
    fs::read_dir(root.join("backlog"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .map(|p| {
            let text = fs::read_to_string(&p).unwrap();
            text.lines()
                .find_map(|l| l.strip_prefix("title: "))
                .unwrap_or_else(|| panic!("{} has a title", p.display()))
                .trim_matches('"')
                .to_owned()
        })
        .collect()
}

#[test]
fn concurrent_creates_never_replace_a_story() {
    let sandbox = project("new");
    let root = sandbox.0.clone();
    let mut expected = BTreeSet::new();
    for round in 0..ROUNDS {
        let racers: Vec<(String, Child)> = (0..RACERS)
            .map(|i| {
                let title = format!("Race {round} {i}");
                let child = spawn(&root, &["new", "--title", &title]);
                (title, child)
            })
            .collect();
        for (title, child) in racers {
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "story new \"{title}\" failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            expected.insert(title);
        }
    }
    let leftovers: Vec<_> = fs::read_dir(root.join("backlog"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| !n.ends_with(".md"))
        .collect();
    assert_eq!(leftovers, Vec::<String>::new(), "no temporary files remain");
    assert_eq!(
        titles(&root),
        expected,
        "every concurrent story new kept its own story file"
    );
}

#[test]
fn a_split_racing_creates_never_replaces_a_story() {
    let sandbox = project("split");
    let root = sandbox.0.clone();
    let mut expected = BTreeSet::new();
    for round in 0..ROUNDS {
        let title = format!("Parent {round}");
        let out = spawn(&root, &["new", "--title", &title])
            .wait_with_output()
            .unwrap();
        assert!(out.status.success());
        expected.insert(title);
        let parent = String::from_utf8(out.stdout).unwrap();
        let parent = parent.split(' ').next().unwrap().to_owned();
        let children = [format!("Child {round} a"), format!("Child {round} b")];
        let specs = root.join(format!("children-{round}.json"));
        fs::write(
            &specs,
            format!(
                r#"[{{"title":"{}"}},{{"title":"{}"}}]"#,
                children[0], children[1]
            ),
        )
        .unwrap();
        let mut racers: Vec<(Vec<String>, Child)> = (0..RACERS)
            .map(|i| {
                let title = format!("Race {round} {i}");
                (
                    vec![title.clone()],
                    spawn(&root, &["new", "--title", &title]),
                )
            })
            .collect();
        racers.insert(
            RACERS / 2,
            (
                children.to_vec(),
                spawn(
                    &root,
                    &["split", &parent, "--from", specs.to_str().unwrap()],
                ),
            ),
        );
        for (made, child) in racers {
            let out = child.wait_with_output().unwrap();
            if out.status.success() {
                expected.extend(made);
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                assert!(
                    made[0].starts_with("Child") && stderr.contains("File exists"),
                    "only a split may fail, and only because an id was taken: {stderr}"
                );
            }
        }
    }
    assert_eq!(
        titles(&root),
        expected,
        "no story was replaced and a failed split left nothing behind"
    );
}
