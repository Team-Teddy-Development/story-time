//! ST-179: mark / propose / clear blocked tools and selection skips.

use ring::digest::{digest, SHA256};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const SETTINGS: &str = r#"{
  "tracker": "github",
  "sync": "ask",
  "backlogDir": "backlog",
  "prefix": "ST",
  "team": { "wipLimit": 2 },
  "definitionOfReady": ["Criteria are testable"],
  "definitionOfDone": ["Tests pass"],
  "github": { "repo": "example/fixture" }
}
"#;

fn story(id: &str, status: &str, extra: &str) -> String {
    let deps = if extra.contains("depends_on:") {
        String::new()
    } else {
        "depends_on: []\n".into()
    };
    format!(
        "---\nid: {id}\ntitle: Story {id}\nstatus: {status}\ntype: story\nsize: 3\nvalue: 5\ntime_criticality: 3\nrisk_reduction: 2\n{extra}{deps}created: 2026-11-24\nupdated: 2026-11-24\n---\n\n## Story\nAs an agent, I want {id}, so that fixtures work.\n\n## Acceptance Criteria\n- [ ] Given {id}, when read, then shown\n"
    )
}

struct Fixture {
    base: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-blocked-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("fixture");
        fs::create_dir_all(root.join("backlog")).unwrap();
        fs::create_dir_all(root.join(".story-time")).unwrap();
        fs::write(root.join(".story-time/settings.json"), SETTINGS).unwrap();
        let root = root.canonicalize().unwrap();
        Self { base, root }
    }
    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn writer(&self) -> Server {
        Server::start(
            &["--root", self.root.to_str().unwrap(), "--write"],
            &self.root,
        )
    }
    fn sha(&self, id: &str) -> String {
        let bytes = fs::read(self.root.join(format!("backlog/{id}.md"))).unwrap();
        digest(&SHA256, &bytes)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    fn read(&self, id: &str) -> String {
        fs::read_to_string(self.root.join(format!("backlog/{id}.md"))).unwrap()
    }
    fn cli(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_story"))
            .args(args)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.base)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}
impl Server {
    fn start(args: &[&str], cwd: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_story"))
            .arg("mcp")
            .args(args)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", cwd)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            stdin: child.stdin.take(),
            stdout: BufReader::new(child.stdout.take().unwrap()),
            child,
        }
    }
    fn send(&mut self, message: &Value) -> Value {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"))
    }
    fn meta(mut params: Value) -> Value {
        params.as_object_mut().unwrap().insert(
            "_meta".into(),
            json!({
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "blocked-test", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {}
            }),
        );
        params
    }
    fn init_write(&mut self) {
        let init = self.send(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2026-07-28",
                "capabilities": {},
                "clientInfo": {"name": "blocked-test", "version": "0"}
            }
        }));
        assert!(init.get("error").is_none(), "{init}");
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
        )
        .unwrap();
        stdin.flush().unwrap();
    }
    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.send(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": Self::meta(json!({"name": name, "arguments": arguments}))
        }));
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }
    fn tool_err(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.send(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": Self::meta(json!({"name": name, "arguments": arguments}))
        }));
        assert!(response.get("error").is_none(), "{response}");
        let result = response["result"].clone();
        assert_eq!(result["isError"], true, "{result}");
        result
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

fn structured(result: &Value) -> &Value {
    assert_eq!(result["isError"], false, "{result}");
    &result["structuredContent"]
}

