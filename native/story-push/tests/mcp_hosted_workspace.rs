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

/// ST-156 (VM-034): a `git` that fails each fetch with the resolver text while the file
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
fn a_failed_fetch_shows_its_error_beside_ready_until_the_background_fetch_works() {
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
    let until = |server: &Http, what: &str, done: &dyn Fn(&Value) -> bool| -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(4 * TTL + 60);
        loop {
            let entry = core(server);
            if done(&entry) {
                return entry;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "core never {what}: {entry}"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    };

    let good = until(&server, "became ready", &|e| e["state"] == "ready");
    assert_eq!(good["sha"], fixture.head("core", "main").as_str());
    assert!(good["last_error"].is_null(), "{good}");

    fs::write(&down, "").unwrap();
    let failed = until(&server, "showed a failed fetch", &|e| {
        !e["last_error"].is_null()
    });
    assert_eq!(failed["state"], "ready", "the state enum does not change");
    assert_eq!(
        failed["sha"], good["sha"],
        "the last good fetch still serves reads"
    );
    assert!(
        failed["last_error"]
            .as_str()
            .is_some_and(|e| e.contains("Could not resolve host")),
        "{failed}"
    );

    fixture.write(
        "core",
        "backlog/VC-110.md",
        &story("VC-110", "Roster export", "depends_on: []\n"),
    );
    let moved = fixture.commit("core", "second");
    fs::remove_file(&down).unwrap();
    let back = until(&server, "fetched the new commit", &|e| {
        e["sha"] == moved.as_str()
    });
    assert_eq!(back["state"], "ready");
    assert!(back["last_error"].is_null(), "{back}");
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
    gitea: Arc<Mutex<GiteaComments>>,
}

#[derive(Default)]
struct GiteaComments {
    items: Vec<Value>,
    redirect: Option<String>,
    drop_post_response: bool,
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
        let gitea = Arc::new(Mutex::new(GiteaComments::default()));
        let (log, stop, state) = (seen.clone(), stopped.clone(), gitea.clone());
        let auth = format!("token {token}");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (log, files, basic, state, auth) = (
                    log.clone(),
                    files.clone(),
                    basic.clone(),
                    state.clone(),
                    auth.clone(),
                );
                std::thread::spawn(move || {
                    // An I/O error here is the client closing its connection.
                    let _ = Self::serve(stream, &log, &files, &basic, &state, &auth);
                });
            }
        });
        Self {
            url,
            seen,
            address,
            stopped,
            gitea,
        }
    }

    /// Answers each request of one connection, until the client closes it.
    fn serve(
        stream: std::net::TcpStream,
        log: &Mutex<Vec<(String, String, String)>>,
        files: &Path,
        basic: &str,
        gitea: &Mutex<GiteaComments>,
        token_auth: &str,
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
                .push((method.clone(), path.clone(), authorization.clone()));
            let (status, body): (u16, Vec<u8>) = if path.starts_with("/git/api/v1/") {
                let mut gitea = gitea.lock().unwrap();
                if authorization != token_auth {
                    (401, Vec::new())
                } else if let Some(location) = &gitea.redirect {
                    write!(
                        stream,
                        "HTTP/1.1 307 Stub\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n"
                    )?;
                    stream.flush()?;
                    continue;
                } else if path == "/git/api/v1/user" && method == "GET" {
                    (200, br#"{"id":7,"login":"story-bot"}"#.to_vec())
                } else if path == GITEA_ISSUE && method == "GET" {
                    let page = target
                        .split_once('?')
                        .unwrap()
                        .1
                        .split('&')
                        .find_map(|field| field.strip_prefix("page="))
                        .and_then(|value| value.parse::<usize>().ok())
                        .filter(|page| *page > 0)
                        .unwrap();
                    let items: Vec<_> = gitea.items.iter().skip(page - 1).take(1).collect();
                    (200, serde_json::to_vec(&items).unwrap())
                } else if path == GITEA_ISSUE && method == "POST" {
                    let payload: Value = serde_json::from_slice(&body).unwrap();
                    let item = gitea_comment(
                        200 + gitea.items.len() as u64,
                        7,
                        "story-bot",
                        &payload["body"],
                    );
                    gitea.items.push(item.clone());
                    if gitea.drop_post_response {
                        return Ok(());
                    }
                    (201, serde_json::to_vec(&item).unwrap())
                } else {
                    (404, Vec::new())
                }
            } else if path == "/api/repos/o/core/installation" {
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
const GITEA_ISSUE: &str = "/git/api/v1/repos/o/core/issues/12/comments";
const COMMENT_TIME: &str = "2026-11-10T09:00:00Z";

fn gitea_comment(id: u64, user: u64, login: &str, body: &Value) -> Value {
    json!({"id": id, "user": {"id": user, "login": login}, "body": body,
           "created_at": COMMENT_TIME, "updated_at": COMMENT_TIME})
}

fn private_gitea(fixture: &Fixture) -> (Stub, PathBuf) {
    private_gitea_with(fixture, |_| {})
}

fn private_gitea_with(fixture: &Fixture, setup: impl FnOnce(&str)) -> (Stub, PathBuf) {
    let files = fixture.base.join("stub-git");
    let bare = files.join("o/core.git");
    fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let stub = Stub::start_for(files, GITEA_TOKEN);
    setup(&stub.url);
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
    let registry = gitea_registry(fixture, &format!("{}/git/o/core.git", stub.url));
    (stub, registry)
}

fn issue_story(fixture: &Fixture, id: &str, url: &str) {
    fixture.write(
        "core",
        &format!("backlog/{id}.md"),
        &story(
            id,
            "Roster API",
            &format!("remote: 12\nremote_url: \"{url}\"\n"),
        ),
    );
}

fn comment_server(fixture: &Fixture, registry: &Path) -> Http {
    fs::write(
        fixture.base.join("clients.toml"),
        format!(
            "[[client]]\nid = \"reader\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\"]\n\n[[client]]\nid = \"writer\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\", \"comment\"]\n",
            sha256(READER.as_bytes()), sha256(WRITER.as_bytes())
        ),
    ).unwrap();
    let token = fixture.base.join("gitea-token");
    fs::write(&token, GITEA_TOKEN).unwrap();
    fixture.serve(
        registry,
        &[
            "--gitea-token-file",
            token.to_str().unwrap(),
            "--workers",
            "4",
        ],
    )
}

#[test]
fn gitea_comments_list_and_resolve_once_under_the_deployment_prefix() {
    let fixture = Fixture::new();
    let (stub, registry) = private_gitea_with(&fixture, |origin| {
        issue_story(
            &fixture,
            "VC-109",
            &format!("{origin}/git/o/core/issues/12"),
        );
        fixture.commit("core", "Gitea issue");
    });
    stub.gitea.lock().unwrap().items = vec![
        gitea_comment(101, 4, "maria", &json!("Please cover missing players.")),
        gitea_comment(102, 8, "story-bot", &json!(format!("Resolved: copied\n\n<!-- story-time:v1:resolves=gitea:101:{COMMENT_TIME} -->"))),
        gitea_comment(103, 7, "story-bot", &json!(format!("Resolved: wrong forge\n\n<!-- story-time:v1:resolves=github:101:{COMMENT_TIME} -->"))),
    ];
    let server = comment_server(&fixture, &registry);
    let args = json!({"repository": "core", "id": "VC-109"});
    let issue = format!("{}/git/o/core/issues/12", stub.url);
    let list = || server.ok("story_comments", args.clone());
    let first = list();
    assert_eq!(first["issue_url"], issue);
    assert_eq!(first["comments"].as_array().unwrap().len(), 3);
    assert_eq!(
        first["comments"][0],
        json!({
            "id": 101, "url": format!("{issue}#issuecomment-101"),
            "author": "maria", "author_association": "UNKNOWN",
            "created_at": COMMENT_TIME, "updated_at": COMMENT_TIME,
            "body": "Please cover missing players.", "resolved": null,
        })
    );
    let resolve = json!({"repository": "core", "id": "VC-109", "comment_id": 101, "note": "Added the missing case"});
    let refused = server.call("story_resolve_comment", resolve.clone());
    assert_eq!(refused["error"]["code"], "scope_refused", "{refused}");
    assert_eq!(stub.gitea.lock().unwrap().items.len(), 3);
    let send = || server.call_as(WRITER, "story_resolve_comment", resolve.clone());
    let mut replies = std::thread::scope(|scope| {
        let first = scope.spawn(send);
        let second = scope.spawn(send);
        [first.join().unwrap(), second.join().unwrap()]
    });
    replies.sort_by_key(|reply| reply.get("error").is_some());
    assert_eq!(
        replies[0]["reply_url"],
        format!("{issue}#issuecomment-203"),
        "{replies:?}"
    );
    assert_eq!(
        replies[1]["error"]["code"], "already_resolved",
        "{replies:?}"
    );
    assert_eq!(send()["error"]["code"], "already_resolved");
    assert_eq!(
        list()["comments"][0]["resolved"],
        json!({"by": "story-bot", "at": COMMENT_TIME, "note": "Added the missing case"})
    );
    let state = stub.gitea.lock().unwrap();
    assert_eq!(state.items.len(), 4);
    assert_eq!(state.items[3]["body"], format!("Resolved: Added the missing case\n\nIn reply to {issue}#issuecomment-101\nRequested-by: writer\n\n<!-- story-time:v1:resolves=gitea:101:{COMMENT_TIME} -->"));
    assert_eq!(
        stub.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(method, path, _)| method == "POST" && path == GITEA_ISSUE)
            .count(),
        1
    );
    assert!(stub
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, path, _)| path.starts_with("/git/api/v1/"))
        .all(|(_, _, auth)| auth == &format!("token {GITEA_TOKEN}")));
    assert!(!first.to_string().contains(GITEA_TOKEN));
    assert!(!server.stderr().contains(GITEA_TOKEN));
}

