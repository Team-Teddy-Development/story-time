//! ST-194: the `sprint` field through the local MCP: `story_list` filters by it and
//! `story_update` sets it through the validation `story set` uses.

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

const SETTINGS: &str = r#"{"tracker": "files", "sync": "off", "team": {"wipLimit": 2}}"#;

fn story(id: &str, extra: &str) -> String {
    format!(
        "---\nid: {id}\ntitle: Story {id}\nstatus: draft\ntype: story\n{extra}depends_on: []\ncreated: 2026-11-24\nupdated: 2026-11-24\n---\n\n## Story\nAs an agent, I want {id}, so that fixtures work.\n"
    )
}

struct Fixture {
    base: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-mcp-sprint-{}-{}",
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

fn fixture() -> Fixture {
    let f = Fixture::new();
    for (id, extra) in [
        ("ST-001", "sprint: 2026-S23\n"),
        ("ST-002", "sprint: 2026-S24\n"),
        ("ST-003", ""),
    ] {
        fs::write(f.root.join(format!("backlog/{id}.md")), story(id, extra)).unwrap();
    }
    f
}

fn start(f: &Fixture) -> Server {
    let mut server = Server::start(&["--root", f.root.to_str().unwrap(), "--write"], &f.root);
    server.init_write();
    server
}

#[test]
fn story_list_filters_by_sprint_and_summaries_carry_it() {
    let f = fixture();
    let mut server = start(&f);
    let all = server.tool("story_list", json!({}));
    let rows = all["structuredContent"]["stories"].as_array().unwrap();
    let sprints: Vec<_> = rows.iter().map(|s| s["sprint"].clone()).collect();
    assert_eq!(sprints, [json!("2026-S23"), json!("2026-S24"), Value::Null]);
    let one = server.tool("story_list", json!({"sprint": "2026-S23"}));
    let ids: Vec<_> = one["structuredContent"]["stories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, ["ST-001"]);
}

#[test]
fn story_list_refuses_a_malformed_sprint_filter() {
    let f = fixture();
    let mut server = start(&f);
    let err = server.tool_err("story_list", json!({"sprint": "next"}));
    assert_eq!(
        err["structuredContent"]["error"]["code"],
        "invalid_argument"
    );
}

#[test]
fn story_update_sets_clears_and_refuses_sprint_like_story_set() {
    let f = fixture();
    let mut server = start(&f);
    let sha = f.sha("ST-003");
    let done = server.tool(
        "story_update",
        json!({"id": "ST-003", "expected_sha256": sha, "set": {"sprint": "2026-S25"}}),
    );
    assert_eq!(done["isError"], false, "{done}");
    let text = f.read("ST-003");
    assert!(text.contains("\nsprint: 2026-S25\n"), "{text}");
    assert!(
        text.lines().any(|l| l.ends_with(" set sprint=2026-S25")),
        "{text}"
    );

    let before = f.read("ST-003");
    let bad = server.tool_err(
        "story_update",
        json!({"id": "ST-003", "expected_sha256": f.sha("ST-003"), "set": {"sprint": "2026-23"}}),
    );
    assert_eq!(bad["structuredContent"]["error"]["code"], "invalid_value");
    assert_eq!(f.read("ST-003"), before);

    let cleared = server.tool(
        "story_update",
        json!({"id": "ST-003", "expected_sha256": f.sha("ST-003"), "set": {"sprint": ""}}),
    );
    assert_eq!(cleared["isError"], false, "{cleared}");
    assert!(!f.read("ST-003").contains("sprint:"));
}
