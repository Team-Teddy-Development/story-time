//! `story mcp --http --registry` (ST-130, ADR 0007): the server reads stories from bare clones
//! of the registered repositories. The upstream repositories here are local git repositories,
//! and the GitHub App test serves one over HTTP behind a stub of the GitHub API.

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
const TTL: u64 = 3;

fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn settings(prefix: &str) -> String {
    format!(
        r#"{{"tracker": "files", "sync": "off", "backlogDir": "backlog", "prefix": "{prefix}", "team": {{"wipLimit": 2}}}}"#
    )
}

fn story(id: &str, title: &str, links: &str) -> String {
    format!(
        "---\nid: {id}\ntitle: {title}\nstatus: draft\ntype: story\nvalue: 3\ntime_criticality: 2\nrisk_reduction: 1\nsize: 2\n{links}created: 2026-11-15\nupdated: 2026-11-15\n---\n\n## Story\nAs a billing admin, I want invoices exported, so that I can reconcile them.\n\n## Acceptance Criteria\n- [ ] Given an invoice, when I export, then a CSV row is written.\n"
    )
}

struct Fixture {
    base: PathBuf,
}
impl Fixture {
    /// Upstream `core` (VC) and `web` (VE). `core` also has the branch `story-time/VC-109`.
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-mcp-hosted-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&base).unwrap();
        let fixture = Self {
            base: base.canonicalize().unwrap(),
        };
        for (name, prefix) in [("core", "VC"), ("web", "VE")] {
            let up = fixture.upstream(name);
            fs::create_dir_all(&up).unwrap();
            fixture.git(&up, &["init", "-q", "-b", "main"]);
            fixture.write(name, ".story-time/settings.json", &settings(prefix));
        }
        fixture.write(
            "core",
            "backlog/VC-109.md",
            &story("VC-109", "Roster API", "depends_on: []\n"),
        );
        // `core` keeps a story from before it took the VC prefix. The registry declares both.
        fixture.write(
            "core",
            "backlog/ST-001.md",
            &story("ST-001", "Legacy story", "depends_on: []\n"),
        );
        fixture.write(
            "web",
            "backlog/VE-046.md",
            &story("VE-046", "Roster page", "depends_on: [VC-109]\n"),
        );
        fixture.write(
            "web",
            "backlog/XX-001.md",
            &story("XX-001", "Stray prefix", "depends_on: []\n"),
        );
        fixture.commit("core", "first");
        fixture.commit("web", "first");
        let core = fixture.upstream("core");
        fixture.git(&core, &["checkout", "-q", "-b", "story-time/VC-109"]);
        fixture.write(
            "core",
            "backlog/VC-109.md",
            &story("VC-109", "Roster API, proposed", "depends_on: []\n"),
        );
        fixture.commit("core", "proposed");
        fixture.git(&core, &["checkout", "-q", "main"]);
        fs::write(
            fixture.base.join("clients.toml"),
            format!(
                "[[client]]\nid = \"reader\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\"]\n\n[[client]]\nid = \"writer\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\", \"write\"]\n",
                sha256(READER.as_bytes()),
                sha256(WRITER.as_bytes())
            ),
        )
        .unwrap();
        fixture
    }
    fn upstream(&self, name: &str) -> PathBuf {
        self.base.join("up").join(name)
    }
    fn write(&self, name: &str, relative: &str, text: &str) {
        let path = self.upstream(name).join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args([
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .current_dir(dir)
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
    fn commit(&self, name: &str, message: &str) -> String {
        let up = self.upstream(name);
        self.git(&up, &["add", "-A"]);
        self.git(&up, &["commit", "-q", "-m", message]);
        self.git(&up, &["rev-parse", "HEAD"])
    }
    fn head(&self, name: &str, reference: &str) -> String {
        self.git(&self.upstream(name), &["rev-parse", reference])
    }
    fn file_url(&self, name: &str) -> String {
        format!("file://{}", self.upstream(name).display())
    }
    /// A registry of `core` and `web`, plus the extra `[[repo]]` text.
    fn registry(&self, extra: &str) -> PathBuf {
        let path = self.base.join("registry.toml");
        fs::write(
            &path,
            format!(
                "org = \"fixture\"\nfetch_ttl_seconds = {TTL}\n\n[[repo]]\nkey = \"core\"\nclone = \"{}\"\nforge = \"github\"\nprefixes = [\"VC\", \"ST\"]\n\n[[repo]]\nkey = \"web\"\nclone = \"{}\"\nforge = \"github\"\nprefixes = [\"VE\"]\n{extra}",
                self.file_url("core"),
                self.file_url("web")
            ),
        )
        .unwrap();
        path
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_story"));
        command
            .arg("mcp")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &self.base)
            .current_dir(&self.base);
        command
    }
    fn hosted_args(&self, registry: &Path) -> Vec<String> {
        [
            "--http",
            "127.0.0.1:0",
            "--registry",
            registry.to_str().unwrap(),
            "--data",
            self.base.join("data").to_str().unwrap(),
            "--clients",
            self.base.join("clients.toml").to_str().unwrap(),
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }
    fn serve(&self, registry: &Path, extra: &[&str]) -> Http {
        let mut child = self
            .command()
            .args(self.hosted_args(registry))
            .args(extra)
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
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap(),
            log,
        }
    }
    /// The exit code and stderr of a server that must refuse to start.
    fn refused(&self, args: &[String]) -> (Option<i32>, String) {
        let output = self.command().args(args).output().unwrap();
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
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
    fn rpc(&self, token: &str, method: &str, mut params: Value) -> Value {
        params.as_object_mut().unwrap().insert(
            "_meta".into(),
            json!({
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {}
            }),
        );
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let response = self
            .client
            .post(format!("{}/mcp", self.url))
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .body(message.to_string())
            .send()
            .unwrap();
        assert_eq!(response.status(), 200);
        let reply: Value = response.json().unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }
    /// The structured content of one tool call, whether it succeeded or not.
    fn call(&self, name: &str, arguments: Value) -> Value {
        self.rpc(
            READER,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        )["structuredContent"]
            .clone()
    }
    /// The structured content of a tool call that must succeed.
    fn ok(&self, name: &str, arguments: Value) -> Value {
        let content = self.call(name, arguments);
        assert!(content.get("error").is_none(), "{name}: {content}");
        content
    }
    fn tools(&self, token: &str) -> Vec<Value> {
        self.rpc(token, "tools/list", json!({}))["tools"]
            .as_array()
            .unwrap()
            .clone()
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

fn title(shown: &Value) -> &str {
    shown["frontmatter"]["title"]
        .as_str()
        .unwrap_or_else(|| panic!("{shown}"))
}

#[test]
fn a_read_within_the_ttl_does_not_fetch_and_a_read_after_it_sees_the_new_commit() {
    let fixture = Fixture::new();
    let server = fixture.serve(&fixture.registry(""), &[]);
    let first_head = fixture.head("core", "main");

    let first = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(first["repository"], "core");
    assert_eq!(first["sha"], first_head.as_str());
    assert_eq!(title(&first), "Roster API");
    let fetched_at = first["fetched_at"].as_str().unwrap().to_owned();
    assert!(
        chrono::DateTime::parse_from_rfc3339(&fetched_at).is_ok(),
        "{fetched_at}"
    );

    fixture.write(
        "core",
        "backlog/VC-109.md",
        &story("VC-109", "Roster API, second commit", "depends_on: []\n"),
    );
    let second_head = fixture.commit("core", "second");
    assert_ne!(first_head, second_head);

    let within = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(
        within["sha"],
        first_head.as_str(),
        "no fetch within the TTL"
    );
    assert_eq!(within["fetched_at"], fetched_at.as_str());
    assert_eq!(title(&within), "Roster API");

    std::thread::sleep(Duration::from_millis(TTL * 1000 + 300));
    let after = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(after["sha"], second_head.as_str());
    assert_ne!(after["fetched_at"], fetched_at.as_str());
    assert_eq!(title(&after), "Roster API, second commit");
    assert_ne!(after["sha256"], first["sha256"]);

    // Every read tool names the commit it read.
    let listed = server.ok("story_list", json!({"repository": "core"}));
    assert_eq!(listed["sha"], second_head.as_str());
    assert!(listed["fetched_at"].is_string(), "{listed}");
    let audit: Vec<Value> = server
        .log
        .lock()
        .unwrap()
        .iter()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let last = audit.last().unwrap();
    assert_eq!(last["repository"], "core");
    assert_eq!(last["sha"], second_head.as_str());
    assert_eq!(last["client_id"], "reader");
}

#[test]
fn a_named_ref_reads_the_story_of_that_branch() {
    let fixture = Fixture::new();
    let server = fixture.serve(&fixture.registry(""), &[]);
    let branch = server.ok(
        "story_show",
        json!({"repository": "core", "id": "VC-109", "ref": "story-time/VC-109"}),
    );
    assert_eq!(title(&branch), "Roster API, proposed");
    assert_eq!(
        branch["sha"],
        fixture.head("core", "story-time/VC-109").as_str()
    );
    let default = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(title(&default), "Roster API");
    assert_eq!(default["sha"], fixture.head("core", "main").as_str());

    let missing = server.call(
        "story_show",
        json!({"repository": "core", "id": "VC-109", "ref": "no-such-branch"}),
    );
    assert_eq!(missing["error"]["code"], "ref_not_found", "{missing}");
    for bad in ["../main", "-x", "a..b", "main lock", ""] {
        let refused = server.call(
            "story_show",
            json!({"repository": "core", "id": "VC-109", "ref": bad}),
        );
        assert_eq!(
            refused["error"]["code"], "invalid_argument",
            "{bad}: {refused}"
        );
    }

    let show = server
        .tools(READER)
        .into_iter()
        .find(|tool| tool["name"] == "story_show")
        .unwrap();
    assert!(
        show["inputSchema"]["properties"]["ref"].is_object(),
        "{show}"
    );
    for key in ["sha", "fetched_at"] {
        assert!(show["outputSchema"]["properties"][key].is_object(), "{key}");
    }
}

#[test]
fn a_repository_that_cannot_be_reached_is_unavailable_and_the_rest_keep_working() {
    let fixture = Fixture::new();
    let gone = format!(
        "\n[[repo]]\nkey = \"gone\"\nclone = \"file://{}/up/gone\"\nforge = \"github\"\nprefixes = [\"GX\"]\n",
        fixture.base.display()
    );
    let server = fixture.serve(&fixture.registry(&gone), &[]);

    let shown = server.call("story_show", json!({"repository": "gone", "id": "GX-001"}));
    assert_eq!(shown["error"]["code"], "repository_unavailable", "{shown}");
    assert_eq!(shown["error"]["retryable"], true);
    server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));

    let repos = server.ok("story_repos", json!({}));
    let by_key = |key: &str| {
        repos["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["key"] == key)
            .unwrap_or_else(|| panic!("{repos}"))
            .clone()
    };
    let core = by_key("core");
    assert_eq!(core["state"], "ready");
    assert_eq!(core["sha"], fixture.head("core", "main").as_str());
    assert_eq!(core["prefixes"], json!(["VC", "ST"]));
    assert!(core["fetched_at"].is_string() && core["last_error"].is_null());
    let gone = by_key("gone");
    assert_eq!(gone["state"], "unavailable");
    assert!(gone["sha"].is_null() && gone["fetched_at"].is_null());
    assert!(
        gone["last_error"].as_str().is_some_and(|e| !e.is_empty()),
        "{gone}"
    );

    let ready = server
        .client
        .get(format!("{}/readyz", server.url))
        .send()
        .unwrap();
    assert_eq!(ready.status(), 200);

    // The probe that the container runs: 0 for a server that answers, 1 for none.
    let probe = |url: String| {
        let output = fixture.command().args(["probe", "--url", &url]).output();
        output.unwrap().status.code()
    };
    assert_eq!(probe(format!("{}/readyz", server.url)), Some(0));
    assert_eq!(
        probe(format!("{}/mcp", server.url)),
        Some(1),
        "GET /mcp is 405"
    );
    assert_eq!(probe("http://127.0.0.1:1/readyz".into()), Some(1));
    assert_eq!(probe("file:///etc/hostname".into()), Some(2));

    // A lint that needs every repository says which one it could not read.
    let lint = server.call("story_lint", json!({"repository": "web"}));
    assert_eq!(lint["error"]["code"], "repository_unavailable", "{lint}");
    assert_eq!(lint["error"]["details"]["repository"], "gone");
}

#[test]
fn a_registry_that_names_gitea_or_repeats_a_prefix_is_refused_at_start() {
    let fixture = Fixture::new();
    let args = |registry: &Path| fixture.hosted_args(registry);

    let gitea = fixture.registry(
        "\n[[repo]]\nkey = \"ops\"\nclone = \"https://gitea.example/t/ops.git\"\nforge = \"gitea\"\nprefixes = [\"OP\"]\n",
    );
    let (code, stderr) = fixture.refused(&args(&gitea));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("ops") && stderr.contains("gitea"),
        "{stderr}"
    );

    let repeated = fixture.registry(
        "\n[[repo]]\nkey = \"other\"\nclone = \"https://github.com/o/other.git\"\nforge = \"github\"\nprefixes = [\"VE\"]\n",
    );
    let (code, stderr) = fixture.refused(&args(&repeated));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("VE") && stderr.contains("web") && stderr.contains("other"),
        "{stderr}"
    );

    let good = fixture.registry("");
    let mut with_root = args(&good);
    with_root.extend(["--root".to_owned(), fixture.base.display().to_string()]);
    let (code, stderr) = fixture.refused(&with_root);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("--registry") && stderr.contains("--root"),
        "{stderr}"
    );

    // The data directory must be a directory that the server can write.
    let clients = fixture.base.join("clients.toml").display().to_string();
    let mut without_data = args(&good);
    let at = without_data.iter().position(|arg| arg == "--data").unwrap();
    without_data[at + 1] = clients;
    let (code, stderr) = fixture.refused(&without_data);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--data"), "{stderr}");
}