#[test]
fn gitea_comments_refuse_mismatched_issue_targets_before_api_auth() {
    let fixture = Fixture::new();
    let foreign = Stub::start(fixture.base.join("foreign"));
    let (stub, registry) = private_gitea_with(&fixture, |origin| {
        for (id, url) in [
            ("VC-110", format!("{}/git/o/core/issues/12", foreign.url)),
            ("VC-111", format!("{origin}/git/o/other/issues/12")),
            ("VC-112", format!("{origin}/git/o/core/issues/0")),
            (
                "VC-113",
                format!(
                    "{}/git/o/core/issues/12",
                    origin.replacen("http:", "https:", 1)
                ),
            ),
            (
                "VC-114",
                format!(
                    "{}/git/o/core/issues/12",
                    origin.replace("127.0.0.1", "localhost")
                ),
            ),
            ("VC-115", format!("{origin}/o/core/issues/12")),
            ("VC-116", format!("{origin}/git-extra/o/core/issues/12")),
        ] {
            issue_story(&fixture, id, &url);
        }
        fixture.commit("core", "Invalid Gitea issue targets");
    });
    let server = comment_server(&fixture, &registry);
    for id in [
        "VC-110", "VC-111", "VC-112", "VC-113", "VC-114", "VC-115", "VC-116",
    ] {
        for (tool, args) in [
            ("story_comments", json!({"id": id})),
            (
                "story_resolve_comment",
                json!({"id": id, "comment_id": 101, "note": "Done"}),
            ),
        ] {
            let result = server.call_as(WRITER, tool, args);
            assert_eq!(
                result["error"]["code"], "no_tracker_issue",
                "{tool} {id}: {result}"
            );
        }
    }
    assert!(foreign.seen.lock().unwrap().is_empty());
    assert!(stub
        .seen
        .lock()
        .unwrap()
        .iter()
        .all(|(_, path, _)| !path.contains("/api/v1/")));
}

