use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

const IMAGE: &str = "neo4j:5.26.30-community";
const PASSWORD: &str = "fake-graph-password-st011";
const PASSWORD_ENV: &str = "STORY_GRAPH_TEST_PASSWORD";
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn docker(args: &[&str]) -> Option<String> {
    let output = Command::new("docker").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// A disposable Neo4j container on a free loopback port, removed with its volumes on drop.
struct Neo4j {
    name: String,
    url: String,
}
impl Neo4j {
    fn start() -> Option<Self> {
        Self::start_with(&format!("NEO4J_AUTH=neo4j/{PASSWORD}"))
    }

    /// A Neo4j that asks for no credentials, as the sidecar of the hosted server runs.
    fn open() -> Option<Self> {
        Self::start_with("NEO4J_AUTH=none")
    }

    fn start_with(auth: &str) -> Option<Self> {
        if docker(&["info", "--format", "{{.ServerVersion}}"]).is_none() {
            eprintln!("SKIP: docker is unavailable, so the Neo4j graph tests cannot run.");
            return None;
        }
        let name = format!(
            "story-graph-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        docker(&[
            "run",
            "-d",
            "--name",
            &name,
            "--label",
            "story-time-test=ST-011",
            "-p",
            "127.0.0.1::7474",
            "-e",
            auth,
            "-e",
            "NEO4J_server_memory_heap_max__size=256m",
            "-e",
            "NEO4J_server_memory_pagecache_size=32m",
            IMAGE,
        ])
        .unwrap_or_else(|| panic!("docker run {IMAGE} failed"));
        let mut neo4j = Self {
            url: String::new(),
            name,
        };
        let port = docker(&["port", &neo4j.name, "7474/tcp"]).expect("docker port");
        let port = port.lines().next().unwrap().rsplit(':').next().unwrap();
        neo4j.url = format!("http://127.0.0.1:{port}");
        let started = Instant::now();
        while neo4j.cypher("RETURN 1", json!({})).is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(180),
                "Neo4j did not start"
            );
            std::thread::sleep(Duration::from_millis(500));
        }
        Some(neo4j)
    }

    fn cypher(&self, statement: &str, parameters: Value) -> Option<Vec<Value>> {
        let body: Value = reqwest::blocking::Client::new()
            .post(format!("{}/db/neo4j/tx/commit", self.url))
            .basic_auth("neo4j", Some(PASSWORD))
            .json(&json!({"statements": [{"statement": statement, "parameters": parameters}]}))
            .send()
            .ok()?
            .json()
            .ok()?;
        if body["errors"].as_array().is_none_or(|e| !e.is_empty()) {
            return None;
        }
        Some(
            body["results"][0]["data"]
                .as_array()?
                .iter()
                .map(|row| row["row"].clone())
                .collect(),
        )
    }

    /// Every node and edge with one repository identity, sorted, as plain strings.
    fn dump(&self, repository: &str) -> (Vec<String>, Vec<String>) {
        let nodes = self
            .cypher(
                "MATCH (n {repository: $r}) RETURN labels(n), n.id, n.title, n.status, n.missing",
                json!({"r": repository}),
            )
            .unwrap();
        let edges = self
            .cypher(
                "MATCH (a {repository: $r})-[e]->(b) RETURN labels(a)[0] + ':' + a.id + ' ' + type(e) + ' ' + labels(b)[0] + ':' + b.id + ' ' + coalesce(b.repository, '')",
                json!({"r": repository}),
            )
            .unwrap();
        let mut nodes: Vec<String> = nodes.iter().map(Value::to_string).collect();
        let mut edges: Vec<String> = edges
            .iter()
            .map(|e| e[0].as_str().unwrap().to_owned())
            .collect();
        nodes.sort();
        edges.sort();
        (nodes, edges)
    }

    fn stop(&self) {
        docker(&["stop", "-t", "1", &self.name]).expect("docker stop");
    }
}
impl Drop for Neo4j {
    fn drop(&mut self) {
        let _ = docker(&["rm", "-f", "-v", &self.name]);
    }
}

fn story(id: &str, title: &str, status: &str, extra: &str) -> String {
    format!(
        "---\nid: {id}\ntitle: {title}\nstatus: {status}\ntype: story\n{extra}created: 2026-11-14\nupdated: 2026-11-14\n---\n\n## Story\nAs a planner, I want {title}, so that the fixture has data.\n"
    )
}

