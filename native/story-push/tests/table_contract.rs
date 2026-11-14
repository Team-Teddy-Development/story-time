use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const PROMOTION: &str = r#"name = "promotion"
hit_policy = "unique"
outputs = ["promote"]

[inputs]
role = ["admin", "member", "guest"]
verified = [true, false]

[[rules]]
when = { role = "admin", verified = "-" }
then = { promote = "yes" }

[[rules]]
when = { role = "member", verified = true }
then = { promote = "yes" }

[[rules]]
when = { role = "member", verified = false }
then = { promote = "no" }

[[rules]]
when = { role = "guest", verified = "-" }
then = { promote = "no" }
"#;

const EXTRA_ADMIN_RULE: &str =
    "\n[[rules]]\nwhen = { role = \"admin\", verified = true }\nthen = { promote = \"no\" }\n";

const ACTIONS: [&str; 2] = ["check", "criteria"];

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-table-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::write(
            path.join("backlog/ST-001.md"),
            "---\nid: ST-001\ntitle: Fixture\nstatus: draft\n---\n",
        )
        .unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, relative: &str, text: &str) -> &Self {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }
    fn files(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    into.insert(path.clone(), Vec::new());
                    walk(&path, into);
                } else {
                    into.insert(path.clone(), fs::read(&path).unwrap());
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
        cmd.args(args)
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
            "stdout differs for {args:?}; native stderr: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(
            native.status.code(),
            node.status.code(),
            "exit code differs for {args:?}; native stderr: {}; node stderr: {}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&node.stderr),
            "stderr differs for {args:?}"
        );
        native
    }
    /// Runs every action, with and without --json, on `text` saved at `file`, and returns the exit codes.
    fn every_action(&self, file: &str, text: &str) -> Vec<i32> {
        self.write(file, text);
        let mut codes = Vec::new();
        for action in ACTIONS {
            for json in [false, true] {
                let mut args = vec!["table", action, file];
                if json {
                    args.push("--json");
                }
                codes.push(self.compare(&args).status.code().unwrap());
            }
        }
        codes
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

fn with(replacements: &[(&str, &str)], extra: &str) -> String {
    let mut text = PROMOTION.to_owned();
    for (from, to) in replacements {
        assert!(text.contains(from), "fixture lacks {from:?}");
        text = text.replacen(from, to, 1);
    }
    text + extra
}

#[test]
fn valid_tables_match_node() {
    let s = Sandbox::new();
    let cases = [
        PROMOTION.to_owned(),
        with(&[("name = \"promotion\"\n", "")], ""),
        with(&[("name = \"promotion\"", "name = 42")], ""),
        with(&[("name = \"promotion\"", "name = [\"x\", 1]")], ""),
        with(&[("name = \"promotion\"", "name = { a = 1 }")], ""),
        with(&[("\"unique\"", "\"first\"")], EXTRA_ADMIN_RULE),
        with(
            &[("\"unique\"", "\"first\"")],
            "\n[[rules]]\nwhen = { role = \"-\", verified = \"-\" }\nthen = { promote = \"maybe\" }\n",
        ),
        with(
            &[
                ("outputs = [\"promote\"]", "outputs = [\"promote\", \"why\"]"),
                ("then = { promote = \"yes\" }", "then = { promote = [\"y\", 1], why = { b = 1, 2 = \"two\", a = true } }"),
                ("then = { promote = \"yes\" }", "then = { promote = 7, why = \"\" }"),
                ("then = { promote = \"no\" }", "then = { why = false, promote = \"no\" }"),
                ("then = { promote = \"no\" }", "then = { promote = [], why = [[1, 2], \"x\"] }"),
            ],
            "",
        ),
        "hit_policy = \"unique\"\noutputs = [\"o\"]\n[inputs]\nb = [1, 2]\n2 = [\"x\"]\na = [-3, 0]\n1 = [true]\n[[rules]]\nwhen = { a = \"-\", b = \"-\", 1 = \"-\", 2 = \"x\" }\nthen = { o = 9007199254740991 }\n".to_owned(),
        "hit_policy = \"first\"\noutputs = [\"0\"]\n[inputs]\n0 = [\"a\", \"-\"]\n[[rules]]\nwhen = \"a\"\nthen = \"z\"\n[[rules]]\nwhen = [\"-\"]\nthen = [\"y\"]\n".to_owned(),
        "hit_policy = \"unique\"\noutputs = [\"o\"]\n[inputs]\nx = [\"caf\u{e9}\", \"\\\"q\\\"\", \"a, b\"]\n[[rules]]\nwhen = { x = \"-\" }\nthen = { o = \"\\u00e9\\t\" }\n".to_owned(),
    ];
    for (i, text) in cases.iter().enumerate() {
        let codes = s.every_action(&format!("model/rules/valid{i}.toml"), text);
        assert!(codes.iter().all(|c| *c == 0 || *c == 1), "{i}: {codes:?}");
    }
    assert_eq!(
        s.every_action("model/rules/promotion.toml", PROMOTION),
        vec![0; 4]
    );
}

#[test]
fn each_defect_matches_node() {
    let s = Sandbox::new();
    let values = (0..20)
        .map(|i| format!("\"v{i}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let cases = [
        with(&[("[[rules]]\nwhen = { role = \"guest\", verified = \"-\" }\nthen = { promote = \"no\" }\n", "")], ""),
        with(&[], EXTRA_ADMIN_RULE),
        with(&[], &format!("{EXTRA_ADMIN_RULE}{EXTRA_ADMIN_RULE}")),
        with(&[("\"unique\"", "\"first\"")], EXTRA_ADMIN_RULE),
        with(&[("\"unique\"", "\"first\"")], &format!("{EXTRA_ADMIN_RULE}\n[[rules]]\nwhen = {{ role = \"-\", verified = \"-\" }}\nthen = {{ promote = \"x\" }}\n")),
        "hit_policy = \"any\"\noutputs = [\"promote\"]\n\n[inputs]\nrole = [\"admin\"]\nempty = []\n\n[[rules]]\nwhen = { role = \"owner\" }\nthen = { promote = \"yes\", extra = 1 }\n".to_owned(),
        format!("hit_policy = \"first\"\noutputs = [\"o\"]\n[inputs]\na = [{values}]\nb = [{values}]\nc = [{values}]\n[[rules]]\nwhen = {{ a = \"-\", b = \"-\", c = \"-\" }}\nthen = {{ o = 1 }}\n"),
        String::new(),
        with(&[("hit_policy = \"unique\"", "hit_policy = 1")], ""),
        with(&[("hit_policy = \"unique\"", "hit_policy = { b = 1, 1 = [\"x\", true] }")], ""),
        with(&[("hit_policy = \"unique\"", "hit_policy = \"Unique\"")], ""),
        "hit_policy = \"unique\"\ninputs = [\"a\"]\noutputs = \"o\"\nrules = []\n".to_owned(),
        "hit_policy = \"unique\"\ninputs = \"abc\"\noutputs = [1, \"o\"]\nrules = [1, \"x\", [2], true]\n".to_owned(),
        "hit_policy = \"first\"\noutputs = [\"o\", [1], { a = 1 }]\n[inputs]\na = \"xy\"\nb = 3\nc = [[1], { k = 1 }]\nd = [1, 1]\ne = [\"q\", true, 2, \"q\"]\n0 = [\"z\"]\n[[rules]]\nwhen = \"zz\"\nthen = [\"v\"]\n[[rules]]\nwhen = { a = \"x\", b = 3, c = [1], d = 2, e = \"-\", f = 1, toString = 1 }\nthen = { o = 1, 1 = 2, \"[object Object]\" = 3 }\n[[rules]]\nwhen = false\nthen = 0\n".to_owned(),
        with(&[("verified = true }", "verified = \"true\" }"), ("role = \"guest\"", "role = \"Guest\"")], ""),
        with(&[("when = { role = \"admin\", verified = \"-\" }", "when = { role = \"admin\" }")], ""),
        with(&[("when = { role = \"admin\", verified = \"-\" }", "when = { role = \"admin\", verified = \"-\", extra = 1 }")], ""),
        with(&[("then = { promote = \"yes\" }", "then = {}")], ""),
        with(&[("then = { promote = \"yes\" }", "then = { promote = \"yes\", why = 1 }")], ""),
        "hit_policy = \"unique\"\noutputs = [\"o\"]\n[inputs]\nx = [1, 2, 3]\n[[rules]]\nwhen = { x = 1 }\nthen = { o = 1 }\n[[rules]]\nwhen = { x = \"-\" }\nthen = { o = 2 }\n[[rules]]\nwhen = { x = 1 }\nthen = { o = 3 }\n".to_owned(),
    ];
    for (i, text) in cases.iter().enumerate() {
        let codes = s.every_action(&format!("model/rules/defect{i}.toml"), text);
        assert_eq!(codes, vec![1; 4], "case {i} should report defects");
    }
}

#[test]
fn toml_errors_match_node() {
    let s = Sandbox::new();
    let cases = [
        "a = \"unterminated\n",
        "a.b = 1\n",
        "a = 1.5\n",
        "a = 1\na = 2\n",
        "[inputs]\n[inputs]\n",
        "a = [1, 2\n\n",
        "a = {x = 1, x = 2}\n",
        "\u{feff}a = 1\n",
    ];
    for (i, text) in cases.iter().enumerate() {
        let codes = s.every_action(&format!("model/rules/toml{i}.toml"), text);
        assert!(codes.iter().all(|c| *c == 1), "case {i}: {codes:?}");
    }
}

#[test]
fn usage_and_file_errors_match_node() {
    let s = Sandbox::new();
    s.write("model/rules/ok.toml", PROMOTION);
    fs::create_dir_all(s.0.join("model/dir.toml")).unwrap();
    let cases: [(&[&str], i32); 13] = [
        (&["table"], 2),
        (&["table", "--json"], 2),
        (&["table", "check"], 2),
        (&["table", "diagram", "model/rules/ok.toml"], 2),
        (&["table", "check", "missing.toml"], 1),
        (&["table", "criteria", "model/missing.toml", "--json"], 1),
        (&["table", "check", "model/dir.toml"], 1),
        (&["table", "check", "model/rules/ok.toml", "--dir"], 2),
        (
            &[
                "table",
                "check",
                "model/rules/ok.toml",
                "--config",
                "nope.json",
            ],
            0,
        ),
        (
            &[
                "table",
                "criteria",
                "model/rules/ok.toml",
                "extra",
                "--dir",
                "x",
            ],
            0,
        ),
        (&["table", "check", "./model/rules/../rules/ok.toml"], 0),
        (&["table", "check", "model/rules/ok.toml/"], 1),
        (
            &[
                "table",
                "criteria",
                "model/rules/ok.toml",
                "--json",
                "--json",
            ],
            0,
        ),
    ];
    for (args, code) in cases {
        let out = s.compare(args);
        assert_eq!(out.status.code(), Some(code), "{args:?}");
    }
    s.write(".story-time/settings.json", "{\"colour\":\"red\"}");
    let out = s.compare(&["table", "check", "model/rules/ok.toml"]);
    assert_eq!(out.status.code(), Some(1), "invalid config fails first");
}

#[test]
fn documented_example_matches_node() {
    let skill =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/decision-table/SKILL.md");
    let text = fs::read_to_string(skill).unwrap();
    let start = text.find("```toml\n").unwrap() + "```toml\n".len();
    let example = &text[start..start + text[start..].find("```").unwrap()];
    let s = Sandbox::new();
    assert_eq!(
        s.every_action("model/rules/promotion.toml", example),
        vec![1; 4]
    );
}

#[test]
fn node_fixture_matches_node() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills/story-time/scripts/test/fixtures.mjs");
    let text = fs::read_to_string(fixtures).unwrap();
    let start =
        text.find("export const PROMOTION = `").unwrap() + "export const PROMOTION = `".len();
    let fixture = &text[start..start + text[start..].find('`').unwrap()];
    assert_eq!(fixture, PROMOTION, "the Node fixture changed");
    let s = Sandbox::new();
    assert_eq!(
        s.every_action("model/rules/fixture.toml", fixture),
        vec![0; 4]
    );
}