#[test]
fn gitea_comments_do_not_follow_redirects_or_retry_uncertain_posts() {
    let fixture = Fixture::new();
    let foreign = Stub::start(fixture.base.join("foreign"));
    let (stub, registry) = private_gitea_with(&fixture, |origin| {
        issue_story(
            &fixture,
            "VC-109",
            &format!("{origin}/git/o/core/issues/12"),
        );
        fixture.commit("core", "Gitea issue");
    });
    let server = comment_server(&fixture, &registry);
    stub.gitea.lock().unwrap().redirect = Some(format!("{}/api/v1/user", foreign.url));
    let refused = server.call("story_comments", json!({"id": "VC-109"}));
    assert_eq!(refused["error"]["code"], "forge_unavailable", "{refused}");
    assert!(foreign.seen.lock().unwrap().is_empty());
    {
        let mut state = stub.gitea.lock().unwrap();
        state.redirect = None;
        state.drop_post_response = true;
        state.items = vec![gitea_comment(
            101,
            4,
            "maria",
            &json!("Please cover missing players."),
        )];
    }
    let args = json!({"id": "VC-109", "comment_id": 101, "note": "Added the case"});
    let failed = server.call_as(WRITER, "story_resolve_comment", args.clone());
    assert_eq!(failed["error"]["code"], "reply_not_confirmed", "{failed}");
    assert_eq!(failed["error"]["retryable"], false, "{failed}");
    let again = server.call_as(WRITER, "story_resolve_comment", args);
    assert_eq!(again["error"]["code"], "already_resolved", "{again}");
    assert_eq!(
        stub.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, p, _)| m == "POST" && p == GITEA_ISSUE)
            .count(),
        1
    );
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
        seen.iter().all(|call| !call.1.contains("/api/")),
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
        .all(|call| call.2.is_empty() || !call.1.contains("/api/")));
}

