//! Writes of a registry server (ST-133, ADR 0007): each story change is one commit on the
//! branch `story-time/<ID>` and one pull request. The upstream repositories are local git
//! repositories, and a stub stands in for the GitHub API.

use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const READER: &str = "reader-token-0123456789abcdef";
const WRITER: &str = "writer-token-0123456789abcdef";
const SECOND: &str = "second-token-0123456789abcdef";
const INSTALLATION_TOKEN: &str = "ghs_StubInstallationToken0123456789";

fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn story(id: &str, title: &str, extra: &str) -> String {
    format!(
        "---\nid: {id}\ntitle: {title}\nstatus: draft\ntype: story\n{extra}depends_on: []\ncreated: 2026-11-15\nupdated: 2026-11-15\n---\n\n## Story\nAs a billing admin, I want invoices exported, so that I can reconcile them.\n\n## Acceptance Criteria\n- [ ] Given an invoice, when I export, then a CSV row is written.\n"
    )
}

/// One pull request of the stub.
#[derive(Clone)]
struct Pull {
    number: u64,
    head: String,
    base: String,
    title: String,
    body: String,
    open: bool,
}

/// What the stub of the GitHub API saw and holds.
#[derive(Default)]
struct Hub {
    pulls: Vec<Pull>,
    /// The method, path, and `Authorization` header of each request.
    seen: Vec<(String, String, String)>,
    /// The forge refuses to open a pull request.
    refuse_pulls: bool,
}

