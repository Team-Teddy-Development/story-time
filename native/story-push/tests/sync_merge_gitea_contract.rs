//! ST-157: `story sync-merge` with `tracker: gitea`, against a temporary git repository and a
//! fake Gitea 1.21 API (no run history, no commit-to-pull endpoint).
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Output},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const API: &str = "/api/v1/repos/acme/shop";
/// The user that a Gitea Actions token acts as.
const ACTIONS_USER: i64 = -2;

/// One answer: `pattern` is a request line `METHOD path`; a trailing `*` makes it a prefix,
/// otherwise it must equal the request line without its query. `"ECHO_LABEL"` echoes a label.
type Rule = (String, u16, Value);

struct Fake {
    base: String,
    rules: Arc<Mutex<Vec<Rule>>>,
    log: Arc<Mutex<Vec<(String, Value)>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Fake {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let rules: Arc<Mutex<Vec<Rule>>> = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, l, s) = (rules.clone(), log.clone(), stop.clone());
        let handle = thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &r, &l),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("fake Gitea accept failed: {e}"),
                }
            }
        });
        let fake = Self {
            base,
            rules,
            log,
            stop,
            handle: Some(handle),
        };
        fake.on(
            &format!("GET {API}"),
            200,
            json!({"default_branch": "main"}),
        );
        fake.on(
            "GET /api/v1/user",
            200,
            json!({"id": ACTIONS_USER, "login": "gitea-actions"}),
        );
        fake.on(&format!("GET {API}/labels*"), 200, json!([]));
        fake.on(&format!("POST {API}/labels"), 200, json!("ECHO_LABEL"));
        fake.on(&format!("GET {API}/pulls*"), 200, json!([]));
        fake
    }
    /// Adds an answer that wins over every earlier one.
    fn on(&self, pattern: &str, status: u16, body: Value) {
        self.rules
            .lock()
            .unwrap()
            .insert(0, (pattern.to_string(), status, body));
    }
    fn issue(&self, n: u64, state: &str) {
        self.on(&format!("GET {API}/issues/{n}/comments"), 200, json!([]));
        self.on(
            &format!("POST {API}/issues/{n}/comments"),
            201,
            json!({"id": 1}),
        );
        self.on(
            &format!("GET {API}/issues/{n}"),
            200,
            json!({"number": n, "state": state, "labels": []}),
        );
        self.on(
            &format!("PATCH {API}/issues/{n}"),
            201,
            json!({"number": n}),
        );
        self.on(&format!("POST {API}/issues/{n}/labels"), 200, json!([]));
        self.on(&format!("PUT {API}/issues/{n}/labels"), 200, json!([]));
        self.on(
            &format!("DELETE {API}/issues/{n}/labels/*"),
            204,
            Value::Null,
        );
    }
    fn pulls(&self, pulls: Value) {
        self.on(&format!("GET {API}/pulls*"), 200, json!([]));
        self.on(
            &format!("GET {API}/pulls?state=closed&sort=recentupdate&page=1&*"),
            200,
            pulls,
        );
    }
    fn calls(&self) -> Vec<String> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .map(|(l, _)| l.clone())
            .collect()
    }
    fn count(&self, line: &str) -> usize {
        self.calls().iter().filter(|l| l.as_str() == line).count()
    }
    fn reset(&self) {
        self.log.lock().unwrap().clear();
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

fn serve(mut stream: TcpStream, rules: &Mutex<Vec<Rule>>, log: &Mutex<Vec<(String, Value)>>) {
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
    assert!(
        head.to_lowercase()
            .contains("authorization: token test-only"),
        "{head}"
    );
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
    let mut words = head.lines().next().unwrap().split(' ');
    let line = format!("{} {}", words.next().unwrap(), words.next().unwrap());
    let bare = line.split('?').next().unwrap().to_string();
    let (status, mut answer) = rules
        .lock()
        .unwrap()
        .iter()
        .find(|(pattern, _, _)| match pattern.strip_suffix('*') {
            Some(prefix) => line.starts_with(prefix),
            None => *pattern == bare,
        })
        .map(|(_, status, answer)| (*status, answer.clone()))
        .unwrap_or_else(|| panic!("unexpected request {line}"));
    if answer == json!("ECHO_LABEL") {
        answer = json!({"name": body["name"], "id": 100 + log.lock().unwrap().len()});
    }
    log.lock().unwrap().push((bare, body));
    let text = if answer.is_null() {
        String::new()
    } else {
        answer.to_string()
    };
    write!(
        stream,
        "HTTP/1.1 {status} Fake\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )
    .unwrap();
}

struct Repo {
    path: PathBuf,
    fake: Fake,
}
impl Repo {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-sync-gitea-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::create_dir_all(path.join("backlog")).unwrap();
        let fake = Fake::start();
        fs::write(
            path.join(".story-time/settings.json"),
            json!({"tracker":"gitea","sync":"auto","gitea":{"baseUrl":fake.base,"owner":"acme","repo":"shop"}})
                .to_string(),
        )
        .unwrap();
        let repo = Self {
            path: path.canonicalize().unwrap(),
            fake,
        };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "t@example.com"]);
        repo.git(&["config", "user.name", "T"]);
        repo
    }
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
    fn card(&self, id: &str, status: &str, remote: Option<u64>) {
        let remote = remote.map(|n| format!("remote: {n}\n")).unwrap_or_default();
        fs::write(
            self.path.join(format!("backlog/{id}.md")),
            format!("---\nid: {id}\ntitle: Story {id}\nstatus: {status}\ntype: story\n{remote}---\n\n## Story\nAs a user, I want it.\n\n## Acceptance Criteria\n- [x] Given a, when b, then c\n"),
        )
        .unwrap();
    }
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
    fn sync_as(&self, branch: &str, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .arg("sync-merge")
            .args(extra)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap())
            .env("HOME", &self.path)
            .env("GITEA_TOKEN", "test-only")
            .env("GITHUB_REPOSITORY", "acme/shop")
            .env("GITHUB_REF_NAME", branch)
            .current_dir(&self.path)
            .output()
            .unwrap()
    }
    fn sync(&self, extra: &[&str]) -> Output {
        self.sync_as("main", extra)
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}
fn report(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout).unwrap_or_else(|_| panic!("{}", text(o)))
}
fn pull(number: u64, sha: &str, body: &str) -> Value {
    json!({"number": number, "body": body, "base": {"ref": "main"}, "merged": true, "merge_commit_sha": sha})
}