#[test]
fn gitea_writes_without_a_gitea_token_are_refused_before_any_forge_call() {
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
    ];
    for (name, arguments) in calls {
        let refused = server.call_as(WRITER, name, arguments);
        assert_eq!(
            refused["error"]["code"], "forge_not_supported",
            "{name}: {refused}"
        );
        assert_eq!(refused["error"]["retryable"], false, "{name}: {refused}");
        let message = refused["error"]["message"].as_str().unwrap();
        // ST-149: a write needs the Gitea bot token, which this server lacks.
        assert!(message.contains("no Gitea bot token"), "{name}: {message}");
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

/// One pull request of the Gitea stub.
#[derive(Clone)]
struct GiteaPull {
    number: u64,
    head: String,
    head_repo: String,
    base: String,
    title: String,
    body: String,
}

/// What the Gitea stub holds and saw.
struct GiteaState {
    pulls: Vec<GiteaPull>,
    /// The method, path, query, and `Authorization` header of each REST request.
    rest: Vec<(String, String, String, String)>,
    /// The token that the REST API and git take. Each has its own, so that a test can refuse one.
    rest_token: String,
    git_token: String,
    /// `git-receive-pack` answers 403.
    refuse_push: bool,
}

/// The pull requests a list page holds at most. Gitea 1.21 caps `limit` with a setting, so the
/// stub caps it below what the client asks.
const GITEA_PAGE_CAP: usize = 2;

/// A stub of Gitea 1.21.11 for the repository `o/core`: the REST API under `/api/v1` (a pull
/// request list with no `head` filter) and its bare repository under `/git`, served by
/// `git http-backend` to a request with the bot token as the basic password.
struct GiteaStub {
    url: String,
    state: Arc<Mutex<GiteaState>>,
    address: std::net::SocketAddr,
    stopped: Arc<std::sync::atomic::AtomicBool>,
}

impl GiteaStub {
    /// `files` holds `o/core.git`. Four pull requests are open at the start: three from
    /// branches that are not story branches, and one that is from a fork of `o/core` and has
    /// the head name `story-time/ST-001`.
    fn start(files: PathBuf, token: &str) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("http://{address}");
        let seeded = |number: u64, head: &str, repo: &str| GiteaPull {
            number,
            head: head.to_owned(),
            head_repo: repo.to_owned(),
            base: "main".into(),
            title: format!("Seeded {number}"),
            body: String::new(),
        };
        let state = Arc::new(Mutex::new(GiteaState {
            pulls: vec![
                seeded(1, "feature/a", "o/core"),
                seeded(2, "story-time/ST-001", "mallory/core"),
                seeded(3, "feature/b", "o/core"),
                seeded(4, "feature/c", "o/core"),
            ],
            rest: Vec::new(),
            rest_token: token.to_owned(),
            git_token: token.to_owned(),
            refuse_push: false,
        }));
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (shared, stop, base) = (state.clone(), stopped.clone(), url.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (shared, files, base) = (shared.clone(), files.clone(), base.clone());
                std::thread::spawn(move || {
                    // An I/O error here is the client closing its connection.
                    let _ = Self::serve(stream, &shared, &files, &base);
                });
            }
        });
        Self {
            url,
            state,
            address,
            stopped,
        }
    }

    fn pulls(&self) -> Vec<GiteaPull> {
        self.state.lock().unwrap().pulls.clone()
    }

    fn posts(&self) -> usize {
        let state = self.state.lock().unwrap();
        state.rest.iter().filter(|call| call.0 == "POST").count()
    }

    fn serve(
        stream: std::net::TcpStream,
        state: &Mutex<GiteaState>,
        files: &Path,
        base: &str,
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
            let target = parts.next().unwrap_or_default().to_owned();
            let (path, query) = target.split_once('?').unwrap_or((&target, ""));
            let (path, query) = (path.to_owned(), query.to_owned());
            let (mut authorization, mut length) = (String::new(), 0usize);
            let (mut content_type, mut encoding) = (String::new(), String::new());
            loop {
                let mut header = String::new();
                reader.read_line(&mut header)?;
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                let (name, value) = header.split_once(':').unwrap_or((header, ""));
                let value = value.trim().to_owned();
                match name.to_ascii_lowercase().as_str() {
                    "authorization" => authorization = value,
                    "content-length" => length = value.parse().unwrap_or(0),
                    "content-type" => content_type = value,
                    "content-encoding" => encoding = value,
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body)?;
            let (status, content_type, answer) = if let Some(rest) = path.strip_prefix("/api/v1/") {
                let (status, answer) =
                    Self::rest(state, base, &method, rest, &query, &authorization, &body);
                (status, "application/json".to_owned(), answer)
            } else if let Some(info) = path.strip_prefix("/git") {
                Self::git(
                    state,
                    files,
                    (&method, info, &query),
                    (&authorization, &content_type, &encoding),
                    &body,
                )
            } else {
                (404, "text/plain".into(), Vec::new())
            };
            write!(
                stream,
                "HTTP/1.1 {status} Stub\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n",
                answer.len()
            )?;
            stream.write_all(&answer)?;
            stream.flush()?;
        }
    }

    /// The pull request routes of Gitea 1.21: a list with `state`, `page`, and `limit` only,
    /// and a create that refuses a second open pull request from the same head.
    fn rest(
        state: &Mutex<GiteaState>,
        base: &str,
        method: &str,
        path: &str,
        query: &str,
        authorization: &str,
        body: &[u8],
    ) -> (u16, Vec<u8>) {
        let mut state = state.lock().unwrap();
        state.rest.push((
            method.to_owned(),
            path.to_owned(),
            query.to_owned(),
            authorization.to_owned(),
        ));
        let reply = |status: u16, value: Value| (status, value.to_string().into_bytes());
        if path != "repos/o/core/pulls" {
            return reply(404, json!({"message": "not found"}));
        }
        if authorization != format!("token {}", state.rest_token) {
            return reply(401, json!({"message": "token is required"}));
        }
        let shown = |pull: &GiteaPull| {
            json!({
                "number": pull.number,
                "html_url": format!("{base}/o/core/pulls/{}", pull.number),
                "state": "open",
                "head": {"ref": pull.head, "repo": {"full_name": pull.head_repo}},
                "base": {"ref": pull.base},
            })
        };
        match method {
            "GET" => {
                let value = |name: &str| {
                    query
                        .split('&')
                        .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
                        .and_then(|text| text.parse::<usize>().ok())
                };
                let page = value("page").unwrap_or(1).max(1);
                let limit = value("limit").unwrap_or(30).clamp(1, GITEA_PAGE_CAP);
                let listed: Vec<Value> = state
                    .pulls
                    .iter()
                    .skip((page - 1) * limit)
                    .take(limit)
                    .map(shown)
                    .collect();
                reply(200, json!(listed))
            }
            "POST" => {
                let sent: Value = serde_json::from_slice(body).unwrap_or_default();
                let text = |key: &str| sent[key].as_str().unwrap_or_default().to_owned();
                let head = text("head");
                if state
                    .pulls
                    .iter()
                    .any(|pull| pull.head == head && pull.head_repo == "o/core")
                {
                    return reply(409, json!({"message": "pull request already exists"}));
                }
                let pull = GiteaPull {
                    number: state.pulls.len() as u64 + 1,
                    head,
                    head_repo: "o/core".into(),
                    base: text("base"),
                    title: text("title"),
                    body: text("body"),
                };
                let answer = shown(&pull);
                state.pulls.push(pull);
                reply(201, answer)
            }
            _ => reply(405, json!({"message": "method not allowed"})),
        }
    }

    /// Runs `git http-backend` for one request, and returns its status, content type, and body.
    fn git(
        state: &Mutex<GiteaState>,
        files: &Path,
        (method, info, query): (&str, &str, &str),
        (authorization, content_type, encoding): (&str, &str, &str),
        body: &[u8],
    ) -> (u16, String, Vec<u8>) {
        use std::io::{Read, Write};
        let (git_token, refuse_push) = {
            let state = state.lock().unwrap();
            (state.git_token.clone(), state.refuse_push)
        };
        let basic = format!(
            "basic {}",
            base64(format!("x-access-token:{git_token}").as_bytes())
        );
        if !authorization.eq_ignore_ascii_case(&basic) {
            return (401, "text/plain".into(), Vec::new());
        }
        if refuse_push && (query.contains("git-receive-pack") || info.ends_with("git-receive-pack"))
        {
            return (403, "text/plain".into(), Vec::new());
        }
        let mut child = Command::new("git")
            .arg("http-backend")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_PROJECT_ROOT", files)
            .env("GIT_HTTP_EXPORT_ALL", "1")
            .env("REMOTE_USER", "bot")
            .env("REMOTE_ADDR", "127.0.0.1")
            .env("REQUEST_METHOD", method)
            .env("PATH_INFO", info)
            .env("QUERY_STRING", query)
            .env("CONTENT_TYPE", content_type)
            .env("CONTENT_LENGTH", body.len().to_string())
            .env("HTTP_CONTENT_ENCODING", encoding)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let input = body.to_vec();
        let feeder = std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
        let mut out = Vec::new();
        child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
        let _ = feeder.join();
        let _ = child.wait();
        let split = out
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("http-backend ends its headers");
        let headers = String::from_utf8_lossy(&out[..split]).into_owned();
        let (mut status, mut kind) = (200, String::from("application/octet-stream"));
        for header in headers.lines() {
            let (name, value) = header.split_once(':').unwrap_or((header, ""));
            match name.to_ascii_lowercase().as_str() {
                "status" => status = value.trim().split(' ').next().unwrap().parse().unwrap(),
                "content-type" => kind = value.trim().to_owned(),
                _ => {}
            }
        }
        (status, kind, out[split + 4..].to_vec())
    }
}
impl Drop for GiteaStub {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(self.address);
    }
}

