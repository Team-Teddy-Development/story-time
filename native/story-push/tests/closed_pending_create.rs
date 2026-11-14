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

struct Issue {
    number: u64,
    title: String,
    state: String,
}

#[derive(Default)]
struct Tracker {
    issues: Vec<Issue>,
    labels: Vec<String>,
    writes: Vec<String>,
}

struct FakeGitea {
    base: String,
    state: Arc<Mutex<Tracker>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl FakeGitea {
    fn start(open: &[(u64, &str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(Tracker {
            issues: open
                .iter()
                .map(|(number, title)| Issue {
                    number: *number,
                    title: (*title).to_owned(),
                    state: "open".into(),
                })
                .collect(),
            ..Tracker::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, done) = (state.clone(), stop.clone());
        let handle = thread::spawn(move || {
            while !done.load(AtomicOrdering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &shared),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("fake tracker accept failed: {e}"),
                }
            }
        });
        Self {
            base,
            state,
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

fn serve(mut stream: TcpStream, state: &Mutex<Tracker>) {
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
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let request = head.lines().next().unwrap().to_owned();
    let (method, target) = {
        let mut parts = request.split(' ');
        (parts.next().unwrap(), parts.next().unwrap())
    };
    let path = target
        .strip_prefix("/api/v1/repos/acme/shop")
        .unwrap_or_else(|| panic!("unexpected request {request}"));
    let first_page = path.contains("page=1&") || path.ends_with("page=1");
    let mut tracker = state.lock().unwrap();
    let route = path.split('?').next().unwrap();
    let issue_number = route
        .strip_prefix("/issues/")
        .map(|rest| rest.trim_end_matches("/labels").parse::<u64>().unwrap());
    let response = match (method, route, issue_number) {
        ("GET", "/labels", _) if first_page => Value::Array(
            tracker
                .labels
                .iter()
                .enumerate()
                .map(|(i, name)| json!({"name":name,"id":i + 1}))
                .collect(),
        ),
        ("GET", "/labels", _) => json!([]),
        ("POST", "/labels", _) => {
            let name = body["name"].as_str().unwrap().to_owned();
            tracker.labels.push(name.clone());
            json!({"name":name,"id":tracker.labels.len()})
        }
        ("GET", "/issues", _) if first_page && path.contains("state=open") => Value::Array(
            tracker
                .issues
                .iter()
                .filter(|i| i.state == "open")
                .map(|i| json!({"number":i.number,"title":i.title,"state":"open"}))
                .collect(),
        ),
        ("GET", "/issues", _) => json!([]),
        ("GET", _, Some(n)) => {
            let issue = tracker.issues.iter().find(|i| i.number == n).unwrap();
            json!({"number":n,"title":issue.title,"state":issue.state,"labels":[]})
        }
        ("PATCH", _, Some(n)) => {
            tracker.writes.push(format!("PATCH {n}"));
            let issue = tracker.issues.iter_mut().find(|i| i.number == n).unwrap();
            issue.title = body["title"].as_str().unwrap().to_owned();
            issue.state = body["state"].as_str().unwrap().to_owned();
            json!({"number":n})
        }
        ("PUT", _, Some(n)) => {
            tracker.writes.push(format!("PUT {n} labels"));
            json!([])
        }
        _ => {
            tracker.writes.push(request.clone());
            panic!("unexpected request {request}")
        }
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

fn fixture(base: &str, status: &str) -> Fixture {
    let f = Fixture::new("auto");
    fs::write(
        f.0.join(".story-time/settings.json"),
        json!({"tracker":"gitea","sync":"auto","gitea":{"baseUrl":base,"owner":"acme","repo":"shop"}}).to_string(),
    )
    .unwrap();
    let story = json!({"id":"ST-001","title":"Native push","status":status,"type":"story","created":"2026-11-11","pending_create":"2026-11-11","sections":{"Story":"As a maintainer I need one issue per story.","Acceptance Criteria":"- [x] Given a retry, When pushed, Then one issue exists."}});
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
fn closed_story_with_a_lost_create_links_and_closes_the_open_item() {
    for status in ["done", "split", "archived"] {
        let tracker = FakeGitea::start(&[(7, "ST-001: Native push"), (8, "ST-002: Other")]);
        let f = fixture(&tracker.base, status);

        let out = push(&f);
        assert!(
            out.status.success(),
            "{status}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(result["pushed"][0]["action"], "link", "{status}");
        assert_eq!(result["pushed"][0]["remoteId"], "7", "{status}");
        assert_eq!(result["skipped"], json!([]), "{status}");
        let state = tracker.state.lock().unwrap();
        let states: Vec<_> = state
            .issues
            .iter()
            .map(|i| (i.number, i.state.as_str()))
            .collect();
        assert_eq!(states, vec![(7, "closed"), (8, "open")], "{status}");
        assert_eq!(state.writes, vec!["PATCH 7", "PUT 7 labels"], "{status}");
        let story = fs::read_to_string(f.0.join("backlog/ST-001.md")).unwrap();
        assert!(story.contains("\nremote: 7\n"), "{status}: {story}");
        assert!(!story.contains("pending_create"), "{status}: {story}");
        assert!(
            story.contains("linked existing tracker item 7"),
            "{status}: {story}"
        );
    }
}

#[test]
fn closed_story_with_a_lost_create_and_no_matching_item_is_skipped() {
    for status in ["done", "split", "archived"] {
        let tracker = FakeGitea::start(&[(8, "ST-002: Other")]);
        let f = fixture(&tracker.base, status);
        let before = fs::read(f.0.join("backlog/ST-001.md")).unwrap();

        let out = push(&f);
        assert!(
            out.status.success(),
            "{status}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(result["pushed"], json!([]), "{status}");
        assert_eq!(result["skipped"], json!(["ST-001"]), "{status}");
        let state = tracker.state.lock().unwrap();
        assert!(state.writes.is_empty(), "{status}: {:?}", state.writes);
        assert_eq!(state.issues.len(), 1, "{status}");
        assert_eq!(state.issues[0].state, "open", "{status}");
        assert_eq!(
            fs::read(f.0.join("backlog/ST-001.md")).unwrap(),
            before,
            "{status}: the story must not change"
        );
    }
}