struct Stub {
    url: String,
    hub: Arc<Mutex<Hub>>,
    server: Arc<tiny_http::Server>,
}
impl Stub {
    fn start() -> Self {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let hub: Arc<Mutex<Hub>> = Arc::default();
        let (listener, state, base) = (server.clone(), hub.clone(), url.clone());
        std::thread::spawn(move || {
            for mut request in listener.incoming_requests() {
                let full = request.url().to_owned();
                let (path, query) = full.split_once('?').unwrap_or((&full, ""));
                let authorization = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.as_str().to_owned())
                    .unwrap_or_default();
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                let method = request.method().to_string();
                let mut hub = state.lock().unwrap();
                hub.seen
                    .push((method.clone(), path.to_owned(), authorization.clone()));
                let shown = |pull: &Pull| {
                    json!({
                        "number": pull.number,
                        "html_url": format!("{base}/up/core/pull/{}", pull.number),
                        "state": if pull.open { "open" } else { "closed" },
                        "head": {"ref": pull.head},
                    })
                };
                let installation = format!("Bearer {INSTALLATION_TOKEN}");
                let (status, answer) = match (method.as_str(), path) {
                    ("GET", "/app") => (200, json!({"slug": "storytime", "id": 12345})),
                    ("GET", "/repos/up/core/installation") => (200, json!({"id": 7})),
                    ("POST", "/app/installations/7/access_tokens") => {
                        let expires =
                            (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
                        (
                            201,
                            json!({"token": INSTALLATION_TOKEN, "expires_at": expires}),
                        )
                    }
                    (_, "/repos/up/core/pulls") if authorization != installation => {
                        (401, json!({"message": "Bad credentials"}))
                    }
                    ("GET", "/repos/up/core/pulls") => {
                        // `head=up:<branch>&state=open`, as GitHub filters.
                        let head = query
                            .split('&')
                            .find_map(|pair| pair.strip_prefix("head=up:"))
                            .unwrap_or_default()
                            .replace("%2F", "/");
                        let found: Vec<Value> = hub
                            .pulls
                            .iter()
                            .filter(|pull| pull.open && pull.head == head)
                            .map(shown)
                            .collect();
                        (200, json!(found))
                    }
                    ("POST", "/repos/up/core/pulls") if hub.refuse_pulls => {
                        (502, json!({"message": "Server Error"}))
                    }
                    ("POST", "/repos/up/core/pulls") => {
                        let sent: Value = serde_json::from_str(&body).unwrap_or_default();
                        let pull = Pull {
                            number: hub.pulls.len() as u64 + 1,
                            head: sent["head"].as_str().unwrap_or_default().to_owned(),
                            base: sent["base"].as_str().unwrap_or_default().to_owned(),
                            title: sent["title"].as_str().unwrap_or_default().to_owned(),
                            body: sent["body"].as_str().unwrap_or_default().to_owned(),
                            open: true,
                        };
                        let answer = shown(&pull);
                        hub.pulls.push(pull);
                        (201, answer)
                    }
                    _ => (404, json!({"message": "Not Found"})),
                };
                drop(hub);
                let response = tiny_http::Response::from_string(answer.to_string())
                    .with_status_code(status)
                    .with_header(
                        "Content-Type: application/json"
                            .parse::<tiny_http::Header>()
                            .unwrap(),
                    );
                let _ = request.respond(response);
            }
        });
        Self { url, hub, server }
    }
    fn pulls(&self) -> Vec<Pull> {
        self.hub.lock().unwrap().pulls.clone()
    }
    fn close(&self, number: u64) {
        let mut hub = self.hub.lock().unwrap();
        hub.pulls
            .iter_mut()
            .filter(|pull| pull.number == number)
            .for_each(|pull| pull.open = false);
    }
}
impl Drop for Stub {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

/// The upstream repository `up/core` (prefix VC), a registry of it, and three clients.
struct Fixture {
    base: PathBuf,
    stub: Stub,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-mcp-write-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&base).unwrap();
        let fixture = Self {
            base: base.canonicalize().unwrap(),
            stub: Stub::start(),
        };
        let up = fixture.upstream();
        fs::create_dir_all(up.join(".story-time")).unwrap();
        fixture.git(&["init", "-q", "-b", "main"]);
        fs::write(
            up.join(".story-time/settings.json"),
            r#"{"tracker": "github", "sync": "ask", "backlogDir": "backlog", "prefix": "VC", "github": {"repo": "up/core"}}"#,
        )
        .unwrap();
        fixture.write("VC-109", &story("VC-109", "Roster API", ""));
        fixture.write("VC-108", &story("VC-108", "Schema", "remote: 12\n"));
        fixture.commit("first");
        let key = fixture.base.join("app-key.pem");
        let made = Command::new("openssl")
            .args(["genrsa", "-out", key.to_str().unwrap(), "2048"])
            .output()
            .expect("openssl makes the throwaway App key");
        assert!(made.status.success());
        fs::write(fixture.base.join("app-id"), "12345\n").unwrap();
        fs::write(
            fixture.base.join("registry.toml"),
            format!(
                "org = \"fixture\"\nfetch_ttl_seconds = 1\n\n[[repo]]\nkey = \"core\"\nclone = \"file://{}\"\nforge = \"github\"\nprefixes = [\"VC\"]\n",
                up.display()
            ),
        )
        .unwrap();
        let client = |id: &str, token: &str, scopes: &str| {
            format!(
                "[[client]]\nid = \"{id}\"\ntoken_sha256 = \"{}\"\nscopes = [{scopes}]\n\n",
                sha256(token.as_bytes())
            )
        };
        fs::write(
            fixture.base.join("clients.toml"),
            client("reader", READER, "\"read\"")
                + &client("writer", WRITER, "\"read\", \"write\"")
                + &client("second", SECOND, "\"read\", \"write\""),
        )
        .unwrap();
        fixture
    }
    fn upstream(&self) -> PathBuf {
        self.base.join("up").join("core")
    }
    fn write(&self, id: &str, text: &str) {
        let dir = self.upstream().join("backlog");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{id}.md")), text).unwrap();
    }
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(["-c", "user.name=Fixture", "-c", "commit.gpgsign=false"])
            .args(["-c", "user.email=fixture@example.invalid"])
            .args(args)
            .current_dir(self.upstream())
            .env("HOME", &self.base)
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
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
    /// The head of an upstream branch, or `None` when the branch does not exist.
    fn head(&self, branch: &str) -> Option<String> {
        let reference = format!("refs/heads/{branch}");
        let output = Command::new("git")
            .args(["rev-parse", "--verify", "--quiet", &reference])
            .current_dir(self.upstream())
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
    }
    /// The text of a story file on an upstream branch.
    fn file(&self, branch: &str, id: &str) -> String {
        self.git(&["show", &format!("{branch}:backlog/{id}.md")])
    }
    fn data(&self) -> PathBuf {
        self.base.join("data")
    }
    fn serve(&self) -> Http {
        let path = |name: &str| self.base.join(name).to_str().unwrap().to_owned();
        let mut child = Command::new(env!("CARGO_BIN_EXE_story"))
            .args(["mcp", "--http", "127.0.0.1:0"])
            .args([
                "--registry",
                &path("registry.toml"),
                "--data",
                &path("data"),
            ])
            .args(["--clients", &path("clients.toml")])
            .args(["--github-app-id-file", &path("app-id")])
            .args(["--github-app-key-file", &path("app-key.pem")])
            .args(["--github-api-url", &self.stub.url])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &self.base)
            .current_dir(&self.base)
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
            log,
        }
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
    log: Arc<Mutex<Vec<String>>>,
}
impl Http {
    fn rpc(&self, token: &str, method: &str, mut params: Value) -> Value {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        let reply: Value = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap()
            .post(format!("{}/mcp", self.url))
            .bearer_auth(token)
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }
    fn call(&self, token: &str, name: &str, arguments: Value) -> Value {
        self.rpc(
            token,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        )["structuredContent"]
            .clone()
    }
    fn ok(&self, token: &str, name: &str, arguments: Value) -> Value {
        let content = self.call(token, name, arguments);
        assert!(content.get("error").is_none(), "{name}: {content}");
        content
    }
    fn show(&self, id: &str) -> Value {
        self.ok(READER, "story_show", json!({"id": id}))
    }
    fn stderr(&self) -> String {
        self.log.lock().unwrap().join("\n")
    }
}
impl Drop for Http {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text(value: &Value) -> &str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("not text: {value}"))
}