/// A Gitea stub that serves a bare copy of `core`, the registry for it, and a token file.
fn writable_gitea(fixture: &Fixture) -> (GiteaStub, PathBuf, PathBuf) {
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
    let stub = GiteaStub::start(files, GITEA_TOKEN);
    let registry = gitea_registry(fixture, &format!("{}/git/o/core.git", stub.url));
    let token_file = fixture.base.join("gitea-token");
    fs::write(&token_file, format!("{GITEA_TOKEN}\n")).unwrap();
    (stub, registry, token_file)
}

fn serve_gitea(fixture: &Fixture, registry: &Path, token_file: &Path) -> Http {
    fixture.serve(
        registry,
        &["--gitea-token-file", token_file.to_str().unwrap()],
    )
}

fn bare_of(fixture: &Fixture) -> PathBuf {
    fixture.base.join("stub-git/o/core.git")
}

#[test]
fn a_story_update_on_a_gitea_repository_pushes_one_commit_and_opens_one_pull_request() {
    let fixture = Fixture::new();
    let (stub, registry, token_file) = writable_gitea(&fixture);
    let server = serve_gitea(&fixture, &registry, &token_file);
    let bare = bare_of(&fixture);
    let main = fixture.git(&bare, &["rev-parse", "main"]);

    // A server with a Gitea token and no GitHub App has the write tools.
    let names: Vec<String> = server
        .tools(WRITER)
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"story_update".to_owned()), "{names:?}");

    let shown = server.call_as(
        WRITER,
        "story_show",
        json!({"repository": "core", "id": "ST-001"}),
    );
    let written = server.call_as(
        WRITER,
        "story_update",
        json!({"repository": "core", "id": "ST-001", "expected_sha256": shown["sha256"], "set": {"title": "Renamed"}}),
    );
    assert!(written.get("error").is_none(), "{written}");
    assert_eq!(written["state"], "proposed", "{written}");
    assert_eq!(written["branch"], "story-time/ST-001");
    // The stub holds four pull requests already, so this one is the fifth.
    assert_eq!(
        written["pr_url"],
        format!("{}/o/core/pulls/5", stub.url).as_str()
    );

    // The remote: the default branch did not move; the story branch is one commit ahead.
    assert_eq!(fixture.git(&bare, &["rev-parse", "main"]), main);
    assert_eq!(
        fixture.git(&bare, &["rev-parse", "story-time/ST-001"]),
        written["head_sha"].as_str().unwrap()
    );
    assert_eq!(
        fixture.git(&bare, &["rev-list", "--count", "main..story-time/ST-001"]),
        "1"
    );
    let author = fixture.git(
        &bare,
        &["log", "-1", "--format=%an|%ae", "story-time/ST-001"],
    );
    assert_eq!(author, "story-time|story-time@noreply.127.0.0.1");

    // The forge: one pull request, to the default branch, with the Stories line first.
    assert_eq!(stub.posts(), 1);
    let pulls = stub.pulls();
    assert_eq!(pulls.len(), 5);
    assert_eq!(pulls[4].head, "story-time/ST-001");
    assert_eq!(pulls[4].base, "main");
    assert!(pulls[4].title.contains("ST-001") && pulls[4].title.contains("Renamed"));
    assert_eq!(pulls[4].body.lines().next().unwrap(), "Stories: ST-001");

    // Each REST request carried the token in the Gitea form, and no output shows the token.
    let basic = base64(format!("x-access-token:{GITEA_TOKEN}").as_bytes());
    for call in stub.state.lock().unwrap().rest.iter() {
        assert_eq!(call.3, format!("token {GITEA_TOKEN}"), "{call:?}");
    }
    for text in [written.to_string(), server.stderr()] {
        assert!(
            !text.contains(GITEA_TOKEN) && !text.contains(&basic),
            "{text}"
        );
    }
}

