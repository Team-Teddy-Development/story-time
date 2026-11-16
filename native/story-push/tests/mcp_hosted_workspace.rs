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
    /// The registry with a third repository `teddy` in its own org. It declares the prefix VC,
    /// which `core` of the org `fixture` also declares.
    fn registry_of_two_orgs(&self) -> PathBuf {
        let up = self.upstream("teddy");
        fs::create_dir_all(&up).unwrap();
        self.git(&up, &["init", "-q", "-b", "main"]);
        self.write("teddy", ".story-time/settings.json", &settings("VC"));
        self.write(
            "teddy",
            "backlog/VC-109.md",
            &story("VC-109", "Teddy roster", "depends_on: []\n"),
        );
        // VE-046 is in `web` of the other org only.
        self.write(
            "teddy",
            "backlog/VC-300.md",
            &story("VC-300", "Teddy page", "depends_on: [VE-046]\n"),
        );
        self.commit("teddy", "first");
        self.registry(&format!(
            "\n[[repo]]\nkey = \"teddy\"\norg = \"teddy\"\nclone = \"{}\"\nforge = \"github\"\nprefixes = [\"VC\"]\n",
            self.file_url("teddy")
        ))
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
        self.serve_on(registry, extra, None)
    }
    /// `path` replaces the PATH of the server, so that a test can give it another `git`.
    fn serve_on(&self, registry: &Path, extra: &[&str], path: Option<&std::ffi::OsStr>) -> Http {
        let mut command = self.command();
        if let Some(path) = path {
            command.env("PATH", path);
        }
        let mut child = command
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
        self.call_as(READER, name, arguments)
    }
    fn call_as(&self, token: &str, name: &str, arguments: Value) -> Value {
        self.rpc(
            token,
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

/// ST-147: a `git` that fails the first `fails` fetches as a resolver failure does, and counts
/// each fetch in the file that this returns with the PATH for the server.
fn git_with_dns_failures(
    fixture: &Fixture,
    name: &str,
    fails: u32,
) -> (std::ffi::OsString, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let found = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real = String::from_utf8(found.stdout).unwrap().trim().to_owned();
    let bin = fixture.base.join(name);
    fs::create_dir_all(&bin).unwrap();
    let count = bin.join("fetches");
    let script = format!(
        "#!/bin/sh\ncase \" $* \" in *\" fetch \"*)\n  n=$(cat '{count}' 2>/dev/null || echo 0); n=$((n+1)); echo $n > '{count}'\n  if [ $n -le {fails} ]; then\n    echo \"fatal: unable to access 'https://github.com/o/r.git/': Could not resolve host: github.com\" >&2\n    exit 128\n  fi;;\nesac\nexec '{real}' \"$@\"\n",
        count = count.display()
    );
    fs::write(bin.join("git"), script).unwrap();
    fs::set_permissions(bin.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
    let mut path = bin.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    (path, count)
}

#[test]
fn a_fetch_is_tried_again_when_the_forge_name_does_not_resolve() {
    let fetches = |count: &Path| -> u32 {
        fs::read_to_string(count)
            .map(|text| text.trim().parse().unwrap())
            .unwrap_or(0)
    };
    let one = "org = \"fixture\"\nfetch_ttl_seconds = 600\n\n[[repo]]\nkey = \"core\"\nclone = \"{}\"\nforge = \"github\"\nprefixes = [\"VC\", \"ST\"]\n";

    // Two failures, then the fetch works: the client sees no failure.
    let fixture = Fixture::new();
    let registry = fixture.base.join("one.toml");
    fs::write(&registry, one.replace("{}", &fixture.file_url("core"))).unwrap();
    let (path, count) = git_with_dns_failures(&fixture, "flaky", 2);
    let server = fixture.serve_on(&registry, &[], Some(&path));
    let shown = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(shown["sha"], fixture.head("core", "main").as_str());
    assert_eq!(fetches(&count), 3, "2 failures and the fetch that worked");
    drop(server);

    // The name never resolves: each fetch that the server starts is tried 4 times.
    let fixture = Fixture::new();
    let registry = fixture.base.join("one.toml");
    fs::write(&registry, one.replace("{}", &fixture.file_url("core"))).unwrap();
    let (path, count) = git_with_dns_failures(&fixture, "down", u32::MAX);
    let server = fixture.serve_on(&registry, &[], Some(&path));
    let shown = server.call("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(shown["error"]["code"], "repository_unavailable", "{shown}");
    assert!(
        shown["error"]["details"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Could not resolve host")),
        "{shown}"
    );
    let tried = fetches(&count);
    assert!(
        tried >= 4 && tried % 4 == 0,
        "{tried} tries: 4 for each fetch"
    );
}

/// ST-155 (VM-034): a `git` that fails each fetch with the resolver text while the file
/// `down` exists in its directory, which this returns with the PATH for the server.
fn git_down_while_flagged(fixture: &Fixture) -> (std::ffi::OsString, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let found = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real = String::from_utf8(found.stdout).unwrap().trim().to_owned();
    let bin = fixture.base.join("switch");
    fs::create_dir_all(&bin).unwrap();
    let down = bin.join("down");
    let script = format!(
        "#!/bin/sh\ncase \" $* \" in *\" fetch \"*)\n  if [ -e '{down}' ]; then\n    echo \"fatal: unable to access 'https://github.com/o/r.git/': Could not resolve host: github.com\" >&2\n    exit 128\n  fi;;\nesac\nexec '{real}' \"$@\"\n",
        down = down.display()
    );
    fs::write(bin.join("git"), script).unwrap();
    fs::set_permissions(bin.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
    let mut path = bin.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    (path, down)
}

#[test]
fn a_failed_fetch_is_stale_until_the_background_fetch_works() {
    let fixture = Fixture::new();
    let (path, down) = git_down_while_flagged(&fixture);
    let server = fixture.serve_on(&fixture.registry(""), &[], Some(&path));
    let core = |server: &Http| {
        server.ok("story_repos", json!({}))["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["key"] == "core")
            .unwrap()
            .clone()
    };
    // story_repos does not fetch, so each change below is the work of the background fetch.
    let until = |server: &Http, state: &str| -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(4 * TTL + 60);
        loop {
            let entry = core(server);
            if entry["state"] == state {
                return entry;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "core never became {state}: {entry}"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    };

    let good = until(&server, "ready");
    assert_eq!(good["sha"], fixture.head("core", "main").as_str());
    assert!(good["last_error"].is_null(), "{good}");

    fs::write(&down, "").unwrap();
    let stale = until(&server, "stale");
    assert_eq!(
        stale["sha"], good["sha"],
        "the last good fetch still serves reads"
    );
    assert!(
        stale["last_error"]
            .as_str()
            .is_some_and(|e| e.contains("Could not resolve host")),
        "{stale}"
    );
    let schema = server
        .tools(READER)
        .into_iter()
        .find(|tool| tool["name"] == "story_repos")
        .unwrap();
    let states = &schema["outputSchema"]["properties"]["repositories"]["items"]["properties"]
        ["state"]["enum"];
    assert!(
        states.as_array().unwrap().contains(&json!("stale")),
        "{states}"
    );

    fixture.write(
        "core",
        "backlog/VC-110.md",
        &story("VC-110", "Roster export", "depends_on: []\n"),
    );
    let moved = fixture.commit("core", "second");
    fs::remove_file(&down).unwrap();
    let back = until(&server, "ready");
    assert!(back["last_error"].is_null(), "{back}");
    assert_eq!(back["sha"], moved.as_str());
}

#[test]
fn a_registry_with_an_unknown_forge_plain_http_or_a_repeated_prefix_is_refused_at_start() {
    let fixture = Fixture::new();
    let args = |registry: &Path| fixture.hosted_args(registry);

    let unknown = fixture.registry(
        "\n[[repo]]\nkey = \"ops\"\nclone = \"https://gitea.example/t/ops.git\"\nforge = \"forgejo\"\nprefixes = [\"OP\"]\n",
    );
    let (code, stderr) = fixture.refused(&args(&unknown));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("ops")
            && stderr.contains("forgejo")
            && stderr.contains("github")
            && stderr.contains("gitea"),
        "{stderr}"
    );

    let plain_http = fixture.registry(
        "\n[[repo]]\nkey = \"ops\"\nclone = \"http://192.168.1.56:30003/t/ops.git\"\nforge = \"gitea\"\nprefixes = [\"OP\"]\n",
    );
    let (code, stderr) = fixture.refused(&args(&plain_http));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("ops") && stderr.contains("allow_http"),
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

    // This server has no GitHub App to propose changes with, so it offers no write tool.
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

/// The rule and severity of each lint finding of the story `id`.
fn findings_of(lint: &Value, id: &str) -> Vec<(String, String)> {
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
}

/// ST-141: a third repository `teddy` in its own org declares the prefix VC, which `core` of
/// the org `fixture` also declares.
#[test]
fn two_orgs_keep_their_stories_and_links_apart() {
    let fixture = Fixture::new();
    let registry = fixture.registry_of_two_orgs();
    let server = fixture.serve(&registry, &[]);

    // Each repository has the story of its own org under the shared id.
    let ours = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    let theirs = server.ok("story_show", json!({"repository": "teddy", "id": "VC-109"}));
    assert_eq!(title(&ours), "Roster API");
    assert_eq!(title(&theirs), "Teddy roster");

    let repos = server.ok("story_repos", json!({}));
    let orgs: Vec<(&str, &str)> = repos["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| (repo["key"].as_str().unwrap(), repo["org"].as_str().unwrap()))
        .collect();
    assert_eq!(
        orgs,
        [("core", "fixture"), ("web", "fixture"), ("teddy", "teddy")]
    );

    // VC-109 is in two repositories, one for each org. The link of `web` goes to `core`, the
    // one repository of its org that declares VC, so it is not ambiguous.
    let web = server.ok(
        "story_lint",
        json!({"repository": "web", "ids": ["VE-046"]}),
    );
    let linked = findings_of(&web, "VE-046");
    assert!(
        !linked.iter().any(|f| f.0 == "links.ambiguous")
            && !linked.contains(&("independent.deps".into(), "error".into())),
        "{linked:?}"
    );
    // No repository of the org `teddy` holds VE-046, so that link does not resolve.
    let teddy = server.ok(
        "story_lint",
        json!({"repository": "teddy", "ids": ["VC-300"]}),
    );
    let unlinked = findings_of(&teddy, "VC-300");
    assert!(
        unlinked.contains(&("independent.deps".into(), "error".into())),
        "{unlinked:?}"
    );
}

const SCOPED: &str = "scoped-token-0123456789abcdef";

impl Fixture {
    /// Adds the client `scoped`, which the clients file limits to `orgs`.
    fn limit_a_client(&self, orgs: &str) {
        let path = self.base.join("clients.toml");
        let mut clients = fs::read_to_string(&path).unwrap();
        clients.push_str(&format!(
            "\n[[client]]\nid = \"scoped\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\"]\norgs = {orgs}\n",
            sha256(SCOPED.as_bytes())
        ));
        fs::write(path, clients).unwrap();
    }
}

fn keys(listed: &Value) -> Vec<&str> {
    listed["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["key"].as_str().unwrap())
        .collect()
}

/// ST-142: `core` and `web` are in the org `fixture`, and `teddy` is in the org `teddy`.
#[test]
fn a_client_limited_to_an_org_uses_no_repository_of_another_org() {
    let fixture = Fixture::new();
    let registry = fixture.registry_of_two_orgs();
    fixture.limit_a_client("[\"fixture\"]");
    let server = fixture.serve(&registry, &[]);

    // A repository of another org gets the answer of a repository that the server does not have.
    let unknown = server.call_as(
        SCOPED,
        "story_show",
        json!({"repository": "nope", "id": "VC-109"}),
    );
    assert_eq!(unknown["error"]["code"], "repository_not_allowed");
    for (tool, arguments) in [
        ("story_show", json!({"repository": "teddy", "id": "VC-109"})),
        ("story_list", json!({"repository": "teddy"})),
        ("story_lint", json!({"repository": "teddy"})),
        ("story_config", json!({"repository": "teddy"})),
    ] {
        let refused = server.call_as(SCOPED, tool, arguments);
        assert_eq!(refused["error"], unknown["error"], "{tool}: {refused}");
    }
    let audit = server.stderr();
    let line = audit
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["tool"] == "story_list")
        .unwrap_or_else(|| panic!("no audit line of story_list: {audit}"));
    assert_eq!(line["client_id"], "scoped");
    assert_eq!(line["outcome"], "error");
    assert_eq!(line["error_code"], "repository_not_allowed");

    // Its own org is open to it, and the lists hold that org only.
    let shown = server.call_as(
        SCOPED,
        "story_show",
        json!({"repository": "core", "id": "VC-109"}),
    );
    assert_eq!(title(&shown), "Roster API");
    let repos = server.call_as(SCOPED, "story_repos", json!({}));
    assert_eq!(keys(&repos), ["core", "web"]);
    let config = server.call_as(SCOPED, "story_config", json!({}));
    assert_eq!(keys(&config), ["core", "web"]);

    // A client with no `orgs` key uses each repository.
    let theirs = server.ok("story_show", json!({"repository": "teddy", "id": "VC-109"}));
    assert_eq!(title(&theirs), "Teddy roster");
    assert_eq!(
        keys(&server.ok("story_repos", json!({}))),
        ["core", "web", "teddy"]
    );
    assert_eq!(
        keys(&server.ok("story_config", json!({}))),
        ["core", "web", "teddy"]
    );
}

#[test]
fn a_client_org_that_the_registry_does_not_hold_stops_the_start() {
    let fixture = Fixture::new();
    let registry = fixture.registry_of_two_orgs();
    fixture.limit_a_client("[\"fixture\", \"nope\"]");
    let (code, stderr) = fixture.refused(&fixture.hosted_args(&registry));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("client scoped") && stderr.contains("org nope"),
        "{stderr}"
    );
}

/// A stub of GitHub: the App API under `/api`, and one private repository under `/git` that it
/// serves as static files (git's dumb HTTP protocol) to a request with the installation token.
struct Stub {
    url: String,
    seen: Arc<Mutex<Vec<(String, String, String)>>>,
    address: std::net::SocketAddr,
    stopped: Arc<std::sync::atomic::AtomicBool>,
}
const INSTALLATION_TOKEN: &str = "ghs_StubInstallationToken0123456789";

impl Stub {
    /// One thread serves each connection. `git` opens several connections at almost the same
    /// time, and the thread pool of `tiny_http` 0.12 can leave one of them in its queue until
    /// another connection closes (ST-143). No pool, no queue.
    fn start(files: PathBuf) -> Self {
        Self::start_for(files, INSTALLATION_TOKEN)
    }

    /// The same stub, with a repository that takes the token `token` as the basic password.
    fn start_for(files: PathBuf, token: &str) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("http://{address}");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let basic = format!(
            "basic {}",
            base64(format!("x-access-token:{token}").as_bytes())
        );
        let (log, stop) = (seen.clone(), stopped.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (log, files, basic) = (log.clone(), files.clone(), basic.clone());
                std::thread::spawn(move || {
                    // An I/O error here is the client closing its connection.
                    let _ = Self::serve(stream, &log, &files, &basic);
                });
            }
        });
        Self {
            url,
            seen,
            address,
            stopped,
        }
    }

    /// Answers each request of one connection, until the client closes it.
    fn serve(
        stream: std::net::TcpStream,
        log: &Mutex<Vec<(String, String, String)>>,
        files: &Path,
        basic: &str,
    ) -> std::io::Result<()> {
        use std::io::{Read, Write};
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut stream = stream;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Ok(());
            }
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_owned();
            let target = parts.next().unwrap_or_default();
            let path = target.split('?').next().unwrap().to_owned();
            let (mut authorization, mut length) = (String::new(), 0usize);
            loop {
                let mut header = String::new();
                reader.read_line(&mut header)?;
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                let (name, value) = header.split_once(':').unwrap_or((header, ""));
                if name.eq_ignore_ascii_case("authorization") {
                    authorization = value.trim().to_owned();
                } else if name.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body)?;
            log.lock()
                .unwrap()
                .push((method, path.clone(), authorization.clone()));
            let (status, body): (u16, Vec<u8>) = if path == "/api/repos/o/core/installation" {
                (200, br#"{"id": 7}"#.to_vec())
            } else if path == "/api/app/installations/7/access_tokens" {
                let expires = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
                (
                    201,
                    json!({"token": INSTALLATION_TOKEN, "expires_at": expires})
                        .to_string()
                        .into_bytes(),
                )
            } else if let Some(relative) = path.strip_prefix("/git/") {
                if !authorization.eq_ignore_ascii_case(basic) {
                    (401, Vec::new())
                } else {
                    match fs::read(files.join(relative)) {
                        Ok(bytes) if !relative.contains("..") => (200, bytes),
                        _ => (404, Vec::new()),
                    }
                }
            } else {
                (404, Vec::new())
            };
            write!(
                stream,
                "HTTP/1.1 {status} Stub\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )?;
            stream.write_all(&body)?;
            stream.flush()?;
        }
    }
}
impl Drop for Stub {
    fn drop(&mut self) {
        // The accept loop reads the flag when one more connection arrives.
        self.stopped.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(self.address);
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
    // The fixture has the branch `story-time/VC-109`, so the read also asks the forge for the
    // open pull request of that branch, with the installation token.
    let (pulls, api_calls): (Vec<_>, Vec<_>) = seen
        .iter()
        .filter(|s| s.1.starts_with("/api/"))
        .partition(|s| s.1.ends_with("/pulls"));
    assert_eq!(pulls.len(), 1, "{seen:?}");
    assert_eq!(pulls[0].2, format!("Bearer {INSTALLATION_TOKEN}"));
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

/// A registry of one repository `core` on the Gitea clone URL `clone`.
fn gitea_registry(fixture: &Fixture, clone: &str) -> PathBuf {
    let registry = fixture.base.join("registry.toml");
    fs::write(
        &registry,
        format!(
            "org = \"fixture\"\nfetch_ttl_seconds = {TTL}\n\n[[repo]]\nkey = \"core\"\nclone = \"{clone}\"\nforge = \"gitea\"\ndefault_branch = \"main\"\nprefixes = [\"VC\", \"ST\"]\n"
        ),
    )
    .unwrap();
    registry
}

#[test]
fn a_gitea_repository_is_read_like_a_github_repository() {
    let fixture = Fixture::new();
    let registry = gitea_registry(&fixture, &fixture.file_url("core"));
    let server = fixture.serve(&registry, &[]);
    let head = fixture.head("core", "main");

    let listed = server.ok("story_list", json!({"repository": "core"}));
    let repos = server.ok("story_repos", json!({}));
    let entry = &repos["repositories"][0];
    assert_eq!(entry["key"], "core", "{repos}");
    assert_eq!(entry["state"], "ready", "{repos}");
    assert_eq!(entry["forge"], "gitea", "{repos}");
    assert_eq!(entry["sha"], head.as_str(), "{repos}");
    assert!(entry["fetched_at"].is_string(), "{repos}");

    let ids: Vec<&str> = listed["stories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|story| story["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["ST-001", "VC-109"], "{listed}");
    let shown = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(title(&shown), "Roster API");
    assert_eq!(shown["sha"], head.as_str());
    let linted = server.ok("story_lint", json!({"repository": "core"}));
    assert_eq!(linted["sha"], head.as_str(), "{linted}");
    let config = server.ok("story_config", json!({}));
    assert_eq!(config["repositories"][0]["status"], "ok", "{config}");
    assert_eq!(config["repositories"][0]["prefix"], "VC", "{config}");

    // A GitHub entry keeps its forge in the same report.
    let mixed = fixture.registry("");
    let server = fixture.serve(&mixed, &[]);
    server.ok("story_list", json!({"repository": "core"}));
    let repos = server.ok("story_repos", json!({}));
    for entry in repos["repositories"].as_array().unwrap() {
        assert_eq!(entry["forge"], "github", "{repos}");
    }
}

const GITEA_TOKEN: &str = "gta_StubGiteaBotToken0123456789abcdef";

fn private_gitea(fixture: &Fixture) -> (Stub, PathBuf) {
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
    let stub = Stub::start_for(files, GITEA_TOKEN);
    let registry = gitea_registry(fixture, &format!("{}/git/o/core.git", stub.url));
    (stub, registry)
}

#[test]
fn a_private_gitea_repository_is_fetched_with_the_bot_token_that_no_output_shows() {
    let fixture = Fixture::new();
    let (stub, registry) = private_gitea(&fixture);
    let token_file = fixture.base.join("gitea-token");
    fs::write(&token_file, format!("{GITEA_TOKEN}\n")).unwrap();
    let server = fixture.serve(
        &registry,
        &["--gitea-token-file", token_file.to_str().unwrap()],
    );

    let shown = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert_eq!(shown["sha"], fixture.head("core", "main").as_str());
    let repos = server.ok("story_repos", json!({}));
    assert_eq!(repos["repositories"][0]["state"], "ready", "{repos}");
    assert_eq!(repos["repositories"][0]["forge"], "gitea", "{repos}");

    let basic = base64(format!("x-access-token:{GITEA_TOKEN}").as_bytes());
    let seen = stub.seen.lock().unwrap().clone();
    assert!(
        seen.iter().all(|call| !call.1.starts_with("/api/")),
        "no forge API request: {seen:?}"
    );
    let git_calls: Vec<_> = seen.iter().filter(|s| s.1.starts_with("/git/")).collect();
    assert!(!git_calls.is_empty());
    for (_, path, authorization) in &git_calls {
        assert!(
            authorization.eq_ignore_ascii_case(&format!("basic {basic}")),
            "{path}"
        );
    }
    for text in [shown.to_string(), repos.to_string(), server.stderr()] {
        assert!(!text.contains(GITEA_TOKEN), "{text}");
        assert!(!text.contains(&basic), "{text}");
    }

    // A token inside a story file is redacted from the result as well.
    fixture.write(
        "core",
        "backlog/VC-109.md",
        &story(
            "VC-109",
            &format!("Leaks {GITEA_TOKEN}"),
            "depends_on: []\n",
        ),
    );
    fixture.commit("core", "leak");
    let bare = fixture.base.join("stub-git/o/core.git");
    fixture.git(
        &bare,
        &[
            "fetch",
            "-q",
            fixture.upstream("core").to_str().unwrap(),
            "main:main",
        ],
    );
    fixture.git(&bare, &["update-server-info"]);
    std::thread::sleep(Duration::from_millis(TTL * 1000 + 300));
    let leaked = server.ok("story_show", json!({"repository": "core", "id": "VC-109"}));
    assert!(title(&leaked).contains("Leaks"), "{leaked}");
    assert!(!leaked.to_string().contains(GITEA_TOKEN), "{leaked}");
    assert!(leaked["redactions"].as_u64().unwrap() >= 1, "{leaked}");
}

#[test]
fn a_private_gitea_repository_is_unavailable_to_a_server_without_the_token() {
    let fixture = Fixture::new();
    let (stub, registry) = private_gitea(&fixture);
    let server = fixture.serve(&registry, &[]);
    let refused = server.call("story_show", json!({"repository": "core", "id": "VC-109"}));
    let repos = server.ok("story_repos", json!({}));
    assert_eq!(repos["repositories"][0]["state"], "unavailable", "{repos}");
    assert_eq!(repos["repositories"][0]["forge"], "gitea", "{repos}");
    assert_eq!(
        refused["error"]["code"], "repository_unavailable",
        "{refused}"
    );
    let basic = base64(format!("x-access-token:{GITEA_TOKEN}").as_bytes());
    for text in [repos.to_string(), refused.to_string(), server.stderr()] {
        assert!(!text.contains(GITEA_TOKEN), "{text}");
        assert!(!text.contains(&basic), "{text}");
    }
    assert!(stub
        .seen
        .lock()
        .unwrap()
        .iter()
        .all(|call| call.2.is_empty() || !call.1.starts_with("/api/")));
}

#[test]
fn writes_and_comments_on_a_gitea_repository_are_refused_before_any_forge_call() {
    let fixture = Fixture::new();
    // `core` is the Gitea repository and has the branch `story-time/VC-109`. `web` is GitHub.
    let registry = fixture.registry("");
    let text = fs::read_to_string(&registry).unwrap().replacen(
        "forge = \"github\"",
        "forge = \"gitea\"",
        1,
    );
    fs::write(&registry, text).unwrap();
    fs::write(
        fixture.base.join("clients.toml"),
        format!(
            "[[client]]\nid = \"writer\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\", \"write\", \"comment\"]\n",
            sha256(WRITER.as_bytes())
        ),
    )
    .unwrap();
    let stub = Stub::start(fixture.base.join("stub-git"));
    let key = fixture.base.join("app-key.pem");
    let generated = Command::new("openssl")
        .args(["genrsa", "-out", key.to_str().unwrap(), "2048"])
        .output()
        .expect("openssl makes the throwaway App key");
    assert!(generated.status.success());
    let id = fixture.base.join("app-id");
    fs::write(&id, "12345\n").unwrap();
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
    let names: Vec<String> = server
        .tools(WRITER)
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"story_update".to_owned()), "{names:?}");
    let branches = fixture.git(&fixture.upstream("core"), &["branch", "--list"]);

    let shown = server.call_as(
        WRITER,
        "story_show",
        json!({"repository": "core", "id": "VC-109"}),
    );
    assert!(shown["proposed"].is_null(), "{shown}");
    let calls = [
        (
            "story_update",
            json!({"repository": "core", "id": "VC-109", "expected_sha256": shown["sha256"], "set": {"title": "Renamed"}}),
        ),
        (
            "story_note",
            json!({"repository": "core", "id": "VC-109", "expected_sha256": shown["sha256"], "note": "A note"}),
        ),
        (
            "story_create",
            json!({"repository": "core", "title": "A new story"}),
        ),
        (
            "story_comments",
            json!({"repository": "core", "id": "VC-109"}),
        ),
        (
            "story_resolve_comment",
            json!({"repository": "core", "id": "VC-109", "comment_id": 1, "note": "Done"}),
        ),
    ];
    for (name, arguments) in calls {
        let refused = server.call_as(WRITER, name, arguments);
        assert_eq!(
            refused["error"]["code"], "forge_not_supported",
            "{name}: {refused}"
        );
        assert_eq!(refused["error"]["retryable"], false, "{name}: {refused}");
        let message = refused["error"]["message"].as_str().unwrap();
        assert!(
            message.contains("Gitea") && message.contains("not supported"),
            "{message}"
        );
    }
    assert!(
        stub.seen.lock().unwrap().is_empty(),
        "no request reached the forge: {:?}",
        stub.seen.lock().unwrap()
    );
    assert_eq!(
        fixture.git(&fixture.upstream("core"), &["branch", "--list"]),
        branches
    );
    assert_eq!(
        fixture.head("core", "story-time/VC-109"),
        fixture.git(
            &fixture.upstream("core"),
            &["rev-parse", "story-time/VC-109"]
        )
    );
}

#[test]
fn a_gitea_token_file_that_is_missing_empty_or_without_a_registry_stops_the_start() {
    let fixture = Fixture::new();
    let registry = gitea_registry(&fixture, &fixture.file_url("core"));
    let base = fixture.hosted_args(&registry);
    let with = |path: &Path| {
        let mut args = base.clone();
        args.extend(["--gitea-token-file".to_owned(), path.display().to_string()]);
        args
    };

    let (code, stderr) = fixture.refused(&with(&fixture.base.join("absent")));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--gitea-token-file"), "{stderr}");

    let empty = fixture.base.join("empty-token");
    fs::write(&empty, " \n").unwrap();
    let (code, stderr) = fixture.refused(&with(&empty));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--gitea-token-file"), "{stderr}");

    let spaced = fixture.base.join("spaced-token");
    fs::write(&spaced, "gta_two words\n").unwrap();
    let (code, stderr) = fixture.refused(&with(&spaced));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("--gitea-token-file") && !stderr.contains("gta_two"),
        "{stderr}"
    );

    let fine = fixture.base.join("fine-token");
    fs::write(&fine, "gta_fine\n").unwrap();
    let (code, stderr) = fixture.refused(&[
        "--http".to_owned(),
        "127.0.0.1:0".to_owned(),
        "--gitea-token-file".to_owned(),
        fine.display().to_string(),
    ]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("--gitea-token-file") && stderr.contains("--registry"),
        "{stderr}"
    );
}