#[test]
fn a_write_is_one_commit_on_the_story_branch_and_one_pull_request() {
    let fixture = Fixture::new();
    let server = fixture.serve();
    let main = fixture.head("main").unwrap();
    let before = server.show("VC-109");
    assert!(before["proposed"].is_null(), "{before}");

    let written = server.ok(
        WRITER,
        "story_update",
        json!({"id": "VC-109", "expected_sha256": before["sha256"], "set": {"title": "Roster API v2"}, "note": "Renamed after review"}),
    );
    assert_eq!(written["status"], "updated", "{written}");
    assert_eq!(written["state"], "proposed");
    assert_eq!(written["branch"], "story-time/VC-109");
    let pr_url = format!("{}/up/core/pull/1", fixture.stub.url);
    assert_eq!(written["pr_url"], pr_url.as_str());
    assert_eq!(written["sha256_before"], before["sha256"]);
    assert!(written["publish"]["next"].is_null(), "{written}");

    // Upstream: the default branch did not move, and the story branch is one commit ahead.
    assert_eq!(fixture.head("main").unwrap(), main);
    let head = fixture.head("story-time/VC-109").expect("the story branch");
    assert_eq!(written["head_sha"], head.as_str());
    assert_eq!(
        fixture.git(&["rev-list", "--count", "main..story-time/VC-109"]),
        "1"
    );
    assert_eq!(
        fixture.git(&["diff", "--name-only", "main", "story-time/VC-109"]),
        "backlog/VC-109.md"
    );
    let file = fixture.file("story-time/VC-109", "VC-109");
    assert!(file.contains("title: Roster API v2"), "{file}");
    assert_eq!(
        written["sha256_after"],
        sha256(format!("{file}\n").as_bytes()).as_str()
    );
    // The commit: the bot of the App is its author, and a trailer names the client.
    let commit = fixture.git(&["log", "-1", "--format=%an|%ae|%cn|%B", "story-time/VC-109"]);
    let (who, message) = commit
        .split_once("|story(")
        .unwrap_or_else(|| panic!("{commit}"));
    assert_eq!(
        who,
        "storytime[bot]|storytime[bot]@users.noreply.github.com|storytime[bot]"
    );
    assert!(message.starts_with("VC-109): "), "{commit}");
    assert_eq!(
        message.trim_end().lines().last().unwrap(),
        "Requested-by: writer",
        "{commit}"
    );

    // The forge: one pull request to the default branch, with the Stories line first.
    let pulls = fixture.stub.pulls();
    assert_eq!(pulls.len(), 1);
    assert_eq!(pulls[0].head, "story-time/VC-109");
    assert_eq!(pulls[0].base, "main");
    assert!(pulls[0].title.contains("VC-109") && pulls[0].title.contains("Roster API v2"));
    assert_eq!(pulls[0].body.lines().next().unwrap(), "Stories: VC-109");

    // A read of the story now names the proposal. A read of the branch returns its content.
    let shown = server.show("VC-109");
    assert_eq!(
        shown["frontmatter"]["title"], "Roster API",
        "the default branch"
    );
    assert_eq!(shown["sha256"], before["sha256"]);
    assert_eq!(
        shown["proposed"],
        json!({"ref": "story-time/VC-109", "content_sha256": written["sha256_after"], "pr_url": pr_url})
    );
    let branch = server.ok(
        READER,
        "story_show",
        json!({"id": "VC-109", "ref": "story-time/VC-109"}),
    );
    assert_eq!(branch["frontmatter"]["title"], "Roster API v2");
    assert_eq!(branch["sha256"], written["sha256_after"]);

    // A second write compares against the branch, adds one commit, and opens no new PR.
    let stale = server.call(
        WRITER,
        "story_update",
        json!({"id": "VC-109", "expected_sha256": before["sha256"], "set": {"size": 3}}),
    );
    assert_eq!(stale["error"]["code"], "conflict", "{stale}");
    assert_eq!(stale["error"]["details"]["ref"], "story-time/VC-109");
    assert_eq!(
        stale["error"]["details"]["current_sha256"],
        written["sha256_after"]
    );
    assert_eq!(
        fixture.head("story-time/VC-109").unwrap(),
        head,
        "nothing written"
    );

    let noted = server.ok(
        WRITER,
        "story_note",
        json!({"id": "VC-109", "expected_sha256": written["sha256_after"], "note": "Reviewed with the team"}),
    );
    assert_eq!(noted["state"], "proposed");
    assert_eq!(noted["pr_url"], pr_url.as_str());
    assert_eq!(
        fixture.git(&["rev-list", "--count", "main..story-time/VC-109"]),
        "2"
    );
    assert_eq!(
        fixture.stub.pulls().len(),
        1,
        "the open pull request is reused"
    );

    // Two clients hold the same content hash. The second one to write gets a conflict.
    let held = noted["sha256_after"].clone();
    let first = server.ok(
        WRITER,
        "story_update",
        json!({"id": "VC-109", "expected_sha256": held, "set": {"size": 3}}),
    );
    let after_first = fixture.head("story-time/VC-109").unwrap();
    let second = server.call(
        SECOND,
        "story_update",
        json!({"id": "VC-109", "expected_sha256": held, "set": {"size": 5}}),
    );
    assert_eq!(second["error"]["code"], "conflict", "{second}");
    assert_eq!(second["error"]["retryable"], true);
    assert_eq!(second["error"]["details"]["ref"], "story-time/VC-109");
    assert_eq!(
        second["error"]["details"]["current_sha256"],
        first["sha256_after"]
    );
    assert_eq!(fixture.head("story-time/VC-109").unwrap(), after_first);
    assert!(fixture
        .file("story-time/VC-109", "VC-109")
        .contains("size: 3"));

    // The lifecycle gate stays: done needs ticked criteria and evidence. Nothing is pushed.
    let done = server.call(
        WRITER,
        "story_update",
        json!({"id": "VC-109", "expected_sha256": first["sha256_after"], "set": {"status": "done"}}),
    );
    assert_eq!(done["error"]["code"], "lifecycle_refused", "{done}");
    assert_eq!(fixture.head("story-time/VC-109").unwrap(), after_first);

    // A story with a tracker issue names it on the Stories line.
    let other = server.show("VC-108");
    server.ok(
        WRITER,
        "story_note",
        json!({"id": "VC-108", "expected_sha256": other["sha256"], "note": "Checked the schema"}),
    );
    let pulls = fixture.stub.pulls();
    assert_eq!(pulls.len(), 2);
    assert_eq!(pulls[1].head, "story-time/VC-108");
    assert_eq!(
        pulls[1].body.lines().next().unwrap(),
        "Stories: VC-108 (#12)"
    );

    // The scopes select the tools, and each forge call carried the installation token.
    let names = |token: &str| -> Vec<String> {
        server.rpc(token, "tools/list", json!({}))["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| text(&tool["name"]).to_owned())
            .collect()
    };
    assert!(!names(READER).contains(&"story_update".to_owned()));
    let tools = server.rpc(WRITER, "tools/list", json!({}))["tools"].clone();
    let update = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "story_update")
        .expect("a writer has story_update");
    for key in ["state", "pr_url", "branch", "head_sha"] {
        assert!(
            update["outputSchema"]["properties"][key].is_object(),
            "{key}"
        );
    }
    let refused = server.call(
        READER,
        "story_note",
        json!({"id": "VC-109", "expected_sha256": held, "note": "x"}),
    );
    assert_eq!(refused["error"]["code"], "scope_refused", "{refused}");
    let hub = fixture.stub.hub.lock().unwrap();
    for (method, path, authorization) in hub.seen.iter().filter(|s| s.1.ends_with("/pulls")) {
        assert_eq!(
            authorization,
            &format!("Bearer {INSTALLATION_TOKEN}"),
            "{method} {path}"
        );
    }
    drop(hub);
    let audit = server.stderr();
    assert!(audit.contains("\"client_id\":\"writer\""), "{audit}");
    assert!(!audit.contains(INSTALLATION_TOKEN) && !audit.contains("Renamed after review"));
}