#[test]
fn a_second_edit_on_a_gitea_repository_uses_the_open_pull_request() {
    let fixture = Fixture::new();
    let (stub, registry, token_file) = writable_gitea(&fixture);
    let server = serve_gitea(&fixture, &registry, &token_file);
    let bare = bare_of(&fixture);

    let shown = server.call_as(
        WRITER,
        "story_show",
        json!({"repository": "core", "id": "ST-001"}),
    );
    let first = server.call_as(
        WRITER,
        "story_update",
        json!({"repository": "core", "id": "ST-001", "expected_sha256": shown["sha256"], "set": {"size": 3}}),
    );
    assert!(first.get("error").is_none(), "{first}");
    let second = server.call_as(
        WRITER,
        "story_note",
        json!({"repository": "core", "id": "ST-001", "expected_sha256": first["sha256_after"], "note": "Reviewed"}),
    );
    assert!(second.get("error").is_none(), "{second}");
    assert_eq!(second["state"], "proposed");
    assert_eq!(second["pr_url"], first["pr_url"], "{second}");
    assert_eq!(
        fixture.git(&bare, &["rev-list", "--count", "main..story-time/ST-001"]),
        "2",
        "a second commit on the same branch"
    );
    assert_eq!(stub.posts(), 1, "no second pull request");
    assert_eq!(stub.pulls().len(), 5);

    // The list has no `head` filter. The server read the pages until it found its own pull
    // request, and skipped the pull request of a fork that has the same head name.
    let state = stub.state.lock().unwrap();
    let lists: Vec<&String> = state
        .rest
        .iter()
        .filter(|call| call.0 == "GET")
        .map(|call| &call.2)
        .collect();
    assert!(
        lists.iter().all(|query| query.contains("state=open")),
        "{lists:?}"
    );
    assert!(
        lists.iter().any(|query| query.contains("page=3")),
        "{lists:?}"
    );
}