/// A base commit with two linked cards, then a merge commit that sets both done.
fn merged_two(r: &Repo) -> (String, String) {
    r.card("VC-001", "in-progress", Some(12));
    r.card("VC-002", "in-progress", Some(13));
    let base = r.commit("base");
    r.card("VC-001", "done", Some(12));
    r.card("VC-002", "done", Some(13));
    let merge = r.commit("Merge pull request #20");
    r.fake.pulls(json!([
        pull(
            19,
            "1111111111111111111111111111111111111111",
            "Stories: VC-009"
        ),
        pull(20, &merge, "Stories: VC-001 (#12), VC-002 (#13)"),
    ]));
    r.fake.issue(12, "open");
    r.fake.issue(13, "open");
    (base, merge)
}

fn with_prefix(r: &Repo) {
    let settings = r.path.join(".story-time/settings.json");
    let mut value: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    value["prefix"] = json!("VC");
    fs::write(&settings, value.to_string()).unwrap();
}

#[test]
fn ac1_named_issues_match_their_cards_and_get_one_merge_comment() {
    let r = Repo::new();
    with_prefix(&r);
    let (base, merge) = merged_two(&r);
    let o = r.sync(&["--from", &base, "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let rep = report(&o);
    assert_eq!(rep["range"]["mode"], "range", "{rep}");
    assert_eq!(rep["prs"], json!([20]));
    assert_eq!(rep["updated"], json!(["VC-001", "VC-002"]));
    assert_eq!(rep["failed"], json!([]));
    for n in [12, 13] {
        assert_eq!(
            r.fake.count(&format!("PATCH {API}/issues/{n}")),
            1,
            "{:?}",
            r.fake.calls()
        );
        assert_eq!(r.fake.count(&format!("POST {API}/issues/{n}/comments")), 1);
    }
    let posted = r
        .fake
        .log
        .lock()
        .unwrap()
        .iter()
        .find(|(l, _)| l == &format!("POST {API}/issues/12/comments"))
        .map(|(_, b)| b["body"].as_str().unwrap().to_string())
        .unwrap();
    assert_eq!(
        posted,
        format!("Merged in #20 ({merge})\n<!-- story-time:v1:merge-sync={merge} -->")
    );
    // A rerun finds the token user's markers and posts nothing.
    for n in [12, 13] {
        r.fake.on(
            &format!("GET {API}/issues/{n}/comments"),
            200,
            json!([{"user": {"id": ACTIONS_USER}, "body": posted}]),
        );
    }
    r.fake.reset();
    let o = r.sync(&["--from", &base, "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert_eq!(r.fake.count(&format!("POST {API}/issues/12/comments")), 0);
    assert_eq!(r.fake.count(&format!("POST {API}/issues/13/comments")), 0);
    assert_eq!(report(&o)["existing"].as_array().unwrap().len(), 2);
    assert_eq!(
        r.git(&["status", "--porcelain"]),
        "",
        "no card file is written"
    );
}

#[test]
fn ac2_a_zero_or_foreign_from_takes_the_all_path() {
    for (from, reason) in [
        (
            "0000000000000000000000000000000000000000",
            "push has no earlier commit",
        ),
        (
            "2222222222222222222222222222222222222222",
            "anchor is not an ancestor of HEAD",
        ),
    ] {
        let r = Repo::new();
        with_prefix(&r);
        let _ = merged_two(&r);
        let rep = report(&r.sync(&["--from", from, "--json"]));
        assert_eq!(rep["range"]["mode"], "all");
        assert_eq!(rep["range"]["reason"], reason);
        assert_eq!(rep["updated"], json!(["VC-001", "VC-002"]));
        assert!(
            !r.fake.calls().iter().any(|l| l.contains("/comments")),
            "{:?}",
            r.fake.calls()
        );
    }
}

#[test]
fn ac3_a_marker_from_another_user_does_not_count() {
    let r = Repo::new();
    with_prefix(&r);
    let (base, merge) = merged_two(&r);
    let marker = format!("<!-- story-time:v1:merge-sync={merge} -->");
    r.fake.on(
        &format!("GET {API}/issues/12/comments"),
        200,
        json!([{"user": {"id": 7}, "body": marker}]),
    );
    let o = r.sync(&["--from", &base]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert_eq!(r.fake.count(&format!("POST {API}/issues/12/comments")), 1);
}

#[test]
fn without_from_the_missing_run_history_takes_the_all_path() {
    let r = Repo::new();
    with_prefix(&r);
    let _ = merged_two(&r);
    let rep = report(&r.sync(&["--json"]));
    assert_eq!(rep["range"]["reason"], "run history cannot be read");
}

#[test]
fn a_server_error_exits_1_and_a_missing_issue_exits_0() {
    let r = Repo::new();
    with_prefix(&r);
    let (base, _) = merged_two(&r);
    r.fake.on(&format!("GET {API}/issues/12"), 503, Value::Null);
    let o = r.sync(&["--from", &base, "--json"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert_eq!(report(&o)["failed"][0]["kind"], "retryable");
    r.fake.on(&format!("GET {API}/issues/12"), 404, Value::Null);
    let o = r.sync(&["--from", &base, "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert_eq!(report(&o)["failed"][0]["kind"], "permanent");
    assert!(
        !text(&o).contains("test-only"),
        "the token never reaches the report"
    );
}

#[test]
fn pulls_are_read_once_per_run() {
    let r = Repo::new();
    with_prefix(&r);
    let (base, _) = merged_two(&r);
    for i in 0..3 {
        r.commit(&format!("direct {i}"));
    }
    let o = r.sync(&["--from", &base]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let pages: Vec<_> = r
        .fake
        .calls()
        .into_iter()
        .filter(|l| l.ends_with("/pulls"))
        .collect();
    // Page 1 holds the merge; the three direct commits read page 2 (empty) once, then stop.
    assert_eq!(pages.len(), 2, "{:?}", r.fake.calls());
}

#[test]
fn a_feature_branch_ref_is_refused() {
    let r = Repo::new();
    with_prefix(&r);
    let (base, _) = merged_two(&r);
    let o = r.sync_as("feature", &["--from", &base]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
}