#[test]
fn a_push_that_the_lease_rejects_is_a_conflict_and_changes_nothing() {
    let fixture = Fixture::new();
    // Another writer pushed the story branch after the server fetched. The upstream hides the
    // branch from a fetch, so the server pushes with a lease that expects no branch.
    fixture.git(&["checkout", "-q", "-b", "story-time/VC-109"]);
    fixture.write(
        "VC-109",
        &story("VC-109", "Roster API, pushed by another writer", ""),
    );
    let theirs = fixture.commit("another writer");
    fixture.git(&["checkout", "-q", "main"]);
    fixture.git(&["config", "uploadpack.hideRefs", "refs/heads/story-time"]);
    let server = fixture.serve();

    let before = server.show("VC-109");
    assert_eq!(before["frontmatter"]["title"], "Roster API");
    let pushed = server.call(
        WRITER,
        "story_update",
        json!({"id": "VC-109", "expected_sha256": before["sha256"], "set": {"title": "Mine"}}),
    );
    assert_eq!(pushed["error"]["code"], "conflict", "{pushed}");
    assert_eq!(pushed["error"]["retryable"], true);
    assert_eq!(pushed["error"]["details"]["ref"], "story-time/VC-109");
    assert_eq!(fixture.head("story-time/VC-109").unwrap(), theirs);
    assert!(
        fixture.stub.pulls().is_empty(),
        "no pull request for a rejected push"
    );
}