#[test]
fn story_create_and_story_note_on_a_gitea_repository_have_the_fields_of_github() {
    let fixture = Fixture::new();
    let (stub, registry, token_file) = writable_gitea(&fixture);
    let server = serve_gitea(&fixture, &registry, &token_file);
    let bare = bare_of(&fixture);
    let schemas: std::collections::HashMap<String, Value> = server
        .tools(WRITER)
        .into_iter()
        .map(|tool| {
            (
                tool["name"].as_str().unwrap().to_owned(),
                tool["outputSchema"].clone(),
            )
        })
        .collect();

    let shown = server.call_as(
        WRITER,
        "story_show",
        json!({"repository": "core", "id": "ST-001"}),
    );
    let noted = server.call_as(
        WRITER,
        "story_note",
        json!({"repository": "core", "id": "ST-001", "expected_sha256": shown["sha256"], "note": "A note"}),
    );
    let created = server.call_as(
        WRITER,
        "story_create",
        json!({"repository": "core", "title": "A new story"}),
    );
    for (name, result) in [("story_note", &noted), ("story_create", &created)] {
        assert!(result.get("error").is_none(), "{name}: {result}");
        let schema = &schemas[name];
        for field in schema["required"].as_array().unwrap() {
            assert!(
                result.get(field.as_str().unwrap()).is_some(),
                "{name} lacks {field}: {result}"
            );
        }
        for field in result.as_object().unwrap().keys() {
            assert!(
                schema["properties"].get(field).is_some(),
                "{name} has the unlisted field {field}: {result}"
            );
        }
        assert_eq!(result["state"], "proposed", "{name}");
        assert!(
            result["pr_url"].as_str().unwrap().starts_with(&stub.url),
            "{result}"
        );
        let id = result["story"]["id"].as_str().unwrap();
        assert_eq!(result["branch"], format!("story-time/{id}").as_str());
        assert_eq!(
            fixture.git(&bare, &["rev-parse", &format!("story-time/{id}")]),
            result["head_sha"].as_str().unwrap()
        );
    }
    assert_eq!(stub.posts(), 2, "one pull request for each story");
}