struct Fixture {
    base: PathBuf,
    roots: Vec<PathBuf>,
}
impl Fixture {
    fn new(settings: &[&str]) -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-graph-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let roots = settings
            .iter()
            .enumerate()
            .map(|(n, settings)| {
                let root = base.join(format!("repo{n}"));
                fs::create_dir_all(root.join("backlog")).unwrap();
                fs::create_dir_all(root.join(".story-time")).unwrap();
                fs::write(root.join(".story-time/settings.json"), settings).unwrap();
                root.canonicalize().unwrap()
            })
            .collect();
        Self { base, roots }
    }
    fn write(&self, root: usize, id: &str, text: &str) {
        fs::write(self.roots[root].join(format!("backlog/{id}.md")), text).unwrap();
    }
    fn remove(&self, root: usize, id: &str) {
        fs::remove_file(self.roots[root].join(format!("backlog/{id}.md"))).unwrap();
    }
    fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, into);
                } else {
                    into.insert(path.clone(), fs::read(&path).unwrap());
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&self.base, &mut files);
        files
    }
    fn server(&self, neo4j: &Neo4j, extra: &[&str]) -> Server {
        let mut args: Vec<String> = Vec::new();
        for root in &self.roots {
            args.extend(["--root".into(), root.to_str().unwrap().into()]);
        }
        args.extend(
            [
                "--graph",
                "--neo4j-url",
                &neo4j.url,
                "--neo4j-user",
                "neo4j",
                "--neo4j-password-env",
                PASSWORD_ENV,
            ]
            .map(String::from),
        );
        args.extend(extra.iter().map(|a| a.to_string()));
        Server::start(&args, &self.base, &[(PASSWORD_ENV, PASSWORD)])
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
    fn start(args: &[String], cwd: &Path, env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_story"))
            .arg("mcp")
            .args(args)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", cwd)
            .envs(env.iter().copied())
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut server = Self {
            child,
            stdin,
            stdout,
        };
        let init = server.send("initialize", json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "graph-test", "version": "1"}}));
        assert!(init.get("result").is_some(), "{init}");
        server
    }
    fn send(&mut self, method: &str, params: Value) -> Value {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let stdin = self.stdin.as_mut().unwrap();
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        assert!(
            self.stdout.read_line(&mut line).unwrap() > 0,
            "server closed stdout"
        );
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"))
    }
    /// The raw `tools/call` result, so callers can check both success and error shapes.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.send("tools/call", json!({"name": name, "arguments": arguments}));
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }
    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call(name, arguments);
        assert_eq!(result["isError"], false, "{result}");
        result["structuredContent"].clone()
    }
    fn error(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call(name, arguments);
        assert_eq!(result["isError"], true, "{result}");
        result["structuredContent"]["error"].clone()
    }
    fn tools(&mut self) -> Vec<String> {
        self.send("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }
    fn finish(mut self) -> (String, String) {
        drop(self.stdin.take());
        let mut rest = String::new();
        self.stdout.read_to_string(&mut rest).unwrap();
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(self.child.wait().unwrap().success());
        (rest, stderr)
    }
}

const GITHUB: &str =
    r#"{"tracker": "github", "sync": "ask", "prefix": "ST", "github": {"repo": "example/one"}}"#;
const GITEA: &str = r#"{"tracker": "gitea", "sync": "ask", "prefix": "ST", "gitea": {"baseUrl": "http://gitea.test:3000", "owner": "team", "repo": "two"}}"#;
const FILES: &str = r#"{"tracker": "files", "prefix": "ST"}"#;

fn sha256(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn graph_tools_appear_only_with_the_graph_flag() {
    let fixture = Fixture::new(&[GITHUB]);
    let root = fixture.roots[0].to_str().unwrap().to_owned();
    let mut plain = Server::start(&["--root".into(), root], &fixture.base, &[]);
    let tools = plain.tools();
    assert!(
        !tools.iter().any(|t| t.starts_with("story_graph")),
        "{tools:?}"
    );
    plain.finish();

    let root = fixture.roots[0].to_str().unwrap();
    let args = [
        "--root",
        root,
        "--graph",
        "--neo4j-url",
        "http://127.0.0.1:9",
        "--neo4j-user",
        "neo4j",
        "--neo4j-password-env",
        PASSWORD_ENV,
    ]
    .map(String::from);
    let mut graph = Server::start(&args, &fixture.base, &[(PASSWORD_ENV, PASSWORD)]);
    let listed = graph.send("tools/list", json!({}))["result"]["tools"].clone();
    let open_world: Vec<(String, bool)> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_owned(),
                t["annotations"]["openWorldHint"].as_bool().unwrap(),
            )
        })
        .collect();
    assert!(
        open_world
            .iter()
            .all(|(name, open)| *open == name.starts_with("story_graph")),
        "only the Neo4j tools reach outside the repository: {open_world:?}"
    );
    graph.finish();
}