#[test]
fn the_registry_is_the_org_for_links_and_declares_each_prefix() {
    let fixture = Fixture::new();
    let server = fixture.serve(&fixture.registry(""), &[]);
    let lint = server.ok("story_lint", json!({"repository": "web"}));
    let findings = |id: &str| -> Vec<(String, String)> {
        lint["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|result| result["id"] == id)
            .unwrap_or_else(|| panic!("{lint}"))["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["rule"].as_str().unwrap().to_owned(),
                    f["severity"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    };
    // VC-109 is in `core`, which the registry gives the VC prefix.
    let linked = findings("VE-046");
    assert!(
        !linked.contains(&("independent.deps".into(), "error".into())),
        "{linked:?}"
    );
    assert!(
        !linked
            .iter()
            .any(|f| f.0 == "independent.foreign_unresolved"),
        "{linked:?}"
    );
    // `web` declares VE only, so a story file with another prefix is an error.
    assert!(
        findings("XX-001").contains(&("links.undeclared_prefix".into(), "error".into())),
        "{:?}",
        findings("XX-001")
    );
    assert!(!findings("VE-046")
        .iter()
        .any(|f| f.0 == "links.undeclared_prefix"));

    // `core` declares VC and ST, so a story of either prefix is read and has no finding.
    let legacy = server.ok("story_show", json!({"repository": "core", "id": "ST-001"}));
    assert_eq!(title(&legacy), "Legacy story");
    let core = server.ok(
        "story_lint",
        json!({"repository": "core", "ids": ["ST-001"]}),
    );
    assert!(
        !core.to_string().contains("links.undeclared_prefix"),
        "{core}"
    );
    let stray = server.call("story_show", json!({"repository": "web", "id": "VC-109"}));
    assert_eq!(stray["error"]["code"], "invalid_story_id", "{stray}");
    assert_eq!(
        stray["error"]["message"],
        "A story id is VE- followed by digits."
    );

    // Writes arrive with ST-133: until then a registry server offers no write tool.
    for token in [READER, WRITER] {
        let names: Vec<String> = server
            .tools(token)
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect();
        assert!(names.contains(&"story_repos".to_owned()), "{names:?}");
        for write in ["story_update", "story_note", "story_create"] {
            assert!(!names.contains(&write.to_owned()), "{token}: {names:?}");
        }
    }
    let config = server.ok("story_config", json!({}));
    let keys: Vec<&str> = config["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["core", "web"]);
}

/// A stub of GitHub: the App API under `/api`, and one private repository under `/git` that it
/// serves as static files (git's dumb HTTP protocol) to a request with the installation token.
struct Stub {
    url: String,
    seen: Arc<Mutex<Vec<(String, String, String)>>>,
    server: Arc<tiny_http::Server>,
}
const INSTALLATION_TOKEN: &str = "ghs_StubInstallationToken0123456789";

impl Stub {
    fn start(files: PathBuf) -> Self {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (listener, log) = (server.clone(), seen.clone());
        let basic = format!(
            "basic {}",
            base64(format!("x-access-token:{INSTALLATION_TOKEN}").as_bytes())
        );
        std::thread::spawn(move || {
            for request in listener.incoming_requests() {
                let path = request.url().split('?').next().unwrap().to_owned();
                let authorization = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.as_str().to_owned())
                    .unwrap_or_default();
                log.lock().unwrap().push((
                    request.method().to_string(),
                    path.clone(),
                    authorization.clone(),
                ));
                let respond = |status: u16, body: Vec<u8>| {
                    tiny_http::Response::from_data(body).with_status_code(status)
                };
                let response = if path == "/api/repos/o/core/installation" {
                    respond(200, br#"{"id": 7}"#.to_vec())
                } else if path == "/api/app/installations/7/access_tokens" {
                    let expires = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
                    respond(
                        201,
                        json!({"token": INSTALLATION_TOKEN, "expires_at": expires})
                            .to_string()
                            .into_bytes(),
                    )
                } else if let Some(relative) = path.strip_prefix("/git/") {
                    if !authorization.eq_ignore_ascii_case(&basic) {
                        respond(401, Vec::new())
                    } else {
                        match fs::read(files.join(relative)) {
                            Ok(bytes) if !relative.contains("..") => respond(200, bytes),
                            _ => respond(404, Vec::new()),
                        }
                    }
                } else {
                    respond(404, Vec::new())
                };
                let _ = request.respond(response);
            }
        });
        Self { url, seen, server }
    }
}
impl Drop for Stub {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64url_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'-' => 62,
            b'_' => 63,
            _ => panic!("not base64url: {text}"),
        }
    };
    let mut out = Vec::new();
    for chunk in text.as_bytes().chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, c)| n | (value(*c) << (18 - 6 * i)));
        for i in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

#[test]
fn a_private_repository_is_fetched_with_an_installation_token_that_no_output_shows() {
    let fixture = Fixture::new();
    // The private repository: a bare copy of `core`, laid out for git's dumb HTTP protocol.
    let files = fixture.base.join("stub-git");
    let bare = files.join("o/core.git");
    fs::create_dir_all(bare.parent().unwrap()).unwrap();
    fixture.git(
        &fixture.base,
        &[
            "clone",
            "-q",
            "--bare",
            fixture.upstream("core").to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    fixture.git(&bare, &["update-server-info"]);
    let stub = Stub::start(files);

    let key = fixture.base.join("app-key.pem");
    let generated = Command::new("openssl")
        .args(["genrsa", "-out", key.to_str().unwrap(), "2048"])
        .output()
        .expect("openssl makes the throwaway App key");
    assert!(generated.status.success());
    let id = fixture.base.join("app-id");
    fs::write(&id, "12345\n").unwrap();
    let registry = fixture.base.join("registry.toml");
    fs::write(
        &registry,
        format!(
            "org = \"fixture\"\nfetch_ttl_seconds = {TTL}\n\n[[repo]]\nkey = \"core\"\nclone = \"{}/git/o/core.git\"\nforge = \"github\"\ndefault_branch = \"main\"\nprefixes = [\"VC\"]\n",
            stub.url
        ),
    )
    .unwrap();
    let api = format!("{}/api", stub.url);
    let server = fixture.serve(
        &registry,
        &[
            "--github-app-id-file",
            id.to_str().unwrap(),
            "--github-app-key-file",
            key.to_str().unwrap(),
            "--github-api-url",
            &api,
        ],
    );

    let shown = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(shown["sha"], fixture.head("core", "main").as_str());
    let repos = server.ok("story_repos", json!({}));
    assert_eq!(repos["repositories"][0]["state"], "ready", "{repos}");

    let seen = stub.seen.lock().unwrap().clone();
    // The App authenticates to the API with a signed JWT that names the App and lasts 10 minutes.
    let api_calls: Vec<_> = seen.iter().filter(|s| s.1.starts_with("/api/")).collect();
    assert_eq!(
        api_calls.len(),
        2,
        "one installation lookup, one token: {seen:?}"
    );
    for (_, _, authorization) in &api_calls {
        let jwt = authorization.strip_prefix("Bearer ").expect("a bearer JWT");
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: Value = serde_json::from_slice(&base64url_decode(parts[0])).unwrap();
        assert_eq!(header["alg"], "RS256");
        let claims: Value = serde_json::from_slice(&base64url_decode(parts[1])).unwrap();
        assert_eq!(claims["iss"], "12345");
        assert_eq!(
            claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap(),
            600
        );
    }
    // Every git request carried the installation token, and the repository refuses any other.
    let git_calls: Vec<_> = seen.iter().filter(|s| s.1.starts_with("/git/")).collect();
    assert!(!git_calls.is_empty());
    let basic = format!(
        "basic {}",
        base64(format!("x-access-token:{INSTALLATION_TOKEN}").as_bytes())
    );
    for (_, path, authorization) in &git_calls {
        assert!(authorization.eq_ignore_ascii_case(&basic), "{path}");
    }
    let anonymous = server
        .client
        .get(format!("{}/git/o/core.git/HEAD", stub.url))
        .send()
        .unwrap();
    assert_eq!(anonymous.status(), 401, "the repository is private");

    // No output shows the token: not a tool result, and not the server's log.
    for text in [shown.to_string(), repos.to_string(), server.stderr()] {
        assert!(!text.contains(INSTALLATION_TOKEN), "{text}");
        assert!(!text.contains(&basic[6..]), "{text}");
    }
    // A second read within the TTL asks for no new token.
    server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    let tokens = stub
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.1.ends_with("/access_tokens"))
        .count();
    assert_eq!(tokens, 1);
}
