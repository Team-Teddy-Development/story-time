use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

type Findings = Vec<(String, Vec<(String, String, String)>)>;

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODERN: &str = "2026-07-28";
const LEGACY: &str = "2025-11-25";

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

fn story(n: usize, status: &str, kind: &str, epic: Option<&str>, body: &str) -> String {
    let epic = epic.map(|e| format!("epic: {e}\n")).unwrap_or_default();
    let deps = if n > 1 {
        format!("[ST-{:03}]", n - 1)
    } else {
        "[]".into()
    };
    format!(
        "---\nid: ST-{n:03}\ntitle: Story number {n}\nstatus: {status}\ntype: {kind}\nsize: {}\n{epic}depends_on: {deps}\ncreated: 2026-11-14\nupdated: 2026-11-14\n---\n\n## Story\nAs a planner, I want story {n}, so that the fixture has data.\n\n## Acceptance Criteria\n- [ ] Given story {n}, when it is read, then it is shown\n{body}",
        n % 5 + 1
    )
}

struct Fixture {
    base: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-mcp-{}-{}",
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
    fn with_stories(self, count: usize) -> Self {
        for n in 1..=count {
            let status = ["draft", "ready", "in-progress", "done"][n % 4];
            let kind = if n % 7 == 0 { "spike" } else { "story" };
            let epic = (n % 3 == 0).then_some("EP-1");
            self.write(
                &format!("backlog/ST-{n:03}.md"),
                &story(n, status, kind, epic, ""),
            );
        }
        self
    }
    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn snapshot(&self) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
        fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
            use std::os::unix::fs::PermissionsExt;
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let meta = fs::symlink_metadata(&path).unwrap();
                let mode = meta.permissions().mode();
                if meta.file_type().is_symlink() {
                    let target = fs::read_link(&path).unwrap();
                    into.insert(path, (mode, target.into_os_string().into_encoded_bytes()));
                } else if meta.is_dir() {
                    into.insert(path.clone(), (mode, Vec::new()));
                    walk(&path, into);
                } else {
                    into.insert(path.clone(), (mode, fs::read(&path).unwrap()));
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&self.base, &mut files);
        files
    }
    fn cli(&self, args: &[&str]) -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_story"))
            .args(args)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.base)
            .current_dir(&self.root)
            .output()
            .unwrap();
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "story {args:?}: {e}; stderr {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }
    fn server(&self) -> Server {
        Server::start(&["--root", self.root.to_str().unwrap()], &self.root, &[])
    }
    fn writer(&self) -> Server {
        Server::start(
            &["--root", self.root.to_str().unwrap(), "--write"],
            &self.root,
            &[],
        )
    }
    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root.join(relative)).unwrap()
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
    fn start(args: &[&str], cwd: &Path, env: &[(&str, &str)]) -> Self {
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
        Self {
            child,
            stdin,
            stdout,
        }
    }
    fn raw(&mut self, bytes: &[u8]) -> Option<Value> {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(bytes).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        if self.stdout.read_line(&mut line).unwrap() == 0 {
            return None;
        }
        Some(serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}")))
    }
    fn send(&mut self, message: &Value) -> Value {
        let response = self
            .raw(message.to_string().as_bytes())
            .expect("server closed stdout");
        assert_eq!(response["jsonrpc"], "2.0", "{response}");
        assert_eq!(response["id"], message["id"], "{response}");
        response
    }
    fn notify(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }
    fn modern(&mut self, method: &str, params: Value) -> Value {
        self.send(&request(method, with_meta(params, MODERN)))
    }
    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.modern("tools/call", json!({"name": name, "arguments": arguments}));
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }
    fn finish(mut self) -> (Option<i32>, String, String) {
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
        let status = self.child.wait().unwrap();
        (status.code(), rest, stderr)
    }
}