#[test]
fn sync_projects_repository_scoped_stories_and_typed_directed_edges() {
    let Some(neo4j) = Neo4j::start() else { return };
    let fixture = Fixture::new(&[GITHUB, GITEA, FILES]);
    for root in 0..3 {
        fixture.write(
            root,
            "ST-001",
            &story("ST-001", &format!("Base {root}"), "done", "epic: EP-1\n"),
        );
        fixture.write(root, "ST-002", &story("ST-002", "Second", "ready", "depends_on: [ST-001, ST-099]\nepic: EP-1\nremote_url: \"https://github.com/example/one/issues/2\"\n"));
    }
    let mut server = fixture.server(&neo4j, &[]);
    let tools = server.tools();
    assert!(
        tools.contains(&"story_graph_sync".to_owned()) && tools.contains(&"story_graph".to_owned()),
        "{tools:?}"
    );
    let path_identity = format!(
        "path:{}",
        sha256(fixture.roots[2].to_str().unwrap().as_bytes())
    );
    for (n, identity) in [
        "github:example/one".to_owned(),
        "gitea:gitea.test:3000/team/two".to_owned(),
        path_identity,
    ]
    .iter()
    .enumerate()
    {
        let key = format!("repo{n}");
        let result = server.ok("story_graph_sync", json!({"repository": key}));
        assert_eq!(result["repository"], key.as_str());
        assert_eq!(result["repository_id"], identity.as_str());
        assert_eq!(result["stories"], 2);
        assert_eq!(result["missing"], 1);
        assert_eq!(result["epics"], 1);
        assert_eq!(result["depends_on"], 2);
        assert_eq!(result["in_epic"], 2);
        let (nodes, edges) = neo4j.dump(identity);
        assert_eq!(
            nodes,
            [
                r#"[["Epic"],"EP-1",null,null,null]"#.to_owned(),
                format!(r#"[["Story"],"ST-001","Base {n}","done",false]"#),
                r#"[["Story"],"ST-002","Second","ready",false]"#.to_owned(),
                r#"[["Story"],"ST-099",null,null,true]"#.to_owned(),
            ]
        );
        assert_eq!(
            edges,
            [
                format!("Story:ST-001 IN_EPIC Epic:EP-1 {identity}"),
                format!("Story:ST-002 DEPENDS_ON Story:ST-001 {identity}"),
                format!("Story:ST-002 DEPENDS_ON Story:ST-099 {identity}"),
                format!("Story:ST-002 IN_EPIC Epic:EP-1 {identity}"),
            ]
        );
    }
    let total = neo4j
        .cypher("MATCH (n:Story {id: 'ST-001'}) RETURN count(n)", json!({}))
        .unwrap();
    assert_eq!(total, [json!([3])]);
    server.finish();
}

#[test]
fn resync_after_changed_and_removed_links_matches_the_backlog() {
    let Some(neo4j) = Neo4j::start() else { return };
    let fixture = Fixture::new(&[GITHUB]);
    fixture.write(0, "ST-001", &story("ST-001", "One", "done", "epic: EP-1\n"));
    fixture.write(
        0,
        "ST-002",
        &story(
            "ST-002",
            "Two",
            "ready",
            "depends_on: [ST-001]\nepic: EP-1\n",
        ),
    );
    fixture.write(
        0,
        "ST-003",
        &story("ST-003", "Three", "draft", "depends_on: [ST-002, ST-050]\n"),
    );
    let mut server = fixture.server(&neo4j, &[]);
    server.ok("story_graph_sync", json!({}));
    let first = neo4j.dump("github:example/one");
    assert_eq!(first.1.len(), 5, "{first:?}");

    fixture.write(
        0,
        "ST-002",
        &story("ST-002", "Two renamed", "in-progress", "epic: EP-2\n"),
    );
    fixture.write(
        0,
        "ST-003",
        &story("ST-003", "Three", "draft", "depends_on: [ST-001]\n"),
    );
    fixture.remove(0, "ST-001");
    fixture.write(
        0,
        "ST-004",
        &story("ST-004", "Four", "draft", "depends_on: [ST-003, ST-003]\n"),
    );
    for _ in 0..2 {
        let result = server.ok("story_graph_sync", json!({}));
        assert_eq!(result["stories"], 3, "{result}");
        let (nodes, edges) = neo4j.dump("github:example/one");
        assert_eq!(
            nodes,
            [
                r#"[["Epic"],"EP-2",null,null,null]"#,
                r#"[["Story"],"ST-001",null,null,true]"#,
                r#"[["Story"],"ST-002","Two renamed","in-progress",false]"#,
                r#"[["Story"],"ST-003","Three","draft",false]"#,
                r#"[["Story"],"ST-004","Four","draft",false]"#,
            ]
        );
        assert_eq!(
            edges,
            [
                "Story:ST-002 IN_EPIC Epic:EP-2 github:example/one",
                "Story:ST-003 DEPENDS_ON Story:ST-001 github:example/one",
                "Story:ST-004 DEPENDS_ON Story:ST-003 github:example/one",
            ]
        );
    }
    server.finish();
}

fn node<'a>(graph: &'a Value, key: &str) -> &'a Value {
    graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["key"] == key)
        .unwrap_or_else(|| panic!("no node {key} in {graph}"))
}

