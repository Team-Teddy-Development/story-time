use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const ENROLLMENT: &str = r#"entity = "enrollment"
states = ["draft", "submitted", "approved", "rejected"]
initial = "draft"
terminal = ["approved", "rejected"]
events = ["submit", "approve", "reject", "withdraw"]
invariants = ["An approved enrollment has a coverage start date."]

[guards]
complete = "every required field is filled in"

[[transitions]]
from = "draft"
to = "submitted"
event = "submit"
guard = "complete"

[[transitions]]
from = "submitted"
to = "approved"
event = "approve"

[[transitions]]
from = "submitted"
to = "rejected"
event = "reject"

[[transitions]]
from = "submitted"
to = "draft"
event = "withdraw"
"#;

const ACTIONS: [&str; 3] = ["check", "criteria", "diagram"];

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-machine-{}-{}",
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
                let mut args = vec!["machine", action, file];
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
    let mut text = ENROLLMENT.to_owned();
    for (from, to) in replacements {
        assert!(text.contains(from), "fixture lacks {from:?}");
        text = text.replacen(from, to, 1);
    }
    text + extra
}

#[test]
fn valid_machines_match_node() {
    let s = Sandbox::new();
    let cases = [
        ENROLLMENT.to_owned(),
        with(&[("entity = \"enrollment\"\n", "")], ""),
        with(&[("entity = \"enrollment\"\n", "name = \"order\"\n")], ""),
        with(&[("entity = \"enrollment\"", "entity = \"cutover stage\"")], ""),
        with(&[("entity = \"enrollment\"", "entity = 42")], ""),
        with(&[("entity = \"enrollment\"", "entity = [\"x\", 1]")], ""),
        with(
            &[],
            "\n[[transitions]]\nfrom = \"draft\"\nto = \"draft\"\nevent = \"submit\"\n",
        ),
        with(
            &[],
            "\n[[transitions]]\nfrom = \"draft\"\nto = \"rejected\"\nevent = \"submit\"\nguard = \"late\"\n",
        )
        .replace(
            "complete = \"every required field is filled in\"",
            "complete = \"every required field is filled in\"\nlate = \"\"",
        ),
        with(
            &[(
                "guard = \"complete\"",
                "guard = 7",
            ), ("complete = \"every", "7 = \"every")],
            "",
        ),
        with(&[("entity = \"enrollment\"", "states_extra = 1")], "")
            .replace("states = [", "states = [\"draft\", "),
        "# lifecycle\r\n'entity' = 'job'\r\nstates = [\r\n  \"a\", # first\r\n  \"b\",\r\n]\r\ninitial = \"a\"\r\nterminal = [\"b\"]\r\nevents = [\"go\"]\r\ntransitions = [{ from = \"a\", to = \"b\", event = \"go\" }]\r\nn = +1_000\r\nflag = true\r\n".to_owned(),
        "states = [\"only\"]\ninitial = \"only\"\nterminal = [\"only\"]\nevents = [\"poke\", \"poke\"]\nstates_note = \"\\u00e9\\t\\\"x\\\"\"\n[[transitions]]\nfrom = \"only\"\nto = \"only\"\nevent = \"poke\"\nguard = \"g\"\n[guards]\ng = \"\"\n".to_owned(),
    ];
    for (i, text) in cases.iter().enumerate() {
        let codes = s.every_action(&format!("model/state/valid{i}.toml"), text);
        assert!(codes.iter().all(|c| *c == 0 || *c == 1), "{i}: {codes:?}");
    }
    assert_eq!(
        s.every_action("model/state/enrollment.toml", ENROLLMENT),
        vec![0; 6]
    );
}

#[test]
fn each_defect_matches_node() {
    let s = Sandbox::new();
    let cases = [
        with(
            &[
                ("\"rejected\"]\ninitial", "\"rejected\", \"orphan\"]\ninitial"),
                (
                    "terminal = [\"approved\", \"rejected\"]",
                    "terminal = [\"approved\", \"rejected\", \"orphan\"]",
                ),
            ],
            "",
        ),
        with(
            &[(
                "terminal = [\"approved\", \"rejected\"]",
                "terminal = [\"approved\"]",
            )],
            "",
        ),
        with(&[("to = \"approved\"\nevent = \"approve\"", "to = \"approved\"")], ""),
        with(&[("event = \"approve\"", "event = \"\"")], ""),
        with(&[("event = \"approve\"", "event = 0")], ""),
        with(
            &[
                ("to = \"rejected\"", "to = \"rejeced\""),
                ("event = \"withdraw\"", "event = \"retract\""),
                ("guard = \"complete\"", "guard = \"done\""),
            ],
            "",
        ),
        with(&[("initial = \"draft\"", "initial = \"new\"")], ""),
        with(
            &[(
                "terminal = [\"approved\", \"rejected\"]",
                "terminal = [\"approved\", \"rejected\", \"gone\"]",
            )],
            "",
        ),
        with(
            &[],
            "\n[[transitions]]\nfrom = \"approved\"\nto = \"draft\"\nevent = \"withdraw\"\n",
        ),
        with(
            &[],
            "\n[[transitions]]\nfrom = \"submitted\"\nto = \"rejected\"\nevent = \"approve\"\n",
        ),
        with(
            &[],
            "\n[[transitions]]\nfrom = 3\nto = [\"x\", 2]\nevent = { a = 1 }\nguard = [1, 2]\n",
        ),
        with(&[("from = \"draft\"", "from = true")], ""),
        String::new(),
        "states = []\ninitial = \"\"\nevents = [\"\"]\ntransitions = []\nterminal = [1]\nguards = \"x\"\n".to_owned(),
        "states = \"a\"\ninitial = 1\nevents = [1]\ntransitions = [1, \"x\"]\nterminal = \"b\"\nguards = [\"g\"]\n".to_owned(),
        "states = [\"a\", \"b\"]\ninitial = \"a\"\nevents = [\"go\"]\ntransitions = [1, \"x\", [2]]\nentity = { k = 1 }\n".to_owned(),
        "states = [\"a\"]\ninitial = \"a\"\nevents = [\"go\"]\ntransitions = [{ from = \"a\", to = \"a\", event = \"go\", guard = \"toString\" }, { from = \"a\", to = \"a\", event = \"go\", guard = \"toString\" }]\n[guards]\n".to_owned(),
    ];
    for (i, text) in cases.iter().enumerate() {
        let codes = s.every_action(&format!("model/state/defect{i}.toml"), text);
        assert_eq!(codes, vec![1; 6], "case {i} should report defects");
    }
}