fn request(method: &str, params: Value) -> Value {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn with_meta(mut params: Value, version: &str) -> Value {
    params.as_object_mut().unwrap().insert(
        "_meta".into(),
        json!({
            "io.modelcontextprotocol/protocolVersion": version,
            "io.modelcontextprotocol/clientInfo": {"name": "protocol-test", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }),
    );
    params
}

/// The structured result of a successful tool call, after checking that the text block holds
/// the same JSON.
fn structured(result: &Value) -> &Value {
    assert_eq!(result["isError"], false, "{result}");
    let text = result["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap(),
        result["structuredContent"]
    );
    &result["structuredContent"]
}

fn tool_error(result: &Value) -> &Value {
    assert_eq!(result["isError"], true, "{result}");
    &result["structuredContent"]["error"]
}

/// JSON Schema 2020-12, limited to the keywords the server's output schemas use.
fn conforms(schema: &Value, value: &Value, at: &str) -> Result<(), String> {
    if let Some(types) = schema.get("type") {
        let types: Vec<&str> = match types {
            Value::String(t) => vec![t.as_str()],
            Value::Array(t) => t.iter().map(|t| t.as_str().unwrap()).collect(),
            _ => panic!("bad type keyword at {at}"),
        };
        let actual = match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        let ok = types.contains(&actual) || (actual == "integer" && types.contains(&"number"));
        if !ok {
            return Err(format!("{at}: {actual} is not {types:?}"));
        }
    }
    if let Some(choices) = schema.get("enum").and_then(Value::as_array) {
        if !choices.contains(value) {
            return Err(format!("{at}: {value} not in enum"));
        }
    }
    if let Value::Object(object) = value {
        for name in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !object.contains_key(name.as_str().unwrap()) {
                return Err(format!("{at}: missing {name}"));
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, item) in object {
            match properties.and_then(|p| p.get(key)) {
                Some(sub) => conforms(sub, item, &format!("{at}/{key}"))?,
                None => match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => return Err(format!("{at}: extra {key}")),
                    Some(sub @ Value::Object(_)) => conforms(sub, item, &format!("{at}/{key}"))?,
                    _ => {}
                },
            }
        }
    }
    if let (Value::Array(items), Some(sub)) = (value, schema.get("items")) {
        for (i, item) in items.iter().enumerate() {
            conforms(sub, item, &format!("{at}/{i}"))?;
        }
    }
    Ok(())
}

fn output_schemas(server: &mut Server) -> Map<String, Value> {
    let listed = server.modern("tools/list", json!({}));
    listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_owned(),
                t["outputSchema"].clone(),
            )
        })
        .collect()
}