fn edges(graph: &Value) -> Vec<String> {
    let mut edges: Vec<String> = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            format!(
                "{} {} {}",
                e["from"].as_str().unwrap(),
                e["type"].as_str().unwrap(),
                e["to"].as_str().unwrap()
            )
        })
        .collect();
    edges.sort();
    edges
}

#[test]
fn story_graph_returns_labeled_nodes_and_typed_edges_within_bounds() {
    let Some(neo4j) = Neo4j::start() else { return };
    let fixture = Fixture::new(&[GITHUB]);
    fixture.write(
        0,
        "ST-001",
        &story(
            "ST-001",
            "One",
            "done",
            "remote_url: \"https://github.com/example/one/issues/1\"\n",
        ),
    );
    fixture.write(
        0,
        "ST-002",
        &story(
            "ST-002",
            "Two",
            "ready",
            "depends_on: [ST-001]\nepic: EP-1\n",
        ),
    );
    fixture.write(
        0,
        "ST-003",
        &story("ST-003", "Three", "draft", "depends_on: [ST-002, ST-404]\n"),
    );
    fixture.write(
        0,
        "ST-004",
        &story("ST-004", "Four", "draft", "depends_on: [ST-003]\n"),
    );
    let mut server = fixture.server(&neo4j, &[]);
    server.ok("story_graph_sync", json!({}));

    let graph = server.ok("story_graph", json!({"id": "ST-002"}));
    assert_eq!(graph["repository"], "repo0");
    assert_eq!(graph["repository_id"], "github:example/one");
    assert_eq!(graph["truncated"], false);
    assert_eq!(graph["nodes"][0]["key"], "Story:ST-002", "{graph}");
    assert_eq!(
        node(&graph, "Story:ST-001"),
        &json!({"key": "Story:ST-001", "label": "Story", "id": "ST-001", "title": "One", "status": "done", "type": "story", "remote_url": "https://github.com/example/one/issues/1", "missing": false})
    );
    assert_eq!(node(&graph, "Epic:EP-1")["label"], "Epic");
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 4, "{graph}");
    assert_eq!(
        edges(&graph),
        [
            "Story:ST-002 DEPENDS_ON Story:ST-001",
            "Story:ST-002 IN_EPIC Epic:EP-1",
            "Story:ST-003 DEPENDS_ON Story:ST-002",
        ]
    );

    let deep = server.ok("story_graph", json!({"id": "ST-001", "depth": 2}));
    assert_eq!(node(&deep, "Story:ST-003")["status"], "draft");
    assert!(
        deep["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["key"] != "Story:ST-004"),
        "{deep}"
    );
    let missing = server.ok("story_graph", json!({"id": "ST-003"}));
    assert_eq!(node(&missing, "Story:ST-404")["missing"], true);
    assert_eq!(node(&missing, "Story:ST-404")["title"], Value::Null);

    let whole = server.ok("story_graph", json!({"limit": 2}));
    assert_eq!(whole["truncated"], true);
    assert_eq!(whole["nodes"].as_array().unwrap().len(), 2);

    assert_eq!(
        server.error("story_graph", json!({"id": "ST-001", "depth": 4}))["code"],
        "invalid_argument"
    );
    assert_eq!(
        server.error("story_graph", json!({"limit": 201}))["code"],
        "invalid_argument"
    );
    assert_eq!(
        server.error("story_graph", json!({"id": "ST-777"}))["code"],
        "story_not_found"
    );
    server.finish();
}

