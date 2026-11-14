use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const GOOD_STORY: &str =
    "As a billing admin, I want invoices exported, so that I can reconcile them.";
const GOOD_AC: &str = "- [ ] Given an invoice, when I export, then a CSV row is written.";

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-lint-{}-{}",
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
    fn run(&self, native: bool, args: &[&str]) -> Output {
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
        cmd.arg("lint")
            .args(args)
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn compare(&self, args: &[&str]) -> Output {
        let before = self.files();
        let node = self.run(false, args);
        assert_eq!(before, self.files(), "Node changed files for {args:?}");
        let native = self.run(true, args);
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
        assert_eq!(
            native.status.code(),
            node.status.code(),
            "exit code differs for {args:?}"
        );
        native
    }
    fn both_forms(&self, args: &[&str], code: i32) -> String {
        let mut plain = String::new();
        for extra in [&[][..], &["--json"][..]] {
            let all: Vec<&str> = args.iter().chain(extra).copied().collect();
            let out = self.compare(&all);
            assert_eq!(out.status.code(), Some(code), "{all:?} exit code");
            if extra.is_empty() {
                plain = String::from_utf8(out.stdout).unwrap();
            }
        }
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

const SCORED: &str = "status: ready\nsize: 3\nvalue: 5\ntime_criticality: 3\nrisk_reduction: 2";

fn every_rule(s: &Sandbox) {
    s.write("model/state/invoice.toml", "name = \"invoice\"\n");
    s.story(
        "ST-001",
        &format!("title: Clean story\n{SCORED}\nstate_machine: model/state/invoice.toml"),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-002",
        &format!("title:  \n{SCORED}"),
        &[
            ("Story", "Save the cart to the DB."),
            ("Acceptance Criteria", GOOD_AC),
        ],
    );
    s.story(
        "ST-003",
        &format!("title: No benefit\n{SCORED}"),
        &[
            ("Story", "As a shopper, I want to save my cart."),
            ("Acceptance Criteria", GOOD_AC),
        ],
    );
    s.story(
        "ST-004",
        &format!("title: Generic persona\n{SCORED}"),
        &[
            ("Story", "As an end-users, I want x, so that y."),
            ("Acceptance Criteria", GOOD_AC),
        ],
    );
    s.story(
        "ST-005",
        "title: No criteria, unestimated, unscored\nstatus: draft",
        &[("Story", GOOD_STORY)],
    );
    s.story(
        "ST-006",
        &format!("title: Untestable and vague criteria\n{SCORED}"),
        &[
            ("Story", GOOD_STORY),
            (
                "Acceptance Criteria",
                "- [ ] cart is saved\n* [x] Given a, when b, then it is fast\n\
                 - [X] Given the page, when it loads, then it is User Friendly.\n\
                 - [ ]\n- nothing and so on\n  -   [ ]   spaced",
            ),
        ],
    );
    let many: String = (0..9)
        .map(|i| format!("- [ ] Given {i}, when {i}, then {i}\n"))
        .collect();
    s.story(
        "ST-007",
        "title: Too many criteria\nstatus: ready\nsize: 13\nvalue: 1\ntime_criticality: 1",
        &[("Story", GOOD_STORY), ("Acceptance Criteria", &many)],
    );
    s.story(
        "ST-008",
        &format!("title: Large\n{SCORED}\nsize: 8\ndepends_on: [ST-001, ST-404, ST-005, ST-020]"),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-009",
        "title: Bare spike\nstatus: draft\ntype: spike",
        &[],
    );
    s.story(
        "ST-010",
        "title: Answered spike\nstatus: ready\ntype: spike\ntimebox: 2d",
        &[
            ("Question", "Can we use X?"),
            ("Exit Criteria", "- [ ] Decision recorded quickly"),
        ],
    );
    s.story(
        "ST-011",
        &format!("title: Missing machine\n{SCORED}\nstate_machine: model/state/missing.toml\ndepends_on: [ST-010]"),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-012",
        "title: One-criterion chore\nstatus: ready\ntype: chore",
        &[("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-013",
        "title: Closed spike\nstatus: done\ntype: spike\ntimebox: 1d",
        &[("Question", "  ")],
    );
    s.story(
        "ST-020",
        "title: Done dependency\nstatus: done\nsize: 1\nvalue: 1\ntime_criticality: 1\nrisk_reduction: 1",
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
}

#[test]
fn every_rule_and_a_clean_story_match_node() {
    let s = Sandbox::new();
    every_rule(&s);
    let plain = s.both_forms(&[], 1);
    for rule in [
        "title.present",
        "statement.format",
        "statement.benefit",
        "statement.persona",
        "ac.present",
        "ac.gherkin",
        "ac.vague",
        "ac.count",
        "size.estimated",
        "size.small",
        "value.scored",
        "independent.deps",
        "spike.timebox",
        "spike.question",
        "spike.unblocks",
        "state_machine.exists",
    ] {
        assert!(
            plain.contains(&format!(" {rule} ")),
            "fixture never triggers {rule}"
        );
    }
    assert!(plain.contains("ST-001  Clean story  READY (0 errors, 0 warnings)"));
}

#[test]
fn per_id_forms_match_node() {
    let s = Sandbox::new();
    every_rule(&s);
    s.both_forms(&["ST-001"], 0);
    s.both_forms(&["ST-012", "ST-020"], 0);
    s.both_forms(&["ST-009", "ST-001"], 1);
    s.both_forms(&["ST-001", "ST-001"], 0);
    s.both_forms(&["ST-999"], 1);
    s.both_forms(&["bad"], 1);
    s.both_forms(&["ST-001", "--unknown", "value"], 0);
    s.both_forms(&["--dir", "elsewhere"], 0);
    s.both_forms(&["--dir", "elsewhere", "ST-001"], 1);
    for args in [&["--dir"][..], &["--config"][..]] {
        let out = s.compare(args);
        assert_eq!(out.status.code(), Some(2), "{args:?} is a usage error");
    }
}

#[test]
fn statement_and_criteria_text_edge_cases_match_node() {
    let s = Sandbox::new();
    for (i, (story, ac)) in [
        ("as THE Admin,I can x,so that y", GOOD_AC),
        (
            "  As an\u{a0}\u{3000}user ,  I need x so that   y.  ",
            GOOD_AC,
        ),
        (
            "As a person, I want x, ſo that y.",
            "- [ ] given A WHEN b Then c",
        ),
        (
            "As a visitor\n, I want\nmulti\nline, so that\nz.",
            "- [ ] Givenx when y then z",
        ),
        (
            "As a Customers, I want x, so that y",
            "- [ ] Given a, when b, thenc",
        ),
        ("As someone, I want x", "- [ ] Given_x, when y, then z\r"),
        (
            "As a\u{feff}user, I want x, so that y.",
            "- [ ] Given é, when b, then it is \u{212A}ind and nice",
        ),
        (
            "TODO: As a <persona>, I want <goal>, so that <benefit>.",
            "- [ ] Given a, when b, then it is easily done, etc",
        ),
        (
            "As an, I want, so that.",
            "- [ ] Given a, when b, then flexible-ish\n-\t[x]\tGiven a, when b, then efficient",
        ),
    ]
    .iter()
    .enumerate()
    {
        s.story(
            &format!("ST-{:03}", i + 1),
            &format!("title: Case {i}\n{SCORED}\ntype: story"),
            &[("Story", story), ("Acceptance Criteria", ac)],
        );
    }
    s.story(
        "ST-050",
        &format!("title: Bug skips statement\n{SCORED}\ntype: bug"),
        &[("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-051",
        &format!("title: Empty type needs statement\n{SCORED}\ntype: \"\""),
        &[("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-052",
        &format!("title: Exit criteria fallback\n{SCORED}"),
        &[("Story", GOOD_STORY), ("Exit Criteria", GOOD_AC)],
    );
    s.story(
        "ST-053",
        &format!("title: Empty AC section wins over Exit Criteria\n{SCORED}"),
        &[
            ("Story", GOOD_STORY),
            ("Acceptance Criteria", ""),
            ("Exit Criteria", GOOD_AC),
        ],
    );
    s.story(
        "ST-054",
        "title: Odd numbers\nstatus: ready\nsize: 12.5\nvalue: \"5\"\ntime_criticality: true\nrisk_reduction: 0",
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-055",
        &format!("title: Two-criterion chore\n{SCORED}\ntype: chore\nsize:"),
        &[("Acceptance Criteria", &format!("{GOOD_AC}\n{GOOD_AC}"))],
    );
    s.story(
        "ST-056",
        &format!("title: Numeric depends_on is ignored\n{SCORED}\ndepends_on: 0"),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-057",
        &format!("title: Boolean depends_on is ignored\n{SCORED}\ndepends_on: true"),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.both_forms(&[], 1);
}

#[test]
fn dependency_and_spike_value_shapes_match_node() {
    let s = Sandbox::new();
    s.story(
        "ST-001",
        "title: Spike by string\nstatus: draft\ntype: spike\ntimebox: 1d",
        &[("Question", "Q?")],
    );
    s.story(
        "ST-002",
        "title: Spike by substring\nstatus: refined\ntype: spike\ntimebox: 1d",
        &[("Question", "Q?")],
    );
    s.story(
        "ST-003",
        "title: Archived spike\nstatus: archived\ntype: spike",
        &[],
    );
    s.story(
        "ST-004",
        &format!("title: String depends_on is a substring list\n{SCORED}\ndepends_on: \"\""),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-005",
        &format!("title: Odd elements\n{SCORED}\ndepends_on: [, 5, [ST-001], ST-003, ST-003]"),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-006",
        &format!(
            "title: Empty list and falsy machine\n{SCORED}\ndepends_on: []\nstate_machine: \"\""
        ),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.story(
        "ST-007",
        &format!("title: Directory machine path\n{SCORED}\nstate_machine: model/../model/./state/"),
        &[("Story", GOOD_STORY), ("Acceptance Criteria", GOOD_AC)],
    );
    s.write("model/state/x.toml", "");
    s.both_forms(&[], 1);
    s.write(
        "backlog/ST-004.md",
        &format!("---\nid: ST-004\ntitle: Names spikes in a string\n{SCORED}\ndepends_on: \"xST-001, ST-002x\"\n---\n"),
    );
    let out = s.compare(&["ST-001"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn values_javascript_cannot_lint_fail_like_node() {
    for (frontmatter, other) in [
        ("title: 5", None),
        ("title: true", None),
        ("title: [a]", None),
        ("title: x\ndepends_on: ST-002", None),
        ("title: x\nstate_machine: 5", None),
        ("title: x\nstate_machine: -0", None),
        ("title: x\nstate_machine: 1.5", None),
        ("title: x\nstate_machine: true", None),
        ("title: x\nstate_machine: [a]", None),
        (
            "title: x\ntype: spike\nstatus: draft",
            Some("title: y\ndepends_on: 7"),
        ),
        (
            "title: x\ntype: spike\nstatus: draft",
            Some("title: y\ndepends_on: false"),
        ),
        (
            "title: x\ntype: spike\nstatus: draft",
            Some("title: y\ndepends_on: [ST-001]\n"),
        ),
    ] {
        let s = Sandbox::new();
        s.story("ST-001", frontmatter, &[("Story", GOOD_STORY)]);
        if let Some(other) = other {
            s.story("ST-002", other, &[]);
        }
        for args in [&[][..], &["ST-001"][..], &["--json"][..]] {
            s.compare(args);
        }
    }
}

#[test]
fn empty_and_malformed_backlogs_match_node() {
    let s = Sandbox::new();
    s.both_forms(&[], 0);
    s.write("backlog/README.md", "# Backlog\n");
    s.both_forms(&[], 0);
    s.write("backlog/ST-001.md", "no frontmatter\n");
    s.both_forms(&[], 1);
    s.both_forms(&["ST-001"], 1);
}