fn ids(stories: &Value) -> Vec<String> {
    stories
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn read_tools_match_their_output_schemas_and_agree_with_the_cli() {
    let fixture = Fixture::new().with_stories(12);
    let before = fixture.snapshot();
    let mut server = fixture.server();
    let schemas = output_schemas(&mut server);
    let names: Vec<_> = schemas.keys().cloned().collect();
    assert_eq!(
        names,
        ["story_config", "story_list", "story_show", "story_lint"]
    );

    let check = |name: &str, value: &Value| {
        conforms(&schemas[name], value, name).unwrap_or_else(|e| panic!("{e}\n{value:#}"));
    };

    let config = server.tool("story_config", json!({}));
    let config = structured(&config).clone();
    check("story_config", &config);
    let cli_config = fixture.cli(&["config", "--json"]);
    let entry = &config["repositories"][0];
    assert_eq!(entry["key"], "fixture");
    assert_eq!(entry["status"], "ok");
    assert_eq!(entry["prefix"], cli_config["prefix"]);
    assert_eq!(entry["tracker"], cli_config["tracker"]);
    assert_eq!(entry["sync"], cli_config["sync"]);
    assert_eq!(entry["backlog_dir"], cli_config["backlogDir"]);
    assert_eq!(entry["wip_limit"], cli_config["team"]["wipLimit"]);
    assert_eq!(
        entry["definition_of_ready"],
        cli_config["definitionOfReady"]
    );
    assert_eq!(entry["definition_of_done"], cli_config["definitionOfDone"]);
    assert_eq!(entry["tracker_target"], json!({"repo": "example/fixture"}));

    let list = server.tool("story_list", json!({"limit": 100}));
    let list = structured(&list).clone();
    check("story_list", &list);
    let cli_list = fixture.cli(&["list", "--json"]);
    assert_eq!(ids(&list["stories"]), ids(&cli_list));
    assert_eq!(list["total"], 12);
    assert_eq!(list["next_cursor"], Value::Null);
    for (summary, full) in list["stories"]
        .as_array()
        .unwrap()
        .iter()
        .zip(cli_list.as_array().unwrap())
    {
        for field in ["title", "status", "type", "size", "updated"] {
            assert_eq!(summary[field], full[field], "{field}");
        }
        assert_eq!(
            summary["epic"],
            full.get("epic").cloned().unwrap_or(Value::Null)
        );
        assert_eq!(summary["depends_on"], full["depends_on"]);
        let bytes = fs::read(
            fixture
                .root
                .join(format!("backlog/{}.md", full["id"].as_str().unwrap())),
        )
        .unwrap();
        let digest: String = ring::digest::digest(&ring::digest::SHA256, &bytes)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(summary["sha256"], digest);
    }
    let filtered = server.tool(
        "story_list",
        json!({"status": ["ready", "done"], "type": "story", "epic": "EP-1"}),
    );
    let filtered = structured(&filtered).clone();
    check("story_list", &filtered);
    let expected: Vec<String> = cli_list
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| {
            ["ready", "done"].contains(&s["status"].as_str().unwrap())
                && s["type"] == "story"
                && s["epic"] == "EP-1"
        })
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(ids(&filtered["stories"]), expected);

    let show = server.tool("story_show", json!({"id": "ST-003"}));
    let show = structured(&show).clone();
    check("story_show", &show);
    let cli_show = fixture.cli(&["show", "ST-003", "--json"]);
    let mut rebuilt = show["frontmatter"].as_object().unwrap().clone();
    let sections: Map<String, Value> = show["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["heading"].as_str().unwrap().to_owned(), s["body"].clone()))
        .collect();
    rebuilt.insert("sections".into(), Value::Object(sections));
    assert_eq!(Value::Object(rebuilt), cli_show);
    let file = fs::read(fixture.root.join("backlog/ST-003.md")).unwrap();
    assert_eq!(show["path"], "backlog/ST-003.md");
    assert_eq!(show["bytes"], file.len());
    assert_eq!(show["repository"], "fixture");

    let lint = server.tool("story_lint", json!({}));
    let lint = structured(&lint).clone();
    check("story_lint", &lint);
    let cli_lint = fixture.cli(&["lint", "--json"]);
    let project = |results: &Value| -> Findings {
        results
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["id"].as_str().unwrap().to_owned(),
                    r["findings"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|f| {
                            (
                                f["rule"].as_str().unwrap().to_owned(),
                                f["severity"].as_str().unwrap().to_owned(),
                                f["message"].as_str().unwrap().to_owned(),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    };
    assert_eq!(project(&lint["results"]), project(&cli_lint));
    let all: Vec<&Value> = cli_lint
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["findings"].as_array().unwrap())
        .collect();
    let count = |severity: &str| all.iter().filter(|f| f["severity"] == severity).count();
    assert_eq!(lint["errors"], count("error"));
    assert_eq!(lint["warnings"], count("warning"));
    let one = server.tool("story_lint", json!({"ids": ["ST-002"]}));
    assert_eq!(ids(&structured(&one)["results"]), ["ST-002"]);

    let (code, rest, stderr) = server.finish();
    assert_eq!(code, Some(0));
    assert!(rest.is_empty());
    let audit: Vec<Value> = stderr
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(audit.len(), 6, "{stderr}");
    assert_eq!(audit[0]["tool"], "story_config");
    assert_eq!(audit[3]["ids"], json!(["ST-003"]));
    assert_eq!(audit[3]["outcome"], "ok");
    assert_eq!(before, fixture.snapshot());
}

#[test]
fn story_list_pages_every_id_once_and_refuses_a_cursor_with_another_filter() {
    let fixture = Fixture::new().with_stories(120);
    let mut server = fixture.server();
    let cli = ids(&fixture.cli(&["list", "--json"]));
    assert_eq!(cli.len(), 120);

    let mut seen = Vec::new();
    let mut cursor = Value::Null;
    let mut pages = 0;
    loop {
        let mut args = json!({"limit": 50});
        if !cursor.is_null() {
            args["cursor"] = cursor.clone();
        }
        let page = server.tool("story_list", args);
        let page = structured(&page).clone();
        assert_eq!(page["total"], 120);
        seen.extend(ids(&page["stories"]));
        pages += 1;
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(seen, cli);

    let first = server.tool("story_list", json!({"limit": 5, "status": ["ready"]}));
    let cursor = structured(&first)["next_cursor"].clone();
    assert!(cursor.is_string());
    let next = server.tool(
        "story_list",
        json!({"limit": 5, "status": ["ready"], "cursor": cursor}),
    );
    assert_eq!(structured(&next)["stories"].as_array().unwrap().len(), 5);
    let reused = server.tool(
        "story_list",
        json!({"limit": 5, "status": ["done"], "cursor": cursor}),
    );
    assert_eq!(tool_error(&reused)["code"], "invalid_cursor");
    let forged = server.tool("story_list", json!({"cursor": "not-a-cursor"}));
    assert_eq!(tool_error(&forged)["code"], "invalid_cursor");
    let too_big = server.tool("story_list", json!({"limit": 101}));
    let error = tool_error(&too_big);
    assert_eq!(error["code"], "invalid_argument");
    assert_eq!(error["details"]["pointer"], "/limit");
}

#[test]
fn modern_and_legacy_clients_both_work_and_version_faults_use_their_codes() {
    let fixture = Fixture::new().with_stories(3);

    let mut modern = fixture.server();
    let discover = modern.modern("server/discover", json!({}));
    assert_eq!(discover["result"]["resultType"], "complete");
    assert_eq!(
        discover["result"]["supportedVersions"],
        json!([MODERN, LEGACY])
    );
    assert_eq!(
        discover["result"]["capabilities"],
        json!({"tools": {"listChanged": false}})
    );
    assert_eq!(
        discover["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "story-time"
    );
    let tools = modern.modern("tools/list", json!({}));
    assert_eq!(tools["result"]["resultType"], "complete");
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 4);
    assert!(tools["result"].get("nextCursor").is_none());
    for tool in tools["result"]["tools"].as_array().unwrap() {
        assert_eq!(
            tool["annotations"],
            json!({"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false})
        );
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    }
    let called = modern.modern("tools/call", json!({"name": "story_list", "arguments": {}}));
    assert_eq!(called["result"]["resultType"], "complete");
    assert_eq!(ids(&structured(&called["result"])["stories"]).len(), 3);

    let unknown = modern.send(&request("tools/list", with_meta(json!({}), "1900-01-01")));
    assert_eq!(unknown["error"]["code"], -32022);
    assert_eq!(
        unknown["error"]["data"]["supported"],
        json!([MODERN, LEGACY])
    );
    assert_eq!(unknown["error"]["data"]["requested"], "1900-01-01");
    let bare = modern.send(&request("tools/list", json!({})));
    assert_eq!(bare["error"]["code"], -32602);
    let mut no_caps = with_meta(json!({}), MODERN);
    no_caps["_meta"]
        .as_object_mut()
        .unwrap()
        .remove("io.modelcontextprotocol/clientCapabilities");
    assert_eq!(
        modern.send(&request("tools/list", no_caps))["error"]["code"],
        -32602
    );
    let paged = modern.modern("tools/list", json!({"cursor": "x"}));
    assert_eq!(paged["error"]["code"], -32602);
    let missing = modern.modern(
        "tools/call",
        json!({"name": "story_update", "arguments": {}}),
    );
    assert_eq!(missing["error"]["code"], -32602);
    let method = modern.modern("resources/list", json!({}));
    assert_eq!(method["error"]["code"], -32601);
    let ping = modern.modern("ping", json!({}));
    assert_eq!(ping["result"]["resultType"], "complete");
    assert_eq!(modern.finish().0, Some(0));

    let mut legacy = fixture.server();
    let init = legacy.send(&request(
        "initialize",
        json!({"protocolVersion": LEGACY, "capabilities": {}, "clientInfo": {"name": "legacy-test", "version": "1"}}),
    ));
    assert_eq!(init["result"]["protocolVersion"], LEGACY);
    assert_eq!(
        init["result"]["capabilities"],
        json!({"tools": {"listChanged": false}})
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "story-time");
    legacy.notify(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let tools = legacy.send(&request("tools/list", json!({})));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 4);
    assert!(tools["result"].get("resultType").is_none());
    let called = legacy.send(&request(
        "tools/call",
        json!({"name": "story_show", "arguments": {"id": "ST-002"}}),
    ));
    assert_eq!(structured(&called["result"])["id"], "ST-002");
    let ping = legacy.send(&request("ping", json!({})));
    assert_eq!(ping["result"], json!({}));
    assert_eq!(legacy.finish().0, Some(0));
}

#[test]
fn refused_reads_carry_their_error_codes_and_change_no_file() {
    let fixture = Fixture::new().with_stories(2);
    let outside = fixture.base.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        outside.join("ST-009.md"),
        story(9, "ready", "story", None, ""),
    )
    .unwrap();
    let linked = fixture.base.join("linked");
    fs::create_dir_all(linked.join(".story-time")).unwrap();
    fs::write(linked.join(".story-time/settings.json"), SETTINGS).unwrap();
    std::os::unix::fs::symlink(&outside, linked.join("backlog")).unwrap();
    let unconfigured = fixture.base.join("bare");
    fs::create_dir_all(&unconfigured).unwrap();
    let invalid = fixture.base.join("broken");
    fs::create_dir_all(invalid.join(".story-time")).unwrap();
    fs::write(
        invalid.join(".story-time/settings.json"),
        r#"{"tracker": "svn"}"#,
    )
    .unwrap();
    fixture.write("backlog/ST-050.md", &"x".repeat(1024 * 1024 + 1));
    let before = fixture.snapshot();

    let roots = [&fixture.root, &linked, &unconfigured, &invalid];
    let mut args = Vec::new();
    for root in roots {
        args.push("--root");
        args.push(root.to_str().unwrap());
    }
    let mut server = Server::start(&args, &fixture.base, &[]);

    let config = server.tool("story_config", json!({}));
    let statuses: Vec<(Value, Value)> = structured(&config)["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["key"].clone(), r["status"].clone()))
        .collect();
    assert_eq!(
        statuses,
        [
            (json!("fixture"), json!("ok")),
            (json!("linked"), json!("ok")),
            (json!("bare"), json!("not_configured")),
            (json!("broken"), json!("config_invalid")),
        ]
    );

    let code = |server: &mut Server, name: &str, args: Value| -> Value {
        let result = server.tool(name, args);
        tool_error(&result)["code"].clone()
    };
    assert_eq!(
        code(&mut server, "story_list", json!({})),
        "repository_not_allowed"
    );
    assert_eq!(
        code(
            &mut server,
            "story_list",
            json!({"repository": "elsewhere"})
        ),
        "repository_not_allowed"
    );
    assert_eq!(
        code(
            &mut server,
            "story_show",
            json!({"repository": "fixture", "id": "../etc/passwd"})
        ),
        "invalid_story_id"
    );
    assert_eq!(
        code(
            &mut server,
            "story_show",
            json!({"repository": "fixture", "id": "PAY-1"})
        ),
        "invalid_story_id"
    );
    assert_eq!(
        code(
            &mut server,
            "story_show",
            json!({"repository": "fixture", "id": "ST-404"})
        ),
        "story_not_found"
    );
    assert_eq!(
        code(
            &mut server,
            "story_show",
            json!({"repository": "fixture", "id": "ST-050"})
        ),
        "story_too_large"
    );
    assert_eq!(
        code(&mut server, "story_list", json!({"repository": "fixture"})),
        "story_too_large"
    );
    assert_eq!(
        code(&mut server, "story_list", json!({"repository": "linked"})),
        "path_outside_repository"
    );
    assert_eq!(
        code(
            &mut server,
            "story_show",
            json!({"repository": "linked", "id": "ST-009"})
        ),
        "path_outside_repository"
    );
    assert_eq!(
        code(&mut server, "story_lint", json!({"repository": "linked"})),
        "path_outside_repository"
    );
    assert_eq!(
        code(&mut server, "story_list", json!({"repository": "bare"})),
        "not_configured"
    );
    let broken = server.tool("story_list", json!({"repository": "broken"}));
    let error = tool_error(&broken);
    assert_eq!(error["code"], "config_invalid");
    assert!(!error["message"]
        .as_str()
        .unwrap()
        .contains(fixture.base.to_str().unwrap()));
    let extra = server.tool(
        "story_list",
        json!({"repository": "fixture", "path": "/etc"}),
    );
    assert_eq!(tool_error(&extra)["details"]["pointer"], "/path");
    let wrong = server.tool(
        "story_lint",
        json!({"repository": "fixture", "ids": "ST-001"}),
    );
    assert_eq!(tool_error(&wrong)["code"], "invalid_argument");

    let parse = server.raw(b"{not json").unwrap();
    assert_eq!(parse["error"]["code"], -32700);
    assert_eq!(parse["id"], Value::Null);
    let not_rpc = server.raw(br#"{"id": 1, "method": "ping"}"#).unwrap();
    assert_eq!(not_rpc["error"]["code"], -32600);
    let mut huge = br#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{"pad":""#.to_vec();
    huge.extend(std::iter::repeat_n(b'a', 1024 * 1024));
    huge.extend(br#""}}"#);
    let limit = server.raw(&huge).unwrap();
    assert_eq!(limit["error"]["code"], -32600);
    assert_eq!(limit["error"]["message"], "Request exceeds input limit");
    let (code, rest, stderr) = server.finish();
    assert_eq!(code, Some(0));
    assert!(
        rest.is_empty(),
        "server kept reading after the limit: {rest}"
    );
    assert!(!stderr.contains(fixture.base.to_str().unwrap()), "{stderr}");
    assert_eq!(before, fixture.snapshot());
}

#[test]
fn no_output_carries_a_fake_token_and_a_pasted_token_is_redacted() {
    let fixture = Fixture::new();
    let gh = "fake-gh-env-value-0001";
    let gitea = "fake-gitea-env-value-0002";
    let file = "fake-file-secret-value-0003";
    let pasted = "ghp_FAKEfakeFAKEfake0123456789abcdef";
    fixture.write(".story-time/.github.env", &format!("GH_TOKEN={file}\n"));
    fixture.write(".story-time/.gitea.env", &format!("GITEA_TOKEN={file}\n"));
    fixture.write(
        "backlog/ST-001.md",
        &story(
            1,
            "ready",
            "story",
            None,
            &format!("\n## Notes\nPasted {pasted} and {gitea} by mistake. Header: Bearer abcdefgh12345678\n"),
        ),
    );
    fixture.write("backlog/ST-002.md", &story(2, "draft", "story", None, ""));
    let before = fixture.snapshot();
    let mut server = Server::start(
        &["--root", fixture.root.to_str().unwrap()],
        &fixture.root,
        &[("GH_TOKEN", gh), ("GITEA_TOKEN", gitea)],
    );
    let mut transcript = String::new();
    let show = server.tool("story_show", json!({"id": "ST-001"}));
    transcript.push_str(&show.to_string());
    let shown = structured(&show);
    let notes = shown["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["heading"] == "Notes")
        .unwrap()["body"]
        .as_str()
        .unwrap();
    assert_eq!(
        notes,
        "Pasted [redacted] and [redacted] by mistake. Header: [redacted]"
    );
    assert_eq!(shown["redactions"], 3);
    for (name, args) in [
        ("story_list", json!({})),
        ("story_lint", json!({})),
        ("story_config", json!({})),
        ("story_show", json!({"id": "ST-002"})),
        ("story_show", json!({"id": pasted})),
    ] {
        transcript.push_str(&server.tool(name, args).to_string());
    }
    let (code, rest, stderr) = server.finish();
    assert_eq!(code, Some(0));
    transcript.push_str(&rest);
    transcript.push_str(&stderr);
    for secret in [gh, gitea, file, pasted, "abcdefgh12345678"] {
        assert!(
            !transcript.contains(secret),
            "{secret} leaked: {transcript}"
        );
    }
    assert!(transcript.contains("[redacted]"));
    assert_eq!(before, fixture.snapshot());
}

/// The handshakes Claude Code and Codex send to a stdio server: `initialize`, then
/// `notifications/initialized`, `tools/list`, and a `tools/call`. A dual-era client may first
/// probe with `server/discover`.
#[test]
fn claude_code_and_codex_handshakes_list_the_fixture_stories() {
    let fixture = Fixture::new().with_stories(4);
    let expected = ids(&fixture.cli(&["list", "--json"]));
    for (client, version, probe) in [
        ("claude-code", LEGACY, false),
        ("codex-mcp-client", "2025-06-18", false),
        ("dual-era-client", MODERN, true),
    ] {
        let mut server = fixture.server();
        let listed = if probe {
            let discover = server.modern("server/discover", json!({}));
            assert!(discover["result"]["supportedVersions"]
                .as_array()
                .unwrap()
                .contains(&json!(MODERN)));
            server.modern("tools/call", json!({"name": "story_list", "arguments": {}}))
        } else {
            let init = server.send(&request(
                "initialize",
                json!({"protocolVersion": version, "capabilities": {"roots": {}}, "clientInfo": {"name": client, "version": "0"}}),
            ));
            assert_eq!(init["result"]["protocolVersion"], version, "{client}");
            server.notify(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
            let tools = server.send(&request("tools/list", json!({})));
            assert!(tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "story_list"));
            server.send(&request(
                "tools/call",
                json!({"name": "story_list", "arguments": {}}),
            ))
        };
        assert_eq!(
            ids(&structured(&listed["result"])["stories"]),
            expected,
            "{client}"
        );
        assert_eq!(server.finish().0, Some(0), "{client}");
    }
}

#[test]
fn story_mcp_refuses_an_unknown_option() {
    let output: Output = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["mcp", "--writes"])
        .env_clear()
        .env("PATH", "/nonexistent")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn story_lint_reads_an_outside_machine_the_same_whether_it_exists() {
    let fixture = Fixture::new();
    fs::write(fixture.base.join("present.toml"), "name = \"present\"\n").unwrap();
    std::os::unix::fs::symlink(&fixture.base, fixture.root.join("away")).unwrap();
    let base = fixture.base.to_str().unwrap();
    let paths = [
        ("ST-001", "../present.toml"),
        ("ST-002", "../missing.toml"),
        ("ST-003", &format!("{base}/present.toml")),
        ("ST-004", &format!("{base}/missing.toml")),
        ("ST-005", "away/present.toml"),
        ("ST-006", "away/missing.toml"),
    ];
    for (id, path) in &paths {
        let text = story(1, "ready", "story", None, "").replace("ST-001", id);
        let text = text.replacen(
            "depends_on",
            &format!("state_machine: {path}\ndepends_on"),
            1,
        );
        fixture.write(&format!("backlog/{id}.md"), &text);
    }
    let mut server = fixture.server();
    let lint = server.tool("story_lint", json!({}));
    let lint = structured(&lint).clone();
    let cli = fixture.cli(&["lint", "--json"]);
    for results in [&lint["results"], &cli] {
        let machine: Vec<String> = paths
            .iter()
            .map(|(id, path)| {
                let story = results
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["id"] == *id)
                    .unwrap();
                story["findings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|f| f["rule"] == "state_machine.exists")
                    .map(|f| f["message"].as_str().unwrap().replace(path, "PATH"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        assert!(
            machine.iter().all(|m| m
                == "state_machine: PATH is outside the repository (paths are relative to the repo root). Keep the model inside the repository."),
            "{machine:#?}"
        );
    }
    server.finish();
}

const READ_TOOLS: [&str; 4] = ["story_config", "story_list", "story_show", "story_lint"];

fn tool_names(server: &mut Server) -> Vec<String> {
    let listed = server.modern("tools/list", json!({}));
    listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

fn sha256(fixture: &Fixture, id: &str) -> String {
    let mut server = fixture.server();
    let shown = server.tool("story_show", json!({"id": id}));
    let sha = structured(&shown)["sha256"].as_str().unwrap().to_owned();
    server.finish();
    sha
}

fn changelog_lines(text: &str) -> usize {
    text.split("## Changelog")
        .nth(1)
        .map_or(0, |log| log.lines().filter(|l| l.starts_with("- ")).count())
}

#[test]
fn without_write_no_write_tool_is_listed_and_a_write_call_is_an_unknown_tool() {
    let fixture = Fixture::new().with_stories(2);
    let before = fixture.snapshot();
    let sha = sha256(&fixture, "ST-001");
    let mut server = fixture.server();
    assert_eq!(tool_names(&mut server), READ_TOOLS);
    for (name, arguments) in [
        (
            "story_update",
            json!({"id": "ST-001", "expected_sha256": sha, "set": {"status": "refined"}}),
        ),
        (
            "story_note",
            json!({"id": "ST-001", "expected_sha256": sha, "note": "hello"}),
        ),
    ] {
        let response = server.modern("tools/call", json!({"name": name, "arguments": arguments}));
        assert_eq!(response["error"]["code"], -32602, "{response}");
    }
    server.finish();
    assert_eq!(fixture.snapshot(), before);

    let mut server = fixture.writer();
    let listed = server.modern("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [READ_TOOLS.as_slice(), &["story_update", "story_note"]].concat()
    );
    for tool in tools {
        let write = tool["name"] == "story_update" || tool["name"] == "story_note";
        assert_eq!(tool["annotations"]["readOnlyHint"], !write, "{tool}");
        assert_eq!(tool["annotations"]["openWorldHint"], false, "{tool}");
    }
    let update = tools.iter().find(|t| t["name"] == "story_update").unwrap();
    assert_eq!(update["annotations"]["destructiveHint"], true);
    assert_eq!(update["annotations"]["idempotentHint"], true);
    assert_eq!(
        update["inputSchema"]["required"],
        json!(["id", "expected_sha256", "set"])
    );
    let note = tools.iter().find(|t| t["name"] == "story_note").unwrap();
    assert_eq!(note["annotations"]["idempotentHint"], false);
    server.finish();
}

#[test]
fn story_update_writes_what_story_set_writes_and_returns_audit_evidence() {
    for (sync, next) in [
        ("ask", json!("story push ST-002")),
        ("auto", json!("story push ST-002")),
        ("off", Value::Null),
    ] {
        let by_cli = Fixture::new().with_stories(3);
        let by_mcp = Fixture::new().with_stories(3);
        let settings = SETTINGS.replace("\"ask\"", &format!("\"{sync}\""));
        by_cli.write(".story-time/settings.json", &settings);
        by_mcp.write(".story-time/settings.json", &settings);
        let before = by_mcp.read("backlog/ST-002.md");
        let sha = sha256(&by_mcp, "ST-002");

        by_cli.cli(&[
            "set",
            "ST-002",
            "status=refined",
            "size=3",
            "depends_on=ST-001,ST-003",
            "type=",
            "--note",
            "Refined in review",
            "--json",
        ]);
        let mut server = by_mcp.writer();
        let schemas = output_schemas(&mut server);
        let result = server.tool(
            "story_update",
            json!({
                "id": "ST-002",
                "expected_sha256": sha,
                "set": {"status": "refined", "size": 3, "depends_on": ["ST-001", "ST-003"], "type": null},
                "note": "Refined in review",
            }),
        );
        let content = structured(&result).clone();
        conforms(&schemas["story_update"], &content, "story_update").unwrap();
        let (_, _, stderr) = server.finish();

        let written = by_mcp.read("backlog/ST-002.md");
        assert_eq!(written, by_cli.read("backlog/ST-002.md"));
        assert_ne!(written, before);
        assert_eq!(content["status"], "updated");
        assert_eq!(content["sha256_before"], sha);
        assert_eq!(content["sha256_after"], sha256(&by_mcp, "ST-002"));
        assert_eq!(content["story"]["status"], "refined");
        assert_eq!(content["story"]["sha256"], content["sha256_after"]);
        let line =
            "set status=refined, size=3, depends_on=ST-001,ST-003, type= — Refined in review";
        assert_eq!(content["audit"]["changelog"], line);
        assert!(written.contains(&format!(" {line}\n")), "{written}");
        assert_eq!(content["audit"]["operation"], "story_update");
        assert_eq!(content["audit"]["repository"], "fixture");
        assert_eq!(content["audit"]["id"], "ST-002");
        assert_eq!(
            content["audit"]["fields"],
            json!(["status", "size", "depends_on", "type"])
        );
        assert_eq!(content["publish"]["sync"], sync);
        assert_eq!(content["publish"]["tracker"], "github");
        assert_eq!(content["publish"]["pushed"], false);
        assert_eq!(content["publish"]["next"], next);

        let audit: Value = serde_json::from_str(stderr.lines().last().unwrap()).unwrap();
        assert_eq!(audit["tool"], "story_update");
        assert_eq!(audit["sha256_before"], content["sha256_before"]);
        assert_eq!(audit["sha256_after"], content["sha256_after"]);
        assert!(!stderr.contains("Refined in review"), "{stderr}");
    }
}

#[test]
fn story_update_refusals_carry_their_codes_and_change_nothing_unintended() {
    let fixture = Fixture::new().with_stories(3);
    fixture.write(
        "backlog/ST-004.md",
        "---\nid: ST-004\ntitle: Unfinished\nstatus: draft\ntype: story\ncreated: 2026-11-14\nupdated: 2026-11-14\n---\n\n## Story\nSomething.\n",
    );
    let sha = sha256(&fixture, "ST-002");
    let mut server = fixture.writer();
    let refuse = |server: &mut Server, arguments: Value| -> Value {
        let before = fixture.snapshot();
        let result = server.tool("story_update", arguments);
        assert_eq!(fixture.snapshot(), before);
        tool_error(&result).clone()
    };

    let stale = "0".repeat(64);
    let error = refuse(
        &mut server,
        json!({"id": "ST-002", "expected_sha256": stale, "set": {"status": "refined"}}),
    );
    assert_eq!(error["code"], "conflict");
    assert_eq!(error["retryable"], true);
    assert_eq!(error["details"]["expected_sha256"], stale);
    assert_eq!(error["details"]["current_sha256"], sha);

    for field in ["remote", "remote_url", "id", "created"] {
        let error = refuse(
            &mut server,
            json!({"id": "ST-002", "expected_sha256": sha, "set": {"status": "refined", field: "x"}}),
        );
        assert_eq!(error["code"], "field_not_writable", "{field}: {error}");
        assert_eq!(error["details"]["field"], field);
    }

    let draft = sha256(&fixture, "ST-004");
    let error = refuse(
        &mut server,
        json!({"id": "ST-004", "expected_sha256": draft, "set": {"status": "ready"}}),
    );
    assert_eq!(error["code"], "lifecycle_refused", "{error}");
    let findings = error["details"]["findings"].as_array().unwrap();
    assert!(!findings.is_empty());
    assert!(findings
        .iter()
        .all(|f| f["rule"].is_string() && f["message"].is_string()));

    let root = fs::File::open(&fixture.root).unwrap();
    root.try_lock().unwrap();
    let error = refuse(
        &mut server,
        json!({"id": "ST-002", "expected_sha256": sha, "set": {"status": "refined"}}),
    );
    assert_eq!(error["code"], "lock_held");
    assert_eq!(error["retryable"], true);
    drop(root);

    let update = json!({"id": "ST-002", "expected_sha256": sha, "set": {"status": "refined", "size": 3}, "note": "why"});
    let first = server.tool("story_update", update.clone());
    let first = structured(&first).clone();
    assert_eq!(first["status"], "updated");
    let applied = fixture.read("backlog/ST-002.md");
    let retry = server.tool("story_update", update);
    let retry = structured(&retry).clone();
    assert_eq!(retry["status"], "already_applied");
    assert_eq!(retry["sha256_before"], first["sha256_after"]);
    assert_eq!(retry["sha256_after"], first["sha256_after"]);
    assert_eq!(fixture.read("backlog/ST-002.md"), applied);
    assert_eq!(changelog_lines(&applied), 1);

    let error = refuse(
        &mut server,
        json!({"id": "ST-002", "expected_sha256": sha, "set": {"status": "done"}}),
    );
    assert_eq!(error["code"], "conflict");
    server.finish();
}

#[test]
fn story_note_appends_what_story_note_appends_and_a_stale_hash_is_a_conflict() {
    let by_cli = Fixture::new().with_stories(2);
    let by_mcp = Fixture::new().with_stories(2);
    let sha = sha256(&by_mcp, "ST-001");
    by_cli.cli(&["note", "ST-001", "Reworded the story", "--json"]);
    let mut server = by_mcp.writer();
    let schemas = output_schemas(&mut server);
    let result = server.tool(
        "story_note",
        json!({"id": "ST-001", "expected_sha256": sha, "note": "Reworded the story"}),
    );
    let content = structured(&result).clone();
    conforms(&schemas["story_note"], &content, "story_note").unwrap();
    assert_eq!(
        by_mcp.read("backlog/ST-001.md"),
        by_cli.read("backlog/ST-001.md")
    );
    assert_eq!(content["status"], "updated");
    assert_eq!(content["audit"]["changelog"], "Reworded the story");
    assert_eq!(content["publish"]["next"], "story push ST-001");

    let before = by_mcp.snapshot();
    let retry = server.tool(
        "story_note",
        json!({"id": "ST-001", "expected_sha256": sha, "note": "Reworded the story"}),
    );
    let error = tool_error(&retry);
    assert_eq!(error["code"], "conflict");
    assert_eq!(error["details"]["current_sha256"], content["sha256_after"]);
    assert_eq!(by_mcp.snapshot(), before);
    server.finish();
}