#[test]
fn story_graph_of_an_empty_backlog_or_unlinked_story() {
    let Some(neo4j) = Neo4j::start() else { return };
    let fixture = Fixture::new(&[GITHUB, GITEA]);
    fixture.write(1, "ST-001", &story("ST-001", "Alone", "draft", ""));
    let mut server = fixture.server(&neo4j, &[]);
    for key in ["repo0", "repo1"] {
        server.ok("story_graph_sync", json!({"repository": key}));
    }
    let empty = server.ok("story_graph", json!({"repository": "repo0"}));
    assert_eq!(empty["nodes"], json!([]));
    assert_eq!(empty["edges"], json!([]));
    assert_eq!(empty["truncated"], false);
    let alone = server.ok(
        "story_graph",
        json!({"repository": "repo1", "id": "ST-001", "depth": 3}),
    );
    assert_eq!(alone["nodes"].as_array().unwrap().len(), 1, "{alone}");
    assert_eq!(alone["nodes"][0]["title"], "Alone");
    assert_eq!(alone["edges"], json!([]));
    server.finish();
}

#[test]
fn graph_failures_are_typed_and_never_expose_the_password() {
    let Some(neo4j) = Neo4j::start() else { return };
    let fixture = Fixture::new(&[GITHUB]);
    fixture.write(
        0,
        "ST-001",
        &story("ST-001", "One", "ready", "depends_on: [ST-002]\n"),
    );
    let before = fixture.snapshot();
    let mut server = fixture.server(&neo4j, &[]);
    for tool in ["story_graph_sync", "story_graph"] {
        let error = server.error(tool, json!({"repository": "elsewhere"}));
        assert_eq!(error["code"], "repository_not_allowed", "{error}");
    }
    let mut wrong = Server::start(
        &[
            "--root".into(),
            fixture.roots[0].to_str().unwrap().into(),
            "--graph".into(),
            "--neo4j-url".into(),
            neo4j.url.clone(),
            "--neo4j-user".into(),
            "neo4j".into(),
            "--neo4j-password-env".into(),
            PASSWORD_ENV.into(),
        ],
        &fixture.base,
        &[(PASSWORD_ENV, "not-the-fake-password")],
    );
    assert_eq!(
        wrong.error("story_graph_sync", json!({}))["code"],
        "graph_auth_failed"
    );
    let (wrong_out, wrong_err) = wrong.finish();
    assert!(
        !wrong_err.contains("not-the-fake-password")
            && !wrong_out.contains("not-the-fake-password")
    );

    neo4j.stop();
    let mut transcript = String::new();
    for tool in ["story_graph_sync", "story_graph"] {
        let result = server.call(tool, json!({}));
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(
            result["structuredContent"]["error"]["code"], "graph_unavailable",
            "{result}"
        );
        assert_eq!(
            result["structuredContent"]["error"]["retryable"], true,
            "{result}"
        );
        transcript.push_str(&result.to_string());
    }
    let (rest, stderr) = server.finish();
    transcript.push_str(&rest);
    transcript.push_str(&stderr);
    assert!(!transcript.contains(PASSWORD), "{transcript}");
    assert!(
        stderr.contains("\"error_code\":\"graph_unavailable\""),
        "{stderr}"
    );
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn graph_launch_refuses_an_unset_password_variable_without_echoing_values() {
    let fixture = Fixture::new(&[GITHUB]);
    let output = Command::new(env!("CARGO_BIN_EXE_story"))
        .args([
            "mcp",
            "--root",
            fixture.roots[0].to_str().unwrap(),
            "--graph",
            "--neo4j-url",
            "http://127.0.0.1:9",
            "--neo4j-user",
            "neo4j",
            "--neo4j-password-env",
            PASSWORD_ENV,
        ])
        .env_clear()
        .env("PATH", "/nonexistent")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(PASSWORD_ENV), "{stderr}");
}

/// Three registered repositories behind `story mcp --http --registry --graph` (ST-132). The
/// upstream repositories are local git repositories. The server has no Neo4j credentials.
struct Org {
    base: PathBuf,
    child: Child,
    url: String,
    stderr: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}
const ORG_TOKEN: &str = "org-graph-token-0123456789abcdef";

