use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Output},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Barrier, Mutex,
    },
    thread,
    time::Duration,
};

const REPO_LABELS: [(u64, &str); 6] = [
    (1, "story"),
    (2, "size:3"),
    (3, "size:5"),
    (4, "value:5"),
    (5, "value:8"),
    (99, "team"),
];

fn label_name(id: u64) -> &'static str {
    REPO_LABELS.iter().find(|(i, _)| *i == id).unwrap().1
}

struct Tracker {
    issue_labels: Mutex<BTreeSet<u64>>,
    issue_reads: AtomicUsize,
    both_read: Barrier,
}

struct FakeGitea {
    base: String,
    state: Arc<Tracker>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl FakeGitea {
    fn start(initial: &[u64]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Tracker {
            issue_labels: Mutex::new(initial.iter().copied().collect()),
            issue_reads: AtomicUsize::new(0),
            both_read: Barrier::new(2),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, done) = (state.clone(), stop.clone());
        let handle = thread::spawn(move || {
            let mut workers = Vec::new();
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let shared = shared.clone();
                        workers.push(thread::spawn(move || serve(stream, &shared)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("fake tracker accept failed: {e}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            base,
            state,
            stop,
            handle: Some(handle),
        }
    }

    fn issue_labels(&self) -> BTreeSet<&'static str> {
        self.state
            .issue_labels
            .lock()
            .unwrap()
            .iter()
            .map(|id| label_name(*id))
            .collect()
    }
}

impl Drop for FakeGitea {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

fn ids(body: &Value) -> Vec<u64> {
    body["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect()
}

fn serve(mut stream: TcpStream, state: &Tracker) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
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
    let issue = |labels: &BTreeSet<u64>| json!({"number":4,"state":"open","labels":labels.iter().map(|id| json!({"id":id,"name":label_name(*id)})).collect::<Vec<_>>()});
    let response = match (method, path) {
        ("GET", "/labels?page=1&limit=50") => Value::Array(
            REPO_LABELS
                .iter()
                .map(|(id, name)| json!({"id":id,"name":name}))
                .collect(),
        ),
        ("GET", p) if p.starts_with("/labels?") => json!([]),
        ("GET", "/issues/4") => {
            let snapshot = issue(&state.issue_labels.lock().unwrap());
            if state.issue_reads.fetch_add(1, Ordering::SeqCst) < 2 {
                state.both_read.wait();
            }
            snapshot
        }
        ("PATCH", "/issues/4") => issue(&state.issue_labels.lock().unwrap()),
        ("PUT", "/issues/4/labels") => {
            let mut labels = state.issue_labels.lock().unwrap();
            *labels = ids(&body).into_iter().collect();
            issue(&labels)
        }
        ("POST", "/issues/4/labels") => {
            let mut labels = state.issue_labels.lock().unwrap();
            labels.extend(ids(&body));
            issue(&labels)
        }
        ("DELETE", p) if p.starts_with("/issues/4/labels/") => {
            let id: u64 = p.rsplit('/').next().unwrap().parse().unwrap();
            state.issue_labels.lock().unwrap().remove(&id);
            Value::Null
        }
        _ => panic!("unexpected request {request}"),
    };
    let response = if response.is_null() {
        String::new()
    } else {
        response.to_string()
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    )
    .unwrap();
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Checkout(PathBuf);

impl Checkout {
    fn new(base: &str, size: u64, value: u64) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-concurrent-labels-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::write(
            path.join(".story-time/settings.json"),
            json!({"tracker":"gitea","sync":"auto","gitea":{"baseUrl":base,"owner":"acme","repo":"shop"}}).to_string(),
        )
        .unwrap();
        let story = json!({"id":"ST-001","title":"Native push","status":"ready","type":"story","size":size,"value":value,"remote":4,"created":"2026-11-11","sections":{"Story":"As a maintainer I need labels kept.","Acceptance Criteria":"- [ ] Given two pushes, When both finish, Then both label changes remain."}});
        fs::write(
            path.join("backlog/ST-001.md"),
            story_time_native::story::serialize(&story).unwrap(),
        )
        .unwrap();
        Self(path)
    }

    fn push(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .args(["push", "ST-001", "--json"])
            .current_dir(&self.0)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("GITEA_TOKEN", "test-only")
            .output()
            .unwrap()
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn two_pushes_that_change_different_labels_at_once_both_keep_their_change() {
    let tracker = FakeGitea::start(&[1, 2, 4, 99]);
    let resize = Checkout::new(&tracker.base, 5, 5);
    let revalue = Checkout::new(&tracker.base, 3, 8);

    let outputs = thread::scope(|s| {
        let a = s.spawn(|| resize.push());
        let b = s.spawn(|| revalue.push());
        [a.join().unwrap(), b.join().unwrap()]
    });
    for output in &outputs {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["pushed"][0]["action"], "update");
    }
    assert!(
        tracker.state.issue_reads.load(Ordering::SeqCst) >= 2,
        "both pushes must read the issue before either writes"
    );
    assert_eq!(
        tracker.issue_labels(),
        BTreeSet::from(["story", "size:5", "value:8", "team"]),
        "the resize and the revalue must both survive"
    );
}
