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

#[derive(Clone, Copy)]
enum FirstCreate {
    LandsThenDrops,
    DropsBeforeLanding,
}

#[derive(Default)]
struct Issues {
    open: Vec<(u64, String)>,
    create_requests: usize,
}

struct FakeGitea {
    base: String,
    issues: Arc<Mutex<Issues>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl FakeGitea {
    fn start(first: FirstCreate) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let issues = Arc::new(Mutex::new(Issues::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (state, done) = (issues.clone(), stop.clone());
        let handle = thread::spawn(move || {
            while !done.load(AtomicOrdering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &state, first),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("fake tracker accept failed: {e}"),
                }
            }
        });
        Self {
            base,
            issues,
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

fn serve(mut stream: TcpStream, state: &Mutex<Issues>, first: FirstCreate) {
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
    assert!(head
        .to_ascii_lowercase()
        .contains("authorization: token test-only"));
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
    let (method, target) = {
        let mut parts = request.split(' ');
        (parts.next().unwrap(), parts.next().unwrap())
    };
    let path = target
        .strip_prefix("/api/v1/repos/acme/shop")
        .unwrap_or_else(|| panic!("unexpected request {request}"));
    let first_page = path.contains("page=1&") || path.ends_with("page=1");
    let mut issues = state.lock().unwrap();
    let response = match (method, path.split('?').next().unwrap()) {
        ("GET", "/labels") if first_page => json!([{"name":"story","id":1}]),
        ("GET", "/labels") => json!([]),
        ("GET", "/issues") if first_page && path.contains("state=open") => Value::Array(
            issues
                .open
                .iter()
                .map(|(n, t)| json!({"number":n,"title":t,"state":"open"}))
                .collect(),
        ),
        ("GET", "/issues") => json!([]),
        ("POST", "/issues") => {
            issues.create_requests += 1;
            let title = serde_json::from_slice::<Value>(&body).unwrap()["title"]
                .as_str()
                .unwrap()
                .to_owned();
            let number = issues.open.len() as u64 + 1;
            if issues.create_requests == 1 {
                if let FirstCreate::LandsThenDrops = first {
                    issues.open.push((number, title));
                }
                return;
            }
            issues.open.push((number, title));
            json!({"number":number})
        }
        _ => panic!("unexpected request {request}"),
    };
    let response = response.to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    )
    .unwrap();
}

fn fixture(base: &str) -> Fixture {
    let f = Fixture::new("auto");
    fs::write(
        f.0.join(".story-time/settings.json"),
        json!({"tracker":"gitea","sync":"auto","gitea":{"baseUrl":base,"owner":"acme","repo":"shop"}}).to_string(),
    )
    .unwrap();
    let story = json!({"id":"ST-001","title":"Native push","status":"ready","type":"story","created":"2026-11-11","sections":{"Story":"As a maintainer I need one issue per story.","Acceptance Criteria":"- [ ] Given a retry, When pushed, Then one issue exists."}});
    fs::write(
        f.0.join("backlog/ST-001.md"),
        story_time_native::story::serialize(&story).unwrap(),
    )
    .unwrap();
    f
}

fn push(f: &Fixture) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["push", "ST-001", "--json"])
        .current_dir(&f.0)
        .env_clear()
        .env("PATH", "/nonexistent")
        .env("GITEA_TOKEN", "test-only")
        .output()
        .unwrap()
}

#[test]
fn retry_after_a_create_that_landed_but_timed_out_links_the_existing_issue() {
    let tracker = FakeGitea::start(FirstCreate::LandsThenDrops);
    let f = fixture(&tracker.base);

    let first = push(&f);
    assert!(!first.status.success(), "the dropped create must fail");
    assert_eq!(tracker.issues.lock().unwrap().open.len(), 1);

    let second = push(&f);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let result: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(result["pushed"][0]["action"], "link");
    assert_eq!(result["pushed"][0]["remoteId"], "1");
    let issues = tracker.issues.lock().unwrap();
    assert_eq!(issues.open, vec![(1, "ST-001: Native push".to_owned())]);
    assert_eq!(issues.create_requests, 1, "the retry must not create again");
    let story = fs::read_to_string(f.0.join("backlog/ST-001.md")).unwrap();
    assert!(story.contains("\nremote: 1\n"), "{story}");
    assert!(!story.contains("pending_create"), "{story}");
}

#[test]
fn retry_after_a_create_that_never_landed_creates_exactly_one_issue() {
    let tracker = FakeGitea::start(FirstCreate::DropsBeforeLanding);
    let f = fixture(&tracker.base);

    let first = push(&f);
    assert!(!first.status.success(), "the dropped create must fail");
    assert!(tracker.issues.lock().unwrap().open.is_empty());

    let second = push(&f);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let result: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(result["pushed"][0]["action"], "create");
    assert_eq!(result["pushed"][0]["remoteId"], "1");
    assert_eq!(
        tracker.issues.lock().unwrap().open,
        vec![(1, "ST-001: Native push".to_owned())]
    );
    let story = fs::read_to_string(f.0.join("backlog/ST-001.md")).unwrap();
    assert!(story.contains("\nremote: 1\n"), "{story}");
    assert!(!story.contains("pending_create"), "{story}");
}
