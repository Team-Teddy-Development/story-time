include!("../../../tests/empty-push.rs");

use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering as AtomicOrdering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

const ISSUE: u64 = 7;
const TEAM_LABEL: u64 = 50;

struct Request {
    method: String,
    path: String,
    body: Value,
}

struct FakeGitea {
    base: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl FakeGitea {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (log, done) = (requests.clone(), stop.clone());
        let handle = thread::spawn(move || {
            while !done.load(AtomicOrdering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &log),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("fake tracker accept failed: {e}"),
                }
            }
        });
        Self {
            base,
            requests,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for FakeGitea {
    fn drop(&mut self) {
        self.stop.store(true, AtomicOrdering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

fn serve(mut stream: TcpStream, log: &Mutex<Vec<Request>>) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let length = head
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    let request = head.lines().next().unwrap().to_owned();
    let mut parts = request.split(' ');
    let method = parts.next().unwrap().to_owned();
    let path = parts
        .next()
        .unwrap()
        .strip_prefix("/api/v1/repos/acme/shop")
        .unwrap_or_else(|| panic!("unexpected request {request}"))
        .to_owned();
    let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let route = path.split('?').next().unwrap();
    let issue = format!("/issues/{ISSUE}");
    let response = match (method.as_str(), route) {
        ("GET", "/labels") if path.contains("page=1&") => {
            json!([{"name":"story","id":1},{"name":"team:payments","id":TEAM_LABEL}])
        }
        ("GET", "/labels") => json!([]),
        ("POST", "/labels") => json!({"name":body["name"],"id":100 + log.lock().unwrap().len()}),
        ("GET", r) if r == issue => json!({
            "number":ISSUE,
            "title":"Checkout times out",
            "body":"Reported by a customer.",
            "state":"open",
            "labels":[{"name":"team:payments","id":TEAM_LABEL}]
        }),
        ("PATCH", r) if r == issue => json!({"number":ISSUE}),
        ("POST", r) if r == format!("{issue}/labels") => json!([]),
        ("POST", "/issues") => json!({"number":ISSUE + 1}),
        _ => panic!("unexpected request {request}"),
    };
    log.lock().unwrap().push(Request { method, path, body });
    let response = response.to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    )
    .unwrap();
}

fn story(f: &Fixture, args: &[&str]) -> std::process::Output {
    let output = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(args)
        .current_dir(&f.0)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env(
            "STORY_TIME_LEGACY_CLI",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../skills/story-time/scripts/story.mjs"),
        )
        .env("GITEA_TOKEN", "test-only")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "story {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn a_story_that_takes_over_an_open_gitea_issue_updates_it_instead_of_creating_one() {
    let tracker = FakeGitea::start();
    let f = Fixture::new("auto");
    fs::write(
        f.0.join(".story-time/settings.json"),
        json!({"tracker":"gitea","sync":"auto","gitea":{"baseUrl":tracker.base,"owner":"acme","repo":"shop"}}).to_string(),
    )
    .unwrap();
    let draft = json!({"id":"ST-001","title":"Retry a timed-out checkout","status":"draft","type":"story","created":"2026-11-14","sections":{"Story":"As a shopper I want checkout retried so that a slow payment does not lose my order.","Acceptance Criteria":"- [ ] Given a timeout, When checkout runs, Then it retries once.","Notes":"Original issue #7: Checkout times out. Reported by a customer."}});
    fs::write(
        f.0.join("backlog/ST-001.md"),
        story_time_native::story::serialize(&draft).unwrap(),
    )
    .unwrap();

    story(&f, &["set", "ST-001", &format!("remote={ISSUE}")]);
    let pushed = story(&f, &["push", "ST-001", "--json"]);

    let result: Value = serde_json::from_slice(&pushed.stdout).unwrap();
    assert_eq!(result["pushed"][0]["action"], "update");
    assert_eq!(result["pushed"][0]["remoteId"], ISSUE.to_string());
    let requests = tracker.requests.lock().unwrap();
    assert!(
        !requests
            .iter()
            .any(|r| r.method == "POST" && r.path.split('?').next() == Some("/issues")),
        "taking over an issue must not create another"
    );
    let edit = requests
        .iter()
        .find(|r| r.method == "PATCH" && r.path == format!("/issues/{ISSUE}"))
        .expect("the adopted issue is edited");
    assert_eq!(edit.body["title"], "ST-001: Retry a timed-out checkout");
    let body = edit.body["body"].as_str().unwrap();
    assert!(
        body.contains("so that a slow payment does not lose my order"),
        "{body}"
    );
    assert!(body.contains("Given a timeout"), "{body}");
    assert_eq!(edit.body["state"], "open");
    assert!(
        !requests
            .iter()
            .any(|r| r.method == "DELETE" && r.path.ends_with(&format!("/{TEAM_LABEL}"))),
        "the issue's own labels stay"
    );
    let saved = fs::read_to_string(f.0.join("backlog/ST-001.md")).unwrap();
    assert!(saved.contains(&format!("\nremote: {ISSUE}\n")), "{saved}");
    assert!(!saved.contains("pending_create"), "{saved}");
}
