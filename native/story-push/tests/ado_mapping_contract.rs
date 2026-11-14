use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const FAKE_PAT: &str = "fake-pat-never-printed";

const STUB_AZ: &str = r#"
use std::{env, fs::{self, OpenOptions}, io::Write};
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut log = OpenOptions::new().create(true).append(true).open(env::var("AZ_LOG").unwrap()).unwrap();
    writeln!(log, "{:?}", args).unwrap();
    if let Some(at) = args.iter().position(|a| a == "--in-file") {
        writeln!(log, "patch {}", fs::read_to_string(&args[at + 1]).unwrap()).unwrap();
    }
    println!("{{\"id\":42,\"rev\":1,\"fields\":{{\"System.Tags\":\"\"}}}}");
}
"#;

const STORIES: [(&str, &str); 5] = [
    (
        "ST-001",
        "---
id: ST-001
title: \"ST-001: Reset password\"
status: ready
type: story
epic: EP-1
size: 3
---
## Story
As a user, I want a reset link
so that I can sign in.

Second <paragraph> & more.

## Acceptance Criteria
- [ ] Given an email <exists> & is valid, when I ask, then a link is sent
- [x] Given a used token, when I open it, then it fails

## Out of Scope
- SMS reset
",
    ),
    (
        "ST-002",
        "---
id: ST-002
title: \"ST-002: Login fails\"
status: ready
type: bug
size: 1
---
## Story
Login fails on Safari.

## Steps to Reproduce
1. Open <Safari>
2. Log in

## Expected
It works.

## Actual
It fails & logs out.

## Acceptance Criteria
- [ ] Given Safari, when I log in, then it works
",
    ),
    (
        "ST-003",
        "---
id: ST-003
title: \"ST-003: Which queue?\"
status: refined
type: spike
size: 2
---
## Question
Which queue fits?

## Exit Criteria
- [ ] A recommendation is written
",
    ),
    (
        "ST-004",
        "---
id: ST-004
title: \"ST-004: Rename settings\"
status: in-progress
type: chore
epic: EP-2
size: 0.5
remote: 12
---
## Story
Rename the key.

## Acceptance Criteria
- [ ] Given the old key, when loaded, then it maps to the new key
",
    ),
    (
        "ST-005",
        "---
id: ST-005
title: \"ST-005: Crash on save\"
status: archived
type: bug
remote: 13
---
## Story
Saving crashes.

## Steps to Reproduce
Click save.

## Acceptance Criteria
- [ ] Given a story, when I save, then nothing crashes
",
    ),
];