#[test]
fn a_branch_whose_pull_request_closed_is_left_behind_and_the_next_write_starts_again() {
    let fixture = Fixture::new();
    let server = fixture.serve();
    let before = server.show("VC-109");
    let first = server.ok(
        WRITER,
        "story_update",
        json!({"id": "VC-109", "expected_sha256": before["sha256"], "set": {"title": "Rejected title"}}),
    );
    assert_eq!(first["state"], "proposed");
    let rejected = fixture.head("story-time/VC-109").unwrap();
    // A reviewer closes the pull request without a merge. GitHub keeps the branch.
    fixture.stub.close(1);
    std::thread::sleep(Duration::from_millis(1200));
    let shown = server.show("VC-109");
    assert!(shown["proposed"].is_null(), "no open pull request: {shown}");

    // The next write compares against the default branch and starts the branch from it.
    let again = server.ok(
        WRITER,
        "story_update",
        json!({"id": "VC-109", "expected_sha256": before["sha256"], "set": {"size": 2}}),
    );
    assert_eq!(again["state"], "proposed");
    assert_eq!(
        again["pr_url"],
        format!("{}/up/core/pull/2", fixture.stub.url).as_str()
    );
    let head = fixture.head("story-time/VC-109").unwrap();
    assert_ne!(head, rejected);
    assert_eq!(
        fixture.git(&["rev-list", "--count", "main..story-time/VC-109"]),
        "1"
    );
    let file = fixture.file("story-time/VC-109", "VC-109");
    assert!(
        file.contains("title: Roster API\n") && file.contains("size: 2"),
        "the rejected change is not in the new branch: {file}"
    );
    assert_eq!(fixture.stub.pulls().len(), 2);
}