impl Org {
    fn git(base: &Path, dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(["-c", "user.name=Fixture", "-c", "commit.gpgsign=false"])
            .args(["-c", "user.email=fixture@example.invalid"])
            .args(args)
            .current_dir(dir)
            .env("HOME", base)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn write(&self, repo: &str, id: &str, text: &str) {
        let dir = self.base.join("up").join(repo).join("backlog");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{id}.md")), text).unwrap();
    }

    /// Commits everything in `repo` and returns the new head.
    fn commit(&self, repo: &str) -> String {
        let dir = self.base.join("up").join(repo);
        Self::git(&self.base, &dir, &["add", "-A"]);
        Self::git(&self.base, &dir, &["commit", "-q", "-m", "stories"]);
        Self::git(&self.base, &dir, &["rev-parse", "HEAD"])
    }

    fn start(neo4j: &Neo4j, stories: &[(&str, &str, &str)]) -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-mcp-graph-org-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let mut registry = String::from("org = \"fixture\"\nfetch_ttl_seconds = 1\n");
        for (repo, prefix) in [("core", "VC"), ("other", "OT"), ("web", "VE")] {
            let dir = base.join("up").join(repo);
            fs::create_dir_all(dir.join(".story-time")).unwrap();
            Self::git(&base, &dir, &["init", "-q", "-b", "main"]);
            fs::write(
                dir.join(".story-time/settings.json"),
                format!(r#"{{"tracker": "files", "prefix": "{prefix}"}}"#),
            )
            .unwrap();
            registry.push_str(&format!(
                "[[repo]]\nkey = \"{repo}\"\nclone = \"file://{}\"\nforge = \"github\"\nprefixes = [\"{prefix}\"]\n",
                dir.display()
            ));
        }
        fs::write(base.join("registry.toml"), registry).unwrap();
        fs::write(
            base.join("clients.toml"),
            format!(
                "[[client]]\nid = \"reader\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\", \"write\"]\n",
                sha256(ORG_TOKEN.as_bytes())
            ),
        )
        .unwrap();
        let mut org = Self {
            child: Command::new("true").spawn().unwrap(),
            url: String::new(),
            stderr: Default::default(),
            base,
        };
        for (repo, id, text) in stories {
            org.write(repo, id, text);
        }
        for repo in ["core", "other", "web"] {
            org.commit(repo);
        }
        let path = |name: &str| org.base.join(name).to_str().unwrap().to_owned();
        let mut child = Command::new(env!("CARGO_BIN_EXE_story"))
            .args([
                "mcp",
                "--http",
                "127.0.0.1:0",
                "--registry",
                &path("registry.toml"),
            ])
            .args(["--data", &path("data"), "--clients", &path("clients.toml")])
            .args(["--graph", "--neo4j-url", &neo4j.url])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &org.base)
            .current_dir(&org.base)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let lines = BufReader::new(child.stderr.take().unwrap());
        let (found, address) = std::sync::mpsc::channel();
        let sink = org.stderr.clone();
        std::thread::spawn(move || {
            for line in lines.lines().map_while(Result::ok) {
                if let Some(address) = line.strip_prefix("listening on ") {
                    let _ = found.send(address.to_owned());
                }
                sink.lock().unwrap().push(line);
            }
        });
        let _ = org.child.wait();
        org.child = child;
        let address = address
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("no listening line: {:?}", org.stderr.lock().unwrap()));
        org.url = format!("http://{address}");
        org
    }

    fn rpc(&self, method: &str, mut params: Value) -> Value {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        let reply: Value = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap()
            .post(format!("{}/mcp", self.url))
            .bearer_auth(ORG_TOKEN)
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    fn call(&self, name: &str, arguments: Value) -> Value {
        self.rpc("tools/call", json!({"name": name, "arguments": arguments}))["structuredContent"]
            .clone()
    }

    fn ok(&self, name: &str, arguments: Value) -> Value {
        let content = self.call(name, arguments);
        assert!(content.get("error").is_none(), "{name}: {content}");
        content
    }
}
impl Drop for Org {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.base);
    }
}