#[test]
fn toml_errors_and_unsupported_toml_match_node() {
    let s = Sandbox::new();
    let cases = [
        "a = \"unterminated\n",
        "a = 'unterminated",
        "a.b = 1\n",
        "a = \"\"\"multi\"\"\"\n",
        "a = '''multi'''\n",
        "a = 1.5\n",
        "a = 1979-05-27\n",
        "a = inf\n",
        "a = 1\na = 2\n",
        "[t]\n[t]\n",
        "t = 1\n[t]\n",
        "t = []\n[[t]]\n",
        "[t]\n[[t]]\n",
        "[[t]]\n[t]\n",
        "a = 1 b = 2\n",
        "[t] x\n",
        "[t\n",
        "[[t]\n",
        "[ t . u ]\n",
        "a = [1, 2\n\n",
        "a = [1 2]\n",
        "a = {x = 1\n",
        "a = {x = 1 y = 2}\n",
        "a = {x 1}\n",
        "a = {x = 1, x = 2}\n",
        "a = \"bad \\q escape\"\n",
        "a = \"tab\there\"\n",
        "a = \"line \\\ncontinued\"\n",
        "= 1\n",
        "a 1\n",
        "a =\n",
        "\u{feff}a = 1\n",
        "\"quoted key\" = 1\nb = 'lit\\eral'\n",
        "a = 12a\n",
        "a = -\n",
        "a = truex\n",
        "a = [\n  1, # c\n  'x',\n]\n[x]\ny = {}\n[[arr]]\n[[arr]]\nz=1#c\n",
    ];
    for (i, text) in cases.iter().enumerate() {
        let codes = s.every_action(&format!("model/state/toml{i}.toml"), text);
        assert!(codes.iter().all(|c| *c == 1), "case {i}: {codes:?}");
    }
}

#[test]
fn usage_and_file_errors_match_node() {
    let s = Sandbox::new();
    s.write("model/state/ok.toml", ENROLLMENT);
    fs::create_dir_all(s.0.join("model/dir.toml")).unwrap();
    let cases: [(&[&str], i32); 15] = [
        (&["machine"], 2),
        (&["machine", "--json"], 2),
        (&["machine", "check"], 2),
        (&["machine", "graph", "model/state/ok.toml"], 2),
        (&["machine", "check", "missing.toml"], 1),
        (&["machine", "criteria", "model/missing.toml", "--json"], 1),
        (&["machine", "check", "model/dir.toml"], 1),
        (&["machine", "check", "model/state/ok.toml", "--dir"], 2),
        (
            &[
                "machine",
                "check",
                "model/state/ok.toml",
                "--config",
                "nope.json",
            ],
            0,
        ),
        (
            &[
                "machine",
                "diagram",
                "model/state/ok.toml",
                "extra",
                "--dir",
                "x",
            ],
            0,
        ),
        (&["machine", "check", "./model/state/../state/ok.toml"], 0),
        (&["machine", "check", "model/state/ok.toml/"], 1),
        (&["machine", "check", "model/state/"], 1),
        (&["machine", "check", ".toml"], 1),
        (
            &[
                "machine",
                "criteria",
                "model/state/ok.toml",
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
    let out = s.compare(&["machine", "check", "model/state/ok.toml"]);
    assert_eq!(out.status.code(), Some(1), "invalid config fails first");
}

#[test]
fn documented_example_matches_node() {
    let skill =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/state-machine/SKILL.md");
    let text = fs::read_to_string(skill).unwrap();
    let start = text.find("```toml\n").unwrap() + "```toml\n".len();
    let example = &text[start..start + text[start..].find("```").unwrap()];
    let s = Sandbox::new();
    s.every_action("model/state/enrollment.toml", example);
}