#[test]
fn a_refused_gitea_token_gives_forge_unavailable_and_no_output_shows_the_token() {
    let fixture = Fixture::new();
    let (stub, registry, token_file) = writable_gitea(&fixture);
    let server = serve_gitea(&fixture, &registry, &token_file);
    let bare = bare_of(&fixture);
    let shown = server.call_as(
        WRITER,
        "story_show",
        json!({"repository": "core", "id": "ST-001"}),
    );
    let update = json!({"repository": "core", "id": "ST-001", "expected_sha256": shown["sha256"], "set": {"size": 3}});

    // The REST API refuses the token. The branch that the server pushed is removed again.
    stub.state.lock().unwrap().rest_token = "gta_SomeOtherToken".into();
    let refused_rest = server.call_as(WRITER, "story_update", update.clone());
    assert_eq!(
        refused_rest["error"]["code"], "forge_unavailable",
        "{refused_rest}"
    );
    assert_eq!(refused_rest["error"]["retryable"], true);
    assert_eq!(
        refused_rest["error"]["details"]["operation"], "open the pull request",
        "{refused_rest}"
    );
    assert!(
        refused_rest["error"]["details"]["reason"]
            .as_str()
            .unwrap()
            .contains("401"),
        "{refused_rest}"
    );
    assert!(!fixture
        .git(&bare, &["branch", "--list"])
        .contains("story-time/ST-001"));
    assert_eq!(stub.posts(), 1, "the one refused create");

    // Git refuses the push.
    {
        let mut state = stub.state.lock().unwrap();
        state.rest_token = GITEA_TOKEN.into();
        state.refuse_push = true;
    }
    let refused_push = server.call_as(WRITER, "story_update", update);
    assert_eq!(
        refused_push["error"]["code"], "forge_unavailable",
        "{refused_push}"
    );
    assert_eq!(
        refused_push["error"]["details"]["operation"], "push the story branch",
        "{refused_push}"
    );
    assert!(!fixture
        .git(&bare, &["branch", "--list"])
        .contains("story-time/ST-001"));
    assert_eq!(stub.pulls().len(), 4, "no pull request of ours");

    let basic = base64(format!("x-access-token:{GITEA_TOKEN}").as_bytes());
    for text in [
        shown.to_string(),
        refused_rest.to_string(),
        refused_push.to_string(),
        server.stderr(),
    ] {
        assert!(
            !text.contains(GITEA_TOKEN) && !text.contains(&basic),
            "{text}"
        );
    }
}