#[test]
fn two_story_create_calls_at_once_get_different_ids_and_their_own_branches() {
    let fixture = Fixture::new();
    let server = Arc::new(fixture.serve());
    let main = fixture.head("main").unwrap();
    let create = |token: &'static str, title: &'static str| {
        let server = server.clone();
        std::thread::spawn(move || {
            server.ok(
                token,
                "story_create",
                json!({"title": title, "statement": "As a billing admin, I want a report, so that I can check totals."}),
            )
        })
    };
    let (a, b) = (
        create(WRITER, "First new story"),
        create(SECOND, "Second new story"),
    );
    let (a, b) = (a.join().unwrap(), b.join().unwrap());
    let mut ids = vec![
        text(&a["story"]["id"]).to_owned(),
        text(&b["story"]["id"]).to_owned(),
    ];
    ids.sort();
    assert_eq!(ids, ["VC-110", "VC-111"], "{a} {b}");
    for created in [&a, &b] {
        let id = text(&created["story"]["id"]);
        assert_eq!(created["status"], "created");
        assert_eq!(created["state"], "proposed");
        assert_eq!(created["branch"], format!("story-time/{id}").as_str());
        let branch = format!("story-time/{id}");
        assert_eq!(
            fixture.head(&branch).as_deref(),
            created["head_sha"].as_str(),
            "{created}"
        );
        assert_eq!(
            fixture.git(&["diff", "--name-only", "main", &branch]),
            format!("backlog/{id}.md")
        );
        assert!(fixture.file(&branch, id).contains(&format!("id: {id}\n")));
    }
    assert_eq!(fixture.head("main").unwrap(), main);
    assert_eq!(fixture.stub.pulls().len(), 2);

    // An id on a story branch is reserved, although the default branch does not hold it.
    let third = server.ok(WRITER, "story_create", json!({"title": "Third new story"}));
    assert_eq!(third["story"]["id"], "VC-112");
    // The new story is read through its branch until its pull request merges.
    let shown = server.ok(
        READER,
        "story_show",
        json!({"id": "VC-112", "ref": "story-time/VC-112"}),
    );
    assert_eq!(shown["frontmatter"]["title"], "Third new story");
    let duplicate = server.call(WRITER, "story_create", json!({"title": "Roster API"}));
    assert_eq!(duplicate["error"]["code"], "duplicate_title", "{duplicate}");
}

#[test]
fn what_a_crash_left_in_the_data_directory_is_removed_at_start() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let tree = data.join("trees/core/0123456789abcdef0123456789abcdef01234567.7");
    fs::create_dir_all(tree.join("backlog")).unwrap();
    fs::write(tree.join("backlog/VC-109.md"), "left by a crash").unwrap();
    fs::create_dir_all(data.join("tmp")).unwrap();
    let index = data.join("tmp/index.core.7");
    fs::write(&index, "left by a crash").unwrap();

    let server = fixture.serve();
    assert!(!tree.exists(), "the commit directory is removed");
    assert!(!index.exists(), "the index file is removed");
    // The server works after the cleanup, and a write leaves nothing behind.
    let before = server.show("VC-109");
    server.ok(
        WRITER,
        "story_note",
        json!({"id": "VC-109", "expected_sha256": before["sha256"], "note": "After the restart"}),
    );
    let left = |dir: &Path| -> Vec<PathBuf> {
        fs::read_dir(dir)
            .map(|entries| entries.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    };
    assert!(
        left(&data.join("tmp")).is_empty(),
        "{:?}",
        left(&data.join("tmp"))
    );
    // The one directory left is the commit that the read used. The write removed its own.
    let trees = left(&data.join("trees/core"));
    assert_eq!(trees.len(), 1, "{trees:?}");
}

#[test]
fn a_pull_request_that_the_forge_does_not_open_leaves_no_branch() {
    let fixture = Fixture::new();
    fixture.stub.hub.lock().unwrap().refuse_pulls = true;
    let server = fixture.serve();
    let before = server.show("VC-109");
    let update = json!({"id": "VC-109", "expected_sha256": before["sha256"], "set": {"size": 2}});
    let refused = server.call(WRITER, "story_update", update.clone());
    assert_eq!(refused["error"]["code"], "forge_unavailable", "{refused}");
    assert_eq!(refused["error"]["retryable"], true);
    assert_eq!(
        fixture.head("story-time/VC-109"),
        None,
        "the branch is removed"
    );
    assert!(server.show("VC-109")["proposed"].is_null());

    // The same call succeeds when the forge answers again.
    fixture.stub.hub.lock().unwrap().refuse_pulls = false;
    let written = server.ok(WRITER, "story_update", update);
    assert_eq!(written["state"], "proposed");
    assert_eq!(
        fixture.head("story-time/VC-109").as_deref(),
        written["head_sha"].as_str()
    );
    assert_eq!(fixture.stub.pulls().len(), 1);
}
