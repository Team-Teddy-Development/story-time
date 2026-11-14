use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const GHERKIN: &str = "- [ ] Given an invoice, when I export, then a CSV row is written.";

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-judge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, relative: &str, text: &str) -> &Self {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }
    fn story(&self, id: &str, frontmatter: &str, sections: &[(&str, &str)]) -> &Self {
        let body: String = sections
            .iter()
            .map(|(name, text)| format!("\n## {name}\n{text}\n"))
            .collect();
        self.write(
            &format!("backlog/{id}.md"),
            &format!("---\nid: {id}\n{frontmatter}\n---\n{body}"),
        )
    }
    fn files(&self) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
        fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let mode = fs::symlink_metadata(&path).unwrap().permissions().mode();
                if path.is_dir() {
                    into.insert(path.clone(), (mode, Vec::new()));
                    walk(&path, into);
                } else {
                    into.insert(path.clone(), (mode, fs::read(&path).unwrap()));
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&self.0, &mut files);
        files
    }
    fn run(&self, native: bool, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut cmd = if native {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_story"));
            cmd.env_clear().env("PATH", "/nonexistent");
            cmd
        } else {
            let mut cmd = Command::new("node");
            cmd.arg(legacy())
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap());
            cmd
        };
        cmd.arg("judge")
            .args(args)
            .envs(env.iter().copied())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn compare_env(&self, args: &[&str], env: &[(&str, &str)], code: i32) -> String {
        let before = self.files();
        let node = self.run(false, args, env);
        assert_eq!(before, self.files(), "Node changed files for {args:?}");
        let native = self.run(true, args, env);
        assert_eq!(before, self.files(), "native changed files for {args:?}");
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&node.stdout),
            "stdout differs for {args:?}; native stderr: {}; node stderr: {}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr),
            "stderr differs for {args:?}"
        );
        assert_eq!(native.status.code(), node.status.code(), "{args:?}");
        assert_eq!(native.status.code(), Some(code), "{args:?} exit code");
        String::from_utf8(native.stdout).unwrap()
    }
    fn compare(&self, args: &[&str], code: i32) -> String {
        let plain = self.compare_env(args, &[], code);
        let json: Vec<&str> = args.iter().copied().chain(["--json"]).collect();
        self.compare_env(&json, &[], code);
        plain
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

fn criteria(n: usize) -> String {
    (0..n)
        .map(|i| format!("- [ ] Given {i}, when {i}, then {i}\n"))
        .collect()
}

fn every_heuristic(s: &Sandbox) {
    s.story(
        "ST-001",
        "title: Export invoices\nstatus: ready",
        &[
            (
                "Story",
                "As a billing admin, I want invoices exported, so that I can reconcile them.",
            ),
            ("Acceptance Criteria", GHERKIN),
        ],
    );
    s.story(
        "ST-002",
        "title: Save cart button\nstatus: draft",
        &[
            ("Story", "As a shopper, I want a Save Button on the cart, so that I can use the feature."),
            (
                "Acceptance Criteria",
                "- [ ] cart is saved\n- [x] Given a, when b, then it is fast\n- [ ] Given x, when y, then z",
            ),
        ],
    );
    s.story(
        "ST-003",
        "title: Restated\nstatus: refined",
        &[
            (
                "Story",
                "As a reader, I want to read reports, so that I can read the reports.",
            ),
            ("Acceptance Criteria", &criteria(4)),
        ],
    );
    s.story(
        "ST-004",
        "title: Login crashes on submit\nstatus: in-progress",
        &[
            (
                "Story",
                "As a member, I want to log in and see my orders, so that I can track deliveries.",
            ),
            ("Acceptance Criteria", &criteria(9)),
        ],
    );
    s.story(
        "ST-005",
        "title: Upgrade the CI runner\nstatus: draft",
        &[
            ("Story", "As an operator, I want the runner upgraded"),
            ("Acceptance Criteria", &criteria(6)),
        ],
    );
    s.story(
        "ST-006",
        "title: Investigate caching\nstatus: draft\ntype: spike\ntimebox: 2d",
        &[
            (
                "Question",
                "Should we evaluate whether to add a crash cache?",
            ),
            ("Exit Criteria", "- [ ] Given a, when b, then c"),
        ],
    );
    s.story(
        "ST-007",
        "title: Bare\nstatus: draft",
        &[("Story", "Save the cart to the DB.")],
    );
    s.story(
        "ST-008",
        "title: Done work\nstatus: done",
        &[("Story", "As a user, I want x, so that y.")],
    );
    s.story("ST-009", "status: split", &[]);
    s.story("ST-010", "status: archived", &[]);
    s.story(
        "ST-011",
        "status: draft",
        &[("Acceptance Criteria", &criteria(2))],
    );
    s.story(
        "ST-012",
        "title: Many\nstatus: ready",
        &[
            (
                "Story",
                "As a planner, I want many things, so that I can plan.",
            ),
            ("Acceptance Criteria", &criteria(12)),
        ],
    );
    s.story(
        "ST-013",
        "title: Very many\nstatus: ready\ntype: chore",
        &[
            (
                "Story",
                "As a planner, I want lots, so that I can plan more.",
            ),
            ("Acceptance Criteria", &criteria(14)),
        ],
    );
    s.story(
        "ST-014",
        "title: Mixed case\nstatus: ready",
        &[
            (
                "Story",
                "As the Ops team, I need a Redis table and JSON, so that pages load.",
            ),
            ("Acceptance Criteria", &criteria(7)),
        ],
    );
}

#[test]
fn judge_without_a_key_matches_node_for_every_argument_form() {
    let s = Sandbox::new();
    every_heuristic(&s);
    let all = s.compare(&[], 0);
    assert!(
        all.starts_with("engine: code (no TypeSafe key; run story secret set jev to use Jev)\n")
    );
    assert!(!all.contains("ST-008") && !all.contains("ST-009") && !all.contains("ST-010"));
    s.compare(&["ST-008", "ST-002", "ST-006"], 0);
    for id in 1..=14 {
        s.compare(&[&format!("ST-{id:03}")], 0);
    }
    s.compare(&["ST-404"], 1);
    s.compare(&["bad id"], 1);
    s.compare(&[""], 2);
    s.compare(&["ST-001", "--dir", "elsewhere"], 1);
}

#[test]
fn an_empty_backlog_prints_only_the_engine() {
    let s = Sandbox::new();
    s.write("backlog/.keep", "");
    assert_eq!(
        s.compare(&[], 0),
        "engine: code (no TypeSafe key; run story secret set jev to use Jev)\n"
    );
}

#[test]
fn the_kill_switch_and_a_stored_key_select_code_like_node() {
    let s = Sandbox::new();
    every_heuristic(&s);
    for off in ["off", " OFF ", "0", "false", "no"] {
        let out = s.compare_env(&["ST-001"], &[("STORY_TIME_JEV", off)], 0);
        assert!(out.starts_with("engine: code (disabled by STORY_TIME_JEV)"));
    }
    s.write(
        ".story-time/.jev.env",
        "TYPESAFE_API_KEY=tsk_fake_not_a_real_key_0000\n",
    );
    let key = s.0.join(".story-time/.jev.env");
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    s.compare_env(&["ST-001", "--json"], &[("STORY_TIME_JEV", "off")], 0);
    fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap();
    s.compare_env(&["ST-001"], &[("STORY_TIME_JEV", "off")], 1);
    s.compare_env(&["ST-001"], &[], 1);
    fs::write(&key, "OTHER=1\n").unwrap();
    s.compare_env(&["ST-001"], &[], 1);
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    s.compare(&["ST-001"], 0);
}
