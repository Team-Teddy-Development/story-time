use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const COLON: &str = "---
id: ST-001
title: Fix: login fails
status: ready
type: story
---

## Story
As a user, I want login.

## Changelog
- 2026-11-01 created
";

const DUPLICATE_KEY: &str = "---
id: ST-001
title: Old title
status: ready
type: story
title: New title
---

## Story
As a user, I want login.
";

const PREAMBLE: &str = "---
id: ST-001
title: Login
status: ready
type: story
---
Prose that sits before the first section.

## Story
As a user, I want login.
";

const STUB_GH: &str = r#"
use std::{env, fs::OpenOptions, io::{Read, Write}};
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let mut log = OpenOptions::new().create(true).append(true).open("gh-calls").unwrap();
    writeln!(log, "{:?}", args).unwrap();
    match (args[0].as_str(), args[1].as_str()) {
        ("issue", "create") => println!("https://github.com/acme/shop/issues/11"),
        ("issue", "list") => println!("[]"),
        ("issue", "view") => println!("{{\"labels\":[],\"state\":\"OPEN\"}}"),
        _ => {}
    }
}
"#;

struct Sandbox(PathBuf);
impl Sandbox {
    fn new(story: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-push-lenient-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::create_dir_all(path.join("bin")).unwrap();
        fs::write(
            path.join(".story-time/settings.json"),
            json!({"tracker":"github","sync":"auto","github":{"repo":"acme/shop"}}).to_string(),
        )
        .unwrap();
        fs::write(path.join("backlog/ST-001.md"), story).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn backlog(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fs::read_dir(self.0.join("backlog"))
            .unwrap()
            .map(|e| {
                let path = e.unwrap().path();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect()
    }
    fn native(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .args(args)
            .env_clear()
            .env("PATH", self.0.join("bin"))
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn node(&self, args: &[&str]) -> Output {
        Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../skills/story-time/scripts/story.mjs"),
            )
            .args(args)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", &self.0)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    fn install_stub_gh(&self) {
        let mut rustc = Command::new("rustc")
            .args(["--crate-name", "gh_stub", "-o"])
            .arg(self.0.join("bin/gh"))
            .arg("-")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        rustc
            .stdin
            .take()
            .unwrap()
            .write_all(STUB_GH.as_bytes())
            .unwrap();
        let out = rustc.wait_with_output().unwrap();
        assert!(out.status.success(), "{}", text(&out.stderr));
    }
    fn gh_calls(&self) -> String {
        fs::read_to_string(self.0.join("gh-calls")).unwrap_or_default()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn show_reads_it_like_node(s: &Sandbox) {
    let node = s.node(&["show", "ST-001"]);
    let native = s.native(&["show", "ST-001"]);
    assert_eq!(node.status.code(), Some(0), "{}", text(&node.stderr));
    assert_eq!(native.status.code(), Some(0), "{}", text(&native.stderr));
    assert_eq!(text(&native.stdout), text(&node.stdout));
}

fn dry_run_plans_a_create_without_writing(s: &Sandbox) {
    let before = s.backlog();
    let out = s.native(&["push", "ST-001", "--dry-run", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let plan: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        plan,
        json!({"ok":true,"plan":[{"id":"ST-001","action":"create"}]})
    );
    assert_eq!(s.backlog(), before, "a dry run never writes");
    assert_eq!(s.gh_calls(), "", "a dry run never calls the tracker");
}

#[test]
fn push_plans_every_story_show_reads() {
    for story in [COLON, DUPLICATE_KEY, PREAMBLE] {
        let s = Sandbox::new(story);
        show_reads_it_like_node(&s);
        dry_run_plans_a_create_without_writing(&s);
    }
}

#[test]
fn push_reports_the_same_error_as_show() {
    for story in [
        "no frontmatter\n",
        "---\nid: ST-001\nnot a key line\n---\n",
        "---\ntitle: \"bad\"x\"\n---\n",
    ] {
        let s = Sandbox::new(story);
        let before = s.backlog();
        let show = s.native(&["show", "ST-001"]);
        let push = s.native(&["push", "ST-001", "--dry-run"]);
        assert_eq!(show.status.code(), Some(1), "{story}");
        assert_eq!(push.status.code(), show.status.code(), "{story}");
        assert_eq!(text(&push.stderr), text(&show.stderr), "{story}");
        assert_eq!(s.backlog(), before);
    }
}

#[test]
fn real_push_links_a_colon_title_and_writes_only_the_link() {
    let s = Sandbox::new(COLON);
    s.install_stub_gh();
    let out = s.native(&["push", "ST-001", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let written = fs::read_to_string(s.0.join("backlog/ST-001.md")).unwrap();
    assert_eq!(
        written,
        format!(
            "---
id: ST-001
title: \"Fix: login fails\"
status: ready
type: story
remote: 11
remote_url: \"https://github.com/acme/shop/issues/11\"
updated: {date}
---

## Story
As a user, I want login.

## Changelog
- 2026-11-01 created
- {date} pushed to tracker as 11
"
        )
    );
    let reread = s.native(&["show", "ST-001", "--json"]);
    let reread: Value = serde_json::from_slice(&reread.stdout).unwrap();
    assert_eq!(reread["title"], "Fix: login fails");
}

#[test]
fn real_push_refuses_a_rewrite_that_would_drop_text_before_any_tracker_call() {
    for (story, loss) in [
        (DUPLICATE_KEY, "duplicate frontmatter key title"),
        (PREAMBLE, "text before the first section"),
    ] {
        let s = Sandbox::new(story);
        s.install_stub_gh();
        let before = s.backlog();
        let out = s.native(&["push", "ST-001"]);
        assert_eq!(out.status.code(), Some(1), "{}", text(&out.stdout));
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains("ST-001") && stderr.contains(loss),
            "{stderr}"
        );
        assert_eq!(s.backlog(), before, "the story file is unchanged");
        assert_eq!(s.gh_calls(), "", "no tracker call before the refusal");
    }
}
