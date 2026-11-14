use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

const TOKEN: &str = "test-only-issues-token";
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Request {
    method: String,
    path: String,
    authorization: Option<String>,
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
        let (log, done, served) = (requests.clone(), stop.clone(), base.clone());
        let handle = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &log, &served),
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
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

fn serve(mut stream: TcpStream, log: &Mutex<Vec<Request>>, base: &str) {
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
    let mut parts = head.lines().next().unwrap().split(' ');
    let method = parts.next().unwrap().to_owned();
    let path = parts.next().unwrap().to_owned();
    let authorization = head.lines().find_map(|line| {
        line.split_once(':')
            .filter(|(key, _)| key.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.trim().to_owned())
    });
    let issue = |number: u64, title: &str, labels: &[&str]| {
        json!({
            "number": number,
            "title": title,
            "body": format!("Body of {number}"),
            "html_url": format!("{base}/acme/shop/issues/{number}"),
            "labels": labels.iter().map(|name| json!({"name": name, "id": 1})).collect::<Vec<_>>(),
            "pull_request": null
        })
    };
    let response = if path.starts_with("/api/v1/repos/acme/shop/issues?") {
        if path.contains("page=1&") {
            json!([issue(3, "Checkout times out", &["bug"])])
        } else if path.contains("page=2&") {
            let mut pull = issue(4, "Fix checkout", &[]);
            pull["pull_request"] = json!({"merged": false});
            json!([issue(5, "Export to CSV", &["story", "team:data"]), pull])
        } else {
            json!([])
        }
    } else {
        json!({"message": "not found"})
    };
    log.lock().unwrap().push(Request {
        method,
        path,
        authorization,
    });
    let response = response.to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    )
    .unwrap();
}

struct Project(PathBuf);

impl Project {
    fn new(settings: Value) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-open-issues-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::create_dir(path.join("backlog")).unwrap();
        fs::write(path.join(".story-time/settings.json"), settings.to_string()).unwrap();
        Self(path)
    }

    fn story(&self, args: &[&str]) -> Output {
        story_in(&self.0, args)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn story_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_story"))
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("GITEA_TOKEN", TOKEN)
        .output()
        .unwrap()
}

#[test]
fn story_issues_lists_open_gitea_issues_across_pages_with_only_gets() {
    let tracker = FakeGitea::start();
    let project = Project::new(
        json!({"tracker":"gitea","sync":"ask","gitea":{"baseUrl":tracker.base,"owner":"acme","repo":"shop"}}),
    );

    let output = project.story(&["issues", "--state", "open", "--json"]);

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "stdout={stdout} stderr={stderr}");
    let issues: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        issues,
        json!([
            {"number":3,"title":"Checkout times out","labels":["bug"],"body":"Body of 3","url":format!("{}/acme/shop/issues/3", tracker.base)},
            {"number":5,"title":"Export to CSV","labels":["story","team:data"],"body":"Body of 5","url":format!("{}/acme/shop/issues/5", tracker.base)}
        ]),
        "open issues from both pages, without the pull request"
    );
    assert!(!stdout.contains(TOKEN) && !stderr.contains(TOKEN));
    let requests = tracker.requests.lock().unwrap();
    assert!(requests.len() >= 3, "pages until an empty page");
    for request in requests.iter() {
        assert_eq!(request.method, "GET", "{}", request.path);
        assert!(request.path.contains("state=open"), "{}", request.path);
        assert!(request.path.contains("type=issues"), "{}", request.path);
        assert!(request.path.contains("limit=50"), "{}", request.path);
        assert_eq!(
            request.authorization.as_deref(),
            Some(format!("token {TOKEN}").as_str())
        );
    }
}

#[test]
fn story_issues_stops_at_its_limit() {
    let tracker = FakeGitea::start();
    let project = Project::new(
        json!({"tracker":"gitea","sync":"ask","gitea":{"baseUrl":tracker.base,"owner":"acme","repo":"shop"}}),
    );

    let output = project.story(&["issues", "--state", "open", "--limit", "1", "--json"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let issues: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(issues.as_array().unwrap().len(), 1);
    assert_eq!(issues[0]["number"], 3);
    assert_eq!(tracker.requests.lock().unwrap().len(), 1);
}

#[test]
fn story_issues_points_github_projects_to_gh() {
    let project =
        Project::new(json!({"tracker":"github","sync":"ask","github":{"repo":"acme/shop"}}));

    let output = project.story(&["issues", "--state", "open", "--json"]);

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("gh issue list"), "{stderr}");
}
