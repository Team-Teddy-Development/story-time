//! `story mcp --http` (ST-129, ADR 0007): the stdio tools over HTTP, with one bearer token and one
//! scope set per client. Each test starts the real binary on a free port.

use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODERN: &str = "2026-07-28";
const LEGACY: &str = "2025-11-25";
const READER: &str = "reader-token-0123456789abcdef";
const WRITER: &str = "writer-token-0123456789abcdef";
const LINKER: &str = "linker-token-0123456789abcdef";
const ORIGIN: &str = "https://allowed.example";

const SETTINGS: &str = r#"{
  "tracker": "github",
  "sync": "off",
  "backlogDir": "backlog",
  "prefix": "ST",
  "team": { "wipLimit": 2 },
  "definitionOfReady": ["Criteria are testable"],
  "definitionOfDone": ["Tests pass"],
  "github": { "repo": "example/fixture" }
}
"#;

fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn story(n: usize) -> String {
    format!(
        "---\nid: ST-{n:03}\ntitle: Story number {n}\nstatus: draft\ntype: story\nsize: 2\ndepends_on: []\ncreated: 2026-11-14\nupdated: 2026-11-14\n---\n\n## Story\nAs a planner, I want story {n}, so that the fixture has data.\n\n## Acceptance Criteria\n- [ ] Given story {n}, when it is read, then it is shown\n"
    )
}

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    clients: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-mcp-http-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("fixture");
        fs::create_dir_all(root.join("backlog")).unwrap();
        fs::create_dir_all(root.join(".story-time")).unwrap();
        fs::write(root.join(".story-time/settings.json"), SETTINGS).unwrap();
        for n in 1..=3 {
            fs::write(root.join(format!("backlog/ST-{n:03}.md")), story(n)).unwrap();
        }
        let clients = base.join("clients.toml");
        fs::write(
            &clients,
            format!(
                "[[client]]\nid = \"reader\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\"]\n\n[[client]]\nid = \"writer\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\", \"write\"]\n\n[[client]]\nid = \"linker\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\"]\nurl_token = true\n",
                sha256(READER.as_bytes()),
                sha256(WRITER.as_bytes()),
                sha256(LINKER.as_bytes())
            ),
        )
        .unwrap();
        let root = root.canonicalize().unwrap();
        Self {
            base,
            root,
            clients,
        }
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
        walk(&self.root, &mut files);
        files
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_story"));
        command
            .arg("mcp")
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.base)
            .current_dir(&self.root);
        command
    }
    fn http(&self) -> Http {
        let mut child = self
            .command()
            .args(["--http", "127.0.0.1:0", "--root"])
            .arg(&self.root)
            .arg("--clients")
            .arg(&self.clients)
            .args(["--allowed-origin", ORIGIN])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = BufReader::new(child.stderr.take().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let (found, address) = mpsc::channel();
        let sink = log.clone();
        std::thread::spawn(move || {
            for line in stderr.lines().map_while(Result::ok) {
                if let Some(address) = line.strip_prefix("listening on ") {
                    let _ = found.send(address.to_owned());
                }
                sink.lock().unwrap().push(line);
            }
        });
        let address = address
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("no listening line; stderr {:?}", log.lock().unwrap()));
        Http {
            child,
            url: format!("http://{address}"),
            client: Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap(),
            log,
        }
    }
    /// One modern `tools/call` through the stdio server on the same root.
    fn stdio_call(&self, name: &str, arguments: Value) -> Value {
        let mut child = self
            .command()
            .arg("--root")
            .arg(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let message = request(
            "tools/call",
            modern(json!({"name": name, "arguments": arguments})),
        );
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "{message}").unwrap();
        drop(stdin);
        let output = child.wait_with_output().unwrap();
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

struct Http {
    child: Child,
    url: String,
    client: Client,
    log: Arc<Mutex<Vec<String>>>,
}
impl Http {
    fn post(&self, token: Option<&str>) -> RequestBuilder {
        let builder = self
            .client
            .post(format!("{}/mcp", self.url))
            .header("Content-Type", "application/json");
        match token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }
    /// Sends one JSON-RPC request and returns the JSON-RPC response of an HTTP 200.
    fn rpc(&self, token: &str, message: &Value) -> Value {
        json_of(self.post(Some(token)).body(message.to_string()))
    }
    fn tools(&self, token: &str) -> Vec<String> {
        let listed = self.rpc(token, &request("tools/list", modern(json!({}))));
        listed["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{listed}"))
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }
    fn call(&self, token: &str, name: &str, arguments: Value) -> Value {
        let message = request(
            "tools/call",
            modern(json!({"name": name, "arguments": arguments})),
        );
        let response = self.rpc(token, &message);
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }
    fn audit_lines(&self) -> Vec<Value> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
    /// The audit lines once `done` holds for them. The server writes each line to stderr before
    /// it answers, but the reader thread can take it after the answer arrives (ST-159).
    fn audit_when(&self, done: impl Fn(&[Value]) -> bool) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let lines = self.audit_lines();
            if done(&lines) || Instant::now() > deadline {
                return lines;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Http {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn send(builder: RequestBuilder) -> Response {
    builder.send().unwrap()
}

fn json_of(builder: RequestBuilder) -> Value {
    let response = send(builder);
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"].to_str().unwrap(),
        "application/json"
    );
    response.json().unwrap()
}

fn request(method: &str, params: Value) -> Value {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn modern(mut params: Value) -> Value {
    params.as_object_mut().unwrap().insert(
        "_meta".into(),
        json!({
            "io.modelcontextprotocol/protocolVersion": MODERN,
            "io.modelcontextprotocol/clientInfo": {"name": "http-test", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }),
    );
    params
}

#[test]
fn a_request_without_a_known_token_gets_401_on_every_method() {
    let fixture = Fixture::new();
    let server = fixture.http();
    let list = request("tools/list", modern(json!({})));
    let show = request(
        "tools/call",
        modern(json!({"name": "story_show", "arguments": {"id": "ST-001"}})),
    );
    for message in [&list, &show] {
        for token in [None, Some("unknown-token-0123456789abcdef"), Some("")] {
            let response = send(server.post(token).body(message.to_string()));
            assert_eq!(response.status(), 401, "{token:?} {message}");
            assert_eq!(
                response.headers()["www-authenticate"].to_str().unwrap(),
                "Bearer"
            );
            assert!(!response.text().unwrap().contains("ST-001"));
        }
    }
    let basic = send(
        server
            .post(None)
            .header("Authorization", format!("Basic {READER}"))
            .body(list.to_string()),
    );
    assert_eq!(basic.status(), 401);
    assert!(server.audit_lines().is_empty(), "no tool ran");
}

/// ST-146: a client that cannot send a header gives its token in the path.
#[test]
fn a_marked_client_gives_its_token_in_the_path_and_no_other_client_can() {
    let fixture = Fixture::new();
    let server = fixture.http();
    let post = |path: &str| {
        server
            .client
            .post(format!("{}{path}", server.url))
            .header("Content-Type", "application/json")
    };
    let list = request("tools/list", modern(json!({})));

    // The marked client: no header, the token in the path. Its scope is read.
    let listed = json_of(post(&format!("/mcp/{LINKER}")).body(list.to_string()));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("{listed}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"story_show"), "{names:?}");
    assert!(!names.contains(&"story_update"), "{names:?}");
    let show = request(
        "tools/call",
        modern(json!({"name": "story_show", "arguments": {"id": "ST-001"}})),
    );
    let shown = json_of(post(&format!("/mcp/{LINKER}")).body(show.to_string()));
    assert!(shown.get("error").is_none(), "{shown}");
    // The path token decides. A header of another client does not change the caller.
    let both = json_of(
        post(&format!("/mcp/{LINKER}"))
            .bearer_auth(WRITER)
            .body(list.to_string()),
    );
    assert!(!both.to_string().contains("story_update"), "{both}");
    // The marked client can also use the header.
    assert!(server.tools(LINKER).contains(&"story_show".to_owned()));
    let ran = server.audit_lines().len();
    assert_eq!(ran, 1, "one tool call ran: story_show");

    // An unknown token gives the answer that every refusal below must equal.
    let unknown = send(post("/mcp/unknown-token-0123456789abcdef").body(show.to_string()));
    let (status, challenge) = (
        unknown.status(),
        unknown.headers()["www-authenticate"].clone(),
    );
    let body = unknown.text().unwrap();
    assert_eq!(status, 401);
    // A client that is not marked: its token is good in a header and refused in a path.
    for token in [READER, WRITER] {
        let refused = send(post(&format!("/mcp/{token}")).body(show.to_string()));
        assert_eq!(refused.status(), status, "{token}");
        assert_eq!(refused.headers()["www-authenticate"], challenge);
        assert_eq!(refused.text().unwrap(), body);
    }
    // A hash is not a token, and an empty token is no token.
    let hash = sha256(LINKER.as_bytes());
    for path in [format!("/mcp/{hash}"), "/mcp/".to_owned()] {
        let refused = send(post(&path).body(show.to_string()));
        assert_eq!(refused.status(), 401, "{path}");
    }
    // One more segment is not the MCP endpoint, also with a good token.
    for path in [format!("/mcp/{LINKER}/x"), format!("/mcp/{LINKER}/")] {
        let missing = send(post(&path).body(show.to_string()));
        assert_eq!(missing.status(), 404, "{path}");
    }
    // The path endpoint keeps the rules of `/mcp`.
    let get = send(server.client.get(format!("{}/mcp/{LINKER}", server.url)));
    assert_eq!(get.status(), 405);

    assert_eq!(
        server.audit_lines().len(),
        ran,
        "no refused request ran a tool"
    );
    let log = server.log.lock().unwrap().join("\n");
    for token in [LINKER, READER, WRITER, "unknown-token-0123456789abcdef"] {
        assert!(!log.contains(token), "the server wrote a token");
    }
    assert!(
        log.contains("\"linker\""),
        "the audit line names the client: {log}"
    );
}

#[test]
fn story_show_over_http_equals_story_show_over_stdio() {
    let fixture = Fixture::new();
    let server = fixture.http();
    let over_http = server.call(READER, "story_show", json!({"id": "ST-002"}));
    let over_stdio = fixture.stdio_call("story_show", json!({"id": "ST-002"}));
    assert_eq!(over_http["isError"], false, "{over_http}");
    assert_eq!(
        over_http["structuredContent"],
        over_stdio["result"]["structuredContent"]
    );
    assert_eq!(over_http, over_stdio["result"]);

    let audit = server.audit_when(|lines| !lines.is_empty());
    assert_eq!(audit.len(), 1, "{audit:?}");
    assert_eq!(audit[0]["tool"], "story_show");
    assert_eq!(audit[0]["client_id"], "reader");
    assert!(!server.log.lock().unwrap().join("\n").contains(READER));
}

#[test]
fn a_read_only_token_cannot_see_or_run_write_tools() {
    let fixture = Fixture::new();
    let server = fixture.http();
    let shown = server.call(READER, "story_show", json!({"id": "ST-002"}));
    let sha = shown["structuredContent"]["sha256"]
        .as_str()
        .unwrap_or_else(|| panic!("{shown}"))
        .to_owned();
    let update = json!({"id": "ST-002", "expected_sha256": sha, "set": {"size": 3}});
    let before = fixture.snapshot();

    let reader_tools = server.tools(READER);
    assert!(reader_tools.contains(&"story_show".to_owned()));
    for hidden in ["story_update", "story_note", "story_create"] {
        assert!(!reader_tools.contains(&hidden.to_owned()), "{hidden}");
    }
    let refused = server.call(READER, "story_update", update.clone());
    assert_eq!(refused["isError"], true, "{refused}");
    assert_eq!(
        refused["structuredContent"]["error"]["code"],
        "scope_refused"
    );
    assert_eq!(refused["structuredContent"]["error"]["retryable"], false);
    assert_eq!(fixture.snapshot(), before, "a refused call changes no file");
    let audit =
        server.audit_when(|lines| lines.last().is_some_and(|l| l["tool"] == "story_update"));
    let last = audit.last().unwrap();
    assert_eq!(last["tool"], "story_update");
    assert_eq!(last["error_code"], "scope_refused");
    assert_eq!(last["client_id"], "reader");

    let writer_tools = server.tools(WRITER);
    for visible in ["story_show", "story_update", "story_note", "story_create"] {
        assert!(writer_tools.contains(&visible.to_owned()), "{visible}");
    }
    let written = server.call(WRITER, "story_update", update);
    assert_eq!(written["isError"], false, "{written}");
    assert_ne!(fixture.snapshot(), before);
}

#[test]
fn each_request_uses_the_protocol_era_of_its_own_request() {
    let fixture = Fixture::new();
    let server = fixture.http();
    let modern_list = || server.rpc(WRITER, &request("tools/list", modern(json!({}))));
    let legacy_list = || {
        json_of(
            server
                .post(Some(READER))
                .header("MCP-Protocol-Version", LEGACY)
                .body(request("tools/list", json!({})).to_string()),
        )
    };

    let first = modern_list();
    assert_eq!(first["result"]["resultType"], "complete", "{first}");

    let initialize = server.rpc(
        READER,
        &request(
            "initialize",
            json!({"protocolVersion": LEGACY, "capabilities": {}, "clientInfo": {"name": "legacy", "version": "1"}}),
        ),
    );
    assert_eq!(initialize["result"]["protocolVersion"], LEGACY);
    let initialized = send(
        server
            .post(Some(READER))
            .header("MCP-Protocol-Version", LEGACY)
            .body(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string()),
    );
    assert_eq!(initialized.status(), 202);

    for _ in 0..2 {
        let legacy = legacy_list();
        assert!(legacy["result"]["tools"].is_array(), "{legacy}");
        assert!(legacy["result"].get("resultType").is_none(), "{legacy}");
        assert!(legacy["result"].get("_meta").is_none(), "{legacy}");
        let modern = modern_list();
        assert_eq!(modern["result"]["resultType"], "complete", "{modern}");
        assert!(modern["result"]["_meta"].is_object(), "{modern}");
    }

    // The legacy client's `initialize` set no state for the process: a request that names no
    // era in its body or its header is refused.
    let unnamed = server.rpc(READER, &request("tools/list", json!({})));
    assert_eq!(unnamed["error"]["code"], -32022, "{unnamed}");
    assert_eq!(
        unnamed["error"]["data"]["supported"],
        json!([MODERN, LEGACY])
    );
    let unknown = json_of(
        server
            .post(Some(READER))
            .header("MCP-Protocol-Version", "1999-01-01")
            .body(request("tools/list", json!({})).to_string()),
    );
    assert_eq!(unknown["error"]["code"], -32022, "{unknown}");
}

#[test]
fn a_header_that_does_not_match_the_body_gets_minus_32020() {
    let fixture = Fixture::new();
    let server = fixture.http();
    let show = request(
        "tools/call",
        modern(json!({"name": "story_show", "arguments": {"id": "ST-001"}})),
    );
    let with = |headers: &[(&str, &str)]| {
        let mut builder = server.post(Some(READER));
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        json_of(builder.body(show.to_string()))
    };

    let matching = with(&[
        ("MCP-Protocol-Version", MODERN),
        ("Mcp-Method", "tools/call"),
        ("Mcp-Name", "story_show"),
    ]);
    assert_eq!(matching["result"]["isError"], false, "{matching}");
    let ran = server.audit_lines().len();

    for headers in [
        [("Mcp-Method", "tools/list")],
        [("Mcp-Name", "story_list")],
        [("MCP-Protocol-Version", LEGACY)],
    ] {
        let response = with(&headers);
        assert_eq!(response["error"]["code"], -32020, "{headers:?} {response}");
        assert_eq!(response["id"], show["id"]);
        assert!(response.get("result").is_none());
    }
    assert_eq!(
        server.audit_lines().len(),
        ran,
        "a refused request runs no tool"
    );
}

#[test]
fn origin_size_method_and_notification_get_their_status_codes() {
    let fixture = Fixture::new();
    let server = fixture.http();
    let list = request("tools/list", modern(json!({}))).to_string();

    let foreign = send(
        server
            .post(Some(READER))
            .header("Origin", "https://evil.example")
            .body(list.clone()),
    );
    assert_eq!(foreign.status(), 403);
    let allowed = send(
        server
            .post(Some(READER))
            .header("Origin", ORIGIN)
            .body(list.clone()),
    );
    assert_eq!(allowed.status(), 200);

    let padding = "x".repeat(1024 * 1024);
    let large = request("tools/list", modern(json!({"padding": padding}))).to_string();
    let too_large = send(server.post(Some(READER)).body(large));
    assert_eq!(too_large.status(), 413);

    let get = send(
        server
            .client
            .get(format!("{}/mcp", server.url))
            .bearer_auth(READER),
    );
    assert_eq!(get.status(), 405);
    assert_eq!(get.headers()["allow"].to_str().unwrap(), "POST");
    let delete = send(
        server
            .client
            .delete(format!("{}/mcp", server.url))
            .bearer_auth(READER),
    );
    assert_eq!(delete.status(), 405);

    let notification = send(
        server
            .post(Some(READER))
            .body(json!({"jsonrpc": "2.0", "method": "notifications/cancelled"}).to_string()),
    );
    assert_eq!(notification.status(), 202);
    assert!(notification.text().unwrap().is_empty());

    for path in ["healthz", "readyz"] {
        let probe = send(server.client.get(format!("{}/{path}", server.url)));
        assert_eq!(probe.status(), 200, "{path}");
    }
    let missing = send(server.client.get(format!("{}/other", server.url)));
    assert_eq!(missing.status(), 404);

    // The server still answers after each refusal.
    assert_eq!(send(server.post(Some(READER)).body(list)).status(), 200);
}

#[test]
fn http_mode_refuses_to_start_without_a_clients_file() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args(["--http", "127.0.0.1:0", "--root"])
        .arg(&fixture.root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--http needs --clients"), "{stderr}");

    fs::write(&fixture.clients, "[[client]]\nid = \"a\"\n").unwrap();
    let output = fixture
        .command()
        .args(["--http", "127.0.0.1:0", "--root"])
        .arg(&fixture.root)
        .arg("--clients")
        .arg(&fixture.clients)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("token_sha256"), "{stderr}");
}

/// ST-142: an org is a part of a registry. A server of local roots has no org to limit to.
#[test]
fn a_client_with_orgs_needs_a_registry() {
    let fixture = Fixture::new();
    fs::write(
        &fixture.clients,
        format!(
            "[[client]]\nid = \"scoped\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\"]\norgs = [\"varsity\"]\n",
            sha256(READER.as_bytes())
        ),
    )
    .unwrap();
    let output = fixture
        .command()
        .args(["--http", "127.0.0.1:0", "--root"])
        .arg(&fixture.root)
        .arg("--clients")
        .arg(&fixture.clients)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("client scoped") && stderr.contains("org varsity"),
        "{stderr}"
    );
}