/// The `repository/id` of each Story node of a graph, sorted.
fn org_stories(graph: &Value) -> Vec<String> {
    let mut found: Vec<String> = graph["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("{graph}"))
        .iter()
        .filter(|n| n["label"] == "Story")
        .map(|n| {
            format!(
                "{}/{}",
                n["repository"].as_str().unwrap_or("?"),
                n["id"].as_str().unwrap()
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn a_registry_server_answers_directed_queries_across_repositories_without_credentials() {
    let Some(neo4j) = Neo4j::open() else { return };
    let org = Org::start(
        &neo4j,
        &[
            ("core", "VC-108", &story("VC-108", "Schema", "done", "")),
            (
                "core",
                "VC-109",
                &story(
                    "VC-109",
                    "Roster API",
                    "ready",
                    "depends_on: [VC-108]\nrelates_to: [XX-001]\n",
                ),
            ),
            (
                "core",
                "VC-110",
                &story("VC-110", "Roster cache", "draft", ""),
            ),
            (
                "web",
                "VE-046",
                &story(
                    "VE-046",
                    "Roster page",
                    "ready",
                    "depends_on: [VC-109]\nrelates_to: [VC-110]\n",
                ),
            ),
            (
                "web",
                "VE-047",
                &story(
                    "VE-047",
                    "Roster filters",
                    "draft",
                    "depends_on: [VE-046]\n",
                ),
            ),
            // Two repositories hold XX-001 and no repository declares XX, so a link to it from
            // a third repository is ambiguous.
            (
                "web",
                "XX-001",
                &story("XX-001", "Stray in web", "draft", ""),
            ),
            (
                "other",
                "XX-001",
                &story("XX-001", "Stray in other", "draft", ""),
            ),
        ],
    );

    // Dependents of VC-109: the stories of `web` that wait on it, to depth 2. Its own
    // dependency VC-108 is not a dependent.
    let dependents = org.ok(
        "story_graph",
        json!({"repository": "core", "id": "VC-109", "direction": "dependents", "depth": 2}),
    );
    assert_eq!(
        org_stories(&dependents),
        ["core/VC-109", "web/VE-046", "web/VE-047"],
        "{dependents}"
    );
    assert!(
        edges(&dependents).contains(&"Story:web/VE-046 DEPENDS_ON Story:core/VC-109".into()),
        "{:?}",
        edges(&dependents)
    );
    let one = org.ok(
        "story_graph",
        json!({"repository": "core", "id": "VC-109", "direction": "dependents"}),
    );
    assert_eq!(org_stories(&one), ["core/VC-109", "web/VE-046"], "depth 1");

    // Dependencies of VE-046: VC-109 and, at depth 2, VC-108. Its dependent VE-047 is not
    // one. VC-110 is a RELATES_TO neighbor, which is returned at depth 1 only.
    let dependencies = org.ok(
        "story_graph",
        json!({"repository": "web", "id": "VE-046", "direction": "dependencies", "depth": 2}),
    );
    assert_eq!(
        org_stories(&dependencies),
        ["core/VC-108", "core/VC-109", "core/VC-110", "web/VE-046"],
        "{dependencies}"
    );
    let found = edges(&dependencies);
    for edge in [
        "Story:web/VE-046 DEPENDS_ON Story:core/VC-109",
        "Story:core/VC-109 DEPENDS_ON Story:core/VC-108",
        "Story:web/VE-046 RELATES_TO Story:core/VC-110",
    ] {
        assert!(found.contains(&edge.to_owned()), "{edge}: {found:?}");
    }

    // The default direction follows links both ways.
    let both = org.ok("story_graph", json!({"repository": "web", "id": "VE-046"}));
    assert_eq!(
        org_stories(&both),
        ["core/VC-109", "core/VC-110", "web/VE-046", "web/VE-047"],
        "{both}"
    );

    // Every result names the commits the graph was synced from, and the links the sync
    // skipped because their target is ambiguous.
    let head = |repo: &str| {
        Org::git(
            &org.base,
            &org.base.join("up").join(repo),
            &["rev-parse", "HEAD"],
        )
    };
    let synced = json!([
        {"repository": "core", "sha": head("core")},
        {"repository": "other", "sha": head("other")},
        {"repository": "web", "sha": head("web")},
    ]);
    for graph in [&dependents, &dependencies, &both] {
        assert_eq!(graph["sync"]["repositories"], synced, "{graph}");
        assert_eq!(graph["sync"]["skipped_ambiguous"], 1, "{graph}");
    }
    let stored = neo4j
        .cypher(
            "MATCH (a:Story {id: 'VC-109'})-[:RELATES_TO]->(b) RETURN count(b)",
            json!({}),
        )
        .unwrap();
    assert_eq!(stored[0][0], 0, "the ambiguous link has no edge");
    let repositories = neo4j
        .cypher(
            "MATCH (r:StoryRepository) RETURN r.key, r.sha ORDER BY r.key",
            json!({}),
        )
        .unwrap();
    assert_eq!(
        json!(repositories),
        json!([
            ["core", head("core")],
            ["other", head("other")],
            ["web", head("web")]
        ])
    );

    // A new commit reaches the graph after the fetch TTL, with no sync call.
    org.write(
        "web",
        "VE-048",
        &story("VE-048", "Roster export", "draft", "depends_on: [VC-109]\n"),
    );
    let next = org.commit("web");
    std::thread::sleep(Duration::from_millis(1300));
    let after = org.ok(
        "story_graph",
        json!({"repository": "core", "id": "VC-109", "direction": "dependents"}),
    );
    assert_eq!(
        org_stories(&after),
        ["core/VC-109", "web/VE-046", "web/VE-048"],
        "{after}"
    );
    assert_eq!(after["sync"]["repositories"][2]["sha"], next.as_str());

    // The tool list: a direction, the RELATES_TO edge type, and no sync tool, because the
    // server syncs by itself.
    let tools = org.rpc("tools/list", json!({}))["tools"].clone();
    let tool = |name: &str| tools.as_array().unwrap().iter().find(|t| t["name"] == name);
    assert!(tool("story_graph_sync").is_none(), "{tools}");
    let graph = tool("story_graph").unwrap();
    assert_eq!(
        graph["inputSchema"]["properties"]["direction"]["enum"],
        json!(["both", "dependents", "dependencies"])
    );
    let output = &graph["outputSchema"]["properties"];
    assert!(output["edges"]["items"]["properties"]["type"]["enum"]
        .as_array()
        .unwrap()
        .contains(&json!("RELATES_TO")));
    assert!(
        output["sync"].is_object()
            && output["nodes"]["items"]["properties"]["repository"].is_object()
    );

    // With Neo4j stopped, the graph is unavailable and lint, which does not use it, works.
    neo4j.stop();
    let down = org.call("story_graph", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(down["error"]["code"], "graph_unavailable", "{down}");
    assert_eq!(down["error"]["retryable"], true);
    let lint = org.ok(
        "story_lint",
        json!({"repository": "web", "ids": ["VE-046"]}),
    );
    assert_eq!(lint["repository"], "web");
    assert!(
        !lint.to_string().contains("which does not exist"),
        "the link to VC-109 resolves without Neo4j: {lint}"
    );
}

#[test]
fn a_local_sync_projects_relates_to_and_a_read_takes_a_direction() {
    let Some(neo4j) = Neo4j::start() else { return };
    let fixture = Fixture::new(&[GITHUB]);
    fixture.write(0, "ST-001", &story("ST-001", "One", "ready", ""));
    fixture.write(
        0,
        "ST-002",
        &story(
            "ST-002",
            "Two",
            "ready",
            "depends_on: [ST-001]\nrelates_to: [ST-004]\n",
        ),
    );
    fixture.write(
        0,
        "ST-003",
        &story("ST-003", "Three", "draft", "depends_on: [ST-002]\n"),
    );
    fixture.write(0, "ST-004", &story("ST-004", "Four", "draft", ""));
    let mut server = fixture.server(&neo4j, &[]);
    let synced = server.ok("story_graph_sync", json!({}));
    assert_eq!(synced["depends_on"], 2, "{synced}");
    assert_eq!(synced["relates_to"], 1, "{synced}");
    assert_eq!(synced["skipped_ambiguous"], 0, "{synced}");

    let ids = |graph: &Value| -> Vec<String> {
        let mut ids: Vec<String> = graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        ids
    };
    let dependents = server.ok(
        "story_graph",
        json!({"id": "ST-002", "direction": "dependents"}),
    );
    assert_eq!(
        ids(&dependents),
        ["ST-002", "ST-003", "ST-004"],
        "{dependents}"
    );
    let dependencies = server.ok(
        "story_graph",
        json!({"id": "ST-002", "direction": "dependencies"}),
    );
    assert_eq!(ids(&dependencies), ["ST-001", "ST-002", "ST-004"]);
    assert!(
        edges(&dependencies).contains(&"Story:ST-002 RELATES_TO Story:ST-004".into()),
        "{:?}",
        edges(&dependencies)
    );
    assert!(
        dependencies.get("sync").is_none(),
        "a local read has no sync"
    );
    let refused = server.error("story_graph", json!({"id": "ST-002", "direction": "up"}));
    assert_eq!(refused["code"], "invalid_argument");
    server.finish();
}

#[test]
fn graph_launch_takes_the_user_and_the_password_variable_together_or_neither() {
    let fixture = Fixture::new(&[GITHUB]);
    let root = fixture.roots[0].to_str().unwrap();
    let run = |extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_story"))
            .args([
                "mcp",
                "--root",
                root,
                "--graph",
                "--neo4j-url",
                "http://127.0.0.1:9",
            ])
            .args(extra)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env(PASSWORD_ENV, PASSWORD)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    let (code, stderr) = run(&["--neo4j-user", "neo4j"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--neo4j-password-env"), "{stderr}");
    let (code, stderr) = run(&["--neo4j-password-env", PASSWORD_ENV]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("--neo4j-user") && !stderr.contains(PASSWORD),
        "{stderr}"
    );
    // With neither, the server starts; with no input it ends at once and exits 0.
    let (code, stderr) = run(&[]);
    assert_eq!(code, Some(0), "{stderr}");
}