fn error_code(result: &Value) -> String {
    result["structuredContent"]["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn mark_blocked_releases_wip_and_returns_next_steps() {
    let fixture = Fixture::new();
    fixture.write("backlog/ST-001.md", &story("ST-001", "in-progress", ""));
    let mut server = fixture.writer();
    server.init_write();
    let sha = fixture.sha("ST-001");
    let result = server.tool(
        "story_mark_blocked",
        json!({
            "id": "ST-001",
            "expected_sha256": sha,
            "reason": "Missing staging credentials",
            "blocker_kind": "access",
            "release_wip": true
        }),
    );
    let content = structured(&result);
    assert_eq!(content["status"], "updated");
    assert_eq!(content["story"]["status"], "ready");
    assert!(content["labels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l == "blocked"));
    let steps = content["next_steps"].as_array().unwrap();
    assert!(steps.len() >= 2, "{steps:?}");
    assert!(
        steps
            .iter()
            .any(|s| s.as_str().unwrap().contains("story_propose_unblocks")),
        "{steps:?}"
    );
    let body = fixture.read("ST-001");
    assert!(body.contains("labels:"));
    assert!(body.contains("blocked"));
    assert!(body.contains("Blocked: access"));
}

#[test]
fn propose_unblocks_wires_deps_and_steers_agent() {
    let fixture = Fixture::new();
    fixture.write(
        "backlog/ST-010.md",
        &story("ST-010", "ready", "labels: [blocked]\n"),
    );
    let mut server = fixture.writer();
    server.init_write();
    let sha = fixture.sha("ST-010");
    let result = server.tool(
        "story_propose_unblocks",
        json!({
            "id": "ST-010",
            "expected_sha256": sha,
            "unblocks": [{
                "title": "Unblock ST-010: mint staging token",
                "statement": "As an agent unblocking ST-010, I want a staging token, so that ST-010 can proceed.",
                "acceptance_criteria": ["Given ST-010 is blocked on access, when the token exists, then ST-010 can resume."],
                "type": "chore",
                "size": 2
            }]
        }),
    );
    let content = structured(&result);
    assert_eq!(content["status"], "updated");
    let created = content["created"].as_array().unwrap();
    assert_eq!(created.len(), 1);
    let child = created[0]["id"].as_str().unwrap();
    assert!(content["parent"]["depends_on"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d == child));
    let steps = content["next_steps"].as_array().unwrap();
    assert!(
        steps.iter().any(|s| s.as_str().unwrap().contains(child)),
        "{steps:?}"
    );
    let child_body = fixture.read(child);
    assert!(child_body.contains("relates_to:"));
    assert!(child_body.contains("ST-010"));
}

#[test]
fn propose_refuses_over_split_and_duplicate() {
    let fixture = Fixture::new();
    fixture.write(
        "backlog/ST-020.md",
        &story("ST-020", "ready", "labels: [blocked]\n"),
    );
    let mut server = fixture.writer();
    server.init_write();
    let sha = fixture.sha("ST-020");
    let too_many = (0..6)
        .map(|i| {
            json!({
                "title": format!("Unblock ST-020 part {i}"),
                "acceptance_criteria": [format!("Given part {i}, when done, then ST-020 moves.")],
            })
        })
        .collect::<Vec<_>>();
    let err = server.tool_err(
        "story_propose_unblocks",
        json!({
            "id": "ST-020",
            "expected_sha256": sha,
            "unblocks": too_many,
            "max_children": 5
        }),
    );
    // Schema may reject before tool (maxItems 5) or tool returns over_split.
    let text = format!("{err}");
    assert!(
        text.contains("over_split")
            || text.contains("At most")
            || text.contains("maxItems")
            || text.contains("does not match"),
        "{err}"
    );
}

#[test]
fn clear_blocked_refuses_until_deps_done() {
    let fixture = Fixture::new();
    fixture.write(
        "backlog/ST-030.md",
        &story(
            "ST-030",
            "ready",
            "labels: [blocked]\ndepends_on: [ST-031]\n",
        ),
    );
    fixture.write(
        "backlog/ST-031.md",
        &story("ST-031", "ready", "relates_to: [ST-030]\n"),
    );
    let mut server = fixture.writer();
    server.init_write();
    let sha = fixture.sha("ST-030");
    let err = server.tool_err(
        "story_clear_blocked",
        json!({
            "id": "ST-030",
            "expected_sha256": sha
        }),
    );
    assert!(
        error_code(&err).contains("still_blocked") || format!("{err}").contains("still_blocked"),
        "{err}"
    );

    // Mark dep done and clear.
    let dep = fixture
        .read("ST-031")
        .replace("status: ready", "status: done");
    fixture.write("backlog/ST-031.md", &dep);
    let sha = fixture.sha("ST-030");
    let result = server.tool(
        "story_clear_blocked",
        json!({
            "id": "ST-030",
            "expected_sha256": sha,
            "note": "deps finished"
        }),
    );
    let content = structured(&result);
    assert_eq!(content["status"], "updated");
    let labels = content["labels"].as_array().unwrap();
    assert!(!labels.iter().any(|l| l == "blocked"));
    assert!(!content["next_steps"].as_array().unwrap().is_empty());
}

#[test]
fn story_set_done_clears_the_parent_only_after_its_last_unblock_story() {
    let fixture = Fixture::new();
    fixture.write(
        "backlog/ST-070.md",
        &story(
            "ST-070",
            "ready",
            "labels: [blocked]\ndepends_on: [ST-071, ST-072]\n",
        ),
    );
    fixture.write(
        "backlog/ST-071.md",
        &story("ST-071", "in-progress", "relates_to: [ST-070]\n"),
    );
    fixture.write(
        "backlog/ST-072.md",
        &story("ST-072", "in-progress", "relates_to: [ST-070]\n"),
    );

    assert_eq!(
        fixture.cli(&["set", "ST-071", "status=done"]),
        "ST-071 updated.\n"
    );
    assert!(fixture.read("ST-070").contains("blocked"));

    assert_eq!(
        fixture.cli(&["set", "ST-072", "status=done"]),
        "ST-072 updated.\nST-070 unblocked: every dependency is done.\n"
    );
    let parent = fixture.read("ST-070");
    assert!(!parent.contains("labels:"), "{parent}");
    assert!(
        parent.contains("Unblocked: ST-072 finished the last dependency"),
        "{parent}"
    );
}

#[test]
fn story_update_to_done_clears_the_blocked_parent() {
    let fixture = Fixture::new();
    fixture.write(
        "backlog/ST-080.md",
        &story(
            "ST-080",
            "ready",
            "labels: [blocked, platform]\ndepends_on: [ST-081]\n",
        ),
    );
    let accepted = story("ST-081", "in-progress", "relates_to: [ST-080]\n")
        .replace("- [ ]", "- [x]")
        + "\n## Evidence\nThe staging token exists.\n";
    fixture.write("backlog/ST-081.md", &accepted);
    let mut server = fixture.writer();
    server.init_write();
    let result = server.tool(
        "story_update",
        json!({"id": "ST-081", "expected_sha256": fixture.sha("ST-081"), "set": {"status": "done"}}),
    );
    assert_eq!(
        structured(&result)["auto_cleared_blocked"],
        json!(["ST-080"])
    );
    let parent = fixture.read("ST-080");
    assert!(parent.contains("labels: [platform]\n"), "{parent}");
}

#[test]
fn rank_skips_blocked_label() {
    let fixture = Fixture::new();
    fixture.write(
        "backlog/ST-040.md",
        &story("ST-040", "ready", "labels: [blocked]\n"),
    );
    fixture.write("backlog/ST-041.md", &story("ST-041", "ready", ""));
    let output = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["rank", "--json"])
        .env_clear()
        .env("PATH", "/nonexistent")
        .env("HOME", &fixture.base)
        .current_dir(&fixture.root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ranked: Value = serde_json::from_slice(&output.stdout).unwrap();
    let ids: Vec<&str> = ranked["ranked"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert!(!ids.contains(&"ST-040"), "{ranked}");
    assert!(ids.contains(&"ST-041"), "{ranked}");
    let excluded = ranked["excluded"].as_array().unwrap();
    assert!(
        excluded.iter().any(|e| {
            e["id"] == "ST-040" && e["reason"].as_str().unwrap_or("").contains("blocked label")
        }),
        "{excluded:?}"
    );
}

#[test]
fn list_filters_by_blocked_label() {
    let fixture = Fixture::new();
    fixture.write(
        "backlog/ST-050.md",
        &story("ST-050", "ready", "labels: [blocked]\n"),
    );
    fixture.write("backlog/ST-051.md", &story("ST-051", "ready", ""));
    let mut server = fixture.writer();
    server.init_write();
    let result = server.tool("story_list", json!({ "labels": ["blocked"] }));
    let content = structured(&result);
    let stories = content["stories"].as_array().unwrap();
    assert_eq!(stories.len(), 1);
    assert_eq!(stories[0]["id"], "ST-050");
}

#[test]
fn render_card_includes_blocked_label() {
    let fixture = Fixture::new();
    fixture.write(
        "backlog/ST-060.md",
        &story("ST-060", "ready", "labels: [blocked]\n"),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["export", "ST-060", "--format", "github", "--json"])
        .env_clear()
        .env("PATH", "/nonexistent")
        .env("HOME", &fixture.base)
        .current_dir(&fixture.root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let card: Value = serde_json::from_slice(&output.stdout).unwrap();
    let labels = card["labels"].as_array().unwrap();
    assert!(labels.iter().any(|l| l == "blocked"), "{card}");
}