struct Sandbox(PathBuf);
impl Sandbox {
    fn new(settings: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-ado-mapping-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::create_dir_all(path.join("bin")).unwrap();
        for (id, text) in STORIES {
            fs::write(path.join(format!("backlog/{id}.md")), text).unwrap();
        }
        fs::write(path.join(".story-time/settings.json"), settings).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn compile_stub(dir: &Path) {
    let mut rustc = Command::new("rustc")
        .args(["--edition=2021", "--crate-name", "az", "-o"])
        .arg(dir.join("az"))
        .arg("-")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    rustc
        .stdin
        .take()
        .unwrap()
        .write_all(STUB_AZ.as_bytes())
        .unwrap();
    let out = rustc.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn legacy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs")
}

fn run(native: bool, settings: &str, stub: &Path, args: &[&str]) -> (Output, String) {
    let s = Sandbox::new(settings);
    let log = s.0.join("az-calls");
    let mut cmd = if native {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_story"));
        cmd.env_clear().env("PATH", stub);
        cmd
    } else {
        let mut cmd = Command::new("node");
        let path = format!(
            "{}:{}",
            stub.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        cmd.arg(legacy()).env_clear().env("PATH", path);
        cmd
    };
    let out = cmd
        .args(args)
        .env("HOME", &s.0)
        .env("AZ_LOG", &log)
        .env("AZURE_DEVOPS_EXT_PAT", FAKE_PAT)
        .current_dir(&s.0)
        .output()
        .unwrap();
    (out, fs::read_to_string(&log).unwrap_or_default())
}

fn calls(log: &str) -> Vec<Vec<String>> {
    log.lines()
        .filter(|line| line.starts_with('['))
        .map(|line| {
            serde_json::from_str::<Vec<String>>(line).unwrap_or_else(|e| panic!("{e}: {line}"))
        })
        .filter(|args| args.get(1).map(String::as_str) == Some("work-item"))
        .filter(|args| matches!(args[2].as_str(), "create" | "update"))
        .collect()
}

/// Node writes every tag into the update's `--fields`; native merges tags through a
/// revision-guarded patch instead, so its update carries no `System.Tags=` field.
fn split_tags(mut call: Vec<String>) -> (Vec<String>, Option<String>) {
    let tags = call
        .iter()
        .position(|a| a.starts_with("System.Tags="))
        .map(|at| call.remove(at)["System.Tags=".len()..].to_owned());
    if call.last().is_some_and(|a| a == "--fields") {
        call.pop();
    }
    if let Some(at) = call.iter().position(|a| a == "--fields") {
        if call.get(at + 1).is_some_and(|a| a.starts_with("--")) {
            call.remove(at);
        }
    }
    (call, tags)
}

fn patched_tags(log: &str) -> Vec<String> {
    log.lines()
        .filter_map(|line| line.strip_prefix("patch "))
        .map(|patch| {
            let value: serde_json::Value = serde_json::from_str(patch).unwrap();
            value[1]["value"].as_str().unwrap().to_owned()
        })
        .collect()
}

fn settings(process: &str, area: bool) -> String {
    let paths = if area {
        r#","areaPath":"Shop\\Web","iterationPath":"Shop\\Sprint 1""#
    } else {
        ""
    };
    format!(
        r#"{{"tracker":"ado","sync":"ask","ado":{{"organization":"https://dev.azure.com/acme","project":"Team's Board","process":"{process}"{paths}}}}}"#
    )
}

#[test]
fn push_and_export_send_the_same_ado_fields_as_node() {
    let stub = std::env::temp_dir().join(format!("story-ado-mapping-az-{}", std::process::id()));
    fs::create_dir_all(&stub).unwrap();
    compile_stub(&stub);
    for process in ["agile", "scrum", "basic"] {
        for area in [true, false] {
            let settings = settings(process, area);
            let push = ["push", "--all", "--json"];
            let (node, node_log) = run(false, &settings, &stub, &push);
            let (native, native_log) = run(true, &settings, &stub, &push);
            let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
            assert!(node.status.success(), "node: {}", text(&node.stderr));
            assert!(native.status.success(), "native: {}", text(&native.stderr));
            let node_calls = calls(&node_log);
            let native_calls = calls(&native_log);
            assert_eq!(node_calls.len(), native_calls.len(), "{process} {area}");
            assert!(
                node_calls.iter().filter(|c| c[2] == "create").count() == 3
                    && node_calls
                        .iter()
                        .filter(|c| c.iter().any(|a| a == "--title"))
                        .count()
                        == 5,
                "every story must reach az: {node_calls:?}"
            );
            let mut native_patches = patched_tags(&native_log).into_iter();
            for (node_call, native_call) in node_calls.into_iter().zip(native_calls) {
                if node_call[2] == "create" {
                    assert_eq!(native_call, node_call, "create {process} area={area}");
                } else if node_call.iter().any(|a| a.starts_with("--title")) {
                    let (node_call, tags) = split_tags(node_call);
                    assert_eq!(native_call, node_call, "update {process} area={area}");
                    assert_eq!(native_patches.next(), tags, "tags {process} area={area}");
                } else {
                    assert_eq!(native_call, node_call, "state {process} area={area}");
                }
            }
            assert!(!native_log.contains(FAKE_PAT) && !text(&native.stdout).contains(FAKE_PAT));

            for (id, _) in STORIES {
                for extra in [&[][..], &["--command"][..], &["--json"][..]] {
                    let mut args = vec!["export", id, "--format", "ado"];
                    args.extend(extra);
                    let (node, _) = run(false, &settings, &stub, &args);
                    let (native, _) = run(true, &settings, &stub, &args);
                    assert_eq!(text(&native.stdout), text(&node.stdout), "{args:?}");
                    assert_eq!(native.status.code(), node.status.code(), "{args:?}");
                }
            }
        }
    }
    let _ = fs::remove_dir_all(&stub);
}

#[test]
fn the_ado_field_mapping_lives_only_in_story_rs() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut copies = Vec::new();
    for entry in fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == "story.rs" || path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        for reference in [
            "System.Description",
            "System.AreaPath",
            "System.IterationPath",
            "AcceptanceCriteria",
            "ReproSteps",
            "to_html",
        ] {
            if text.contains(reference) {
                copies.push(format!("{} names {reference}", path.display()));
            }
        }
    }
    assert!(
        copies.is_empty(),
        "map story fields to ADO only in src/story.rs:\n{}",
        copies.join("\n")
    );
}
