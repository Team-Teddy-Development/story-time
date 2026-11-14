use super::{judge, Env, Retry};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

const KEY: &str = "tsk_fake_0123456789abcdef0123456789abcdef";
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Request {
    head: String,
    body: String,
}
impl Request {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap()
    }
    fn is_canary(&self) -> bool {
        self.json()["questions"]
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.starts_with("canary_"))
    }
}

type Reply = (u16, String, Vec<(&'static str, &'static str)>);

/// A local HTTP server standing in for the Jev API; `respond` sees each request and its number.
struct Fake {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}
impl Fake {
    fn new(respond: impl Fn(&Value, usize) -> Reply + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/systemone", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0u8; 4096];
                let split = loop {
                    let n = stream.read(&mut buf).unwrap();
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let head = String::from_utf8(bytes[..split].to_vec()).unwrap();
                let length: usize = head
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                while bytes.len() < split + length {
                    let n = stream.read(&mut buf).unwrap();
                    bytes.extend_from_slice(&buf[..n]);
                }
                let body = String::from_utf8(bytes[split..split + length].to_vec()).unwrap();
                let request: Value = serde_json::from_str(&body).unwrap();
                let number = {
                    let mut all = seen.lock().unwrap();
                    all.push(Request { head, body });
                    all.len()
                };
                let (status, text, headers) = respond(&request, number);
                let extra: String = headers
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\r\n"))
                    .collect();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{extra}\r\n{text}",
                    text.len()
                );
            }
        });
        Self { url, requests }
    }
    fn requests(&self) -> std::sync::MutexGuard<'_, Vec<Request>> {
        self.requests.lock().unwrap()
    }
}

/// `answerFor` and `okBody` from the Node test helper `fake-jev.mjs`.
fn answer_for(id: &str, q: &Value) -> Value {
    match q["type"].as_str().unwrap() {
        "noul" if id == "canary_true" => json!({"type": "noul", "noul": 0.95}),
        "noul" if id.starts_with("canary_false") => json!({"type": "noul", "noul": 0.04}),
        "noul" => json!({"type": "noul", "noul": 0.9}),
        "choice" => {
            let keys: Vec<&String> = q["criteria"].as_object().unwrap().keys().collect();
            let mut probabilities = serde_json::Map::new();
            probabilities.insert(keys[0].clone(), json!(0.8));
            for k in &keys[1..] {
                probabilities.insert((*k).clone(), json!(0.2 / (keys.len() - 1) as f64));
            }
            json!({"type": "choice", "choice": keys[0], "probabilities": probabilities, "confidence": 0.8})
        }
        _ => {
            json!({"type": "score", "score": 2, "probabilities": {}, "legend": {}, "confidence": 0.7})
        }
    }
}

fn ok_body(request: &Value) -> String {
    let answers: serde_json::Map<String, Value> = request["questions"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, q)| (id.clone(), answer_for(id, q)))
        .collect();
    json!({"model": "jev-1.13.0", "answers": answers, "usage": {"input_tokens": 400, "output_tokens": 12}})
        .to_string()
}

fn ok(request: &Value, _: usize) -> Reply {
    (200, ok_body(request), vec![])
}

fn status(code: u16) -> impl Fn(&Value, usize) -> Reply {
    move |_, _| (code, json!({"error_type": "stub"}).to_string(), vec![])
}

const FAST: Retry = Retry {
    base_delay_ms: 1.0,
    max_delay_ms: 2.0,
    ..Retry::DEFAULT
};

struct Project(PathBuf);
impl Project {
    fn new(stories: &[(&str, &str, &str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "story-judge-jev-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("backlog")).unwrap();
        fs::create_dir_all(root.join(".story-time")).unwrap();
        fs::write(root.join(".story-time/settings.json"), "{}\n").unwrap();
        for (id, frontmatter, story, criteria) in stories {
            fs::write(
                root.join(format!("backlog/{id}.md")),
                format!("---\nid: {id}\n{frontmatter}\n---\n\n## Story\n{story}\n\n## Acceptance Criteria\n{criteria}\n"),
            )
            .unwrap();
        }
        Self(root.canonicalize().unwrap())
    }
    fn args(&self, extra: &[&str]) -> Vec<String> {
        let config = self.0.join(".story-time/settings.json");
        let dir = self.0.join("backlog");
        extra
            .iter()
            .map(|s| s.to_string())
            .chain(["--config".into(), config.display().to_string()])
            .chain(["--dir".into(), dir.display().to_string()])
            .collect()
    }
    fn native(&self, fake: &Fake, extra: &[&str]) -> String {
        let env = Env {
            key: Some(KEY.into()),
            switch: None,
        };
        let (out, code) = judge(&self.args(extra), &env, &fake.url, FAST)
            .unwrap_or_else(|f| panic!("{}", f.message));
        assert_eq!(code, 0);
        out
    }
    /// The Node CLI with `fetch-stub.mjs` preloaded, as `cli-judge.test.mjs` runs it.
    fn node(&self, extra: &[&str], forced: Option<u16>) -> String {
        let scripts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts");
        let mut cmd = Command::new("node");
        cmd.arg("--import")
            .arg(scripts.join("test/fetch-stub.mjs"))
            .arg(scripts.join("story.mjs"))
            .arg("judge")
            .args(self.args(extra))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", &self.0)
            .env("TYPESAFE_API_KEY", KEY)
            .current_dir(&self.0);
        if let Some(code) = forced {
            cmd.env("FAKE_JEV_STATUS", code.to_string());
        }
        let out = cmd.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const GOOD: &str = "As a billing admin, I want invoices exported, so that I can reconcile them.";

fn backlog() -> Project {
    let many: String = (0..9)
        .map(|i| format!("- [ ] Given {i}, when {i}, then {i}\n"))
        .collect();
    Project::new(&[
        (
            "ST-001",
            "title: Export invoices\nstatus: ready\npersona: billing admin",
            GOOD,
            "- [ ] Given an invoice, when I export, then a CSV row is written.\n- [ ] it is fast",
        ),
        (
            "ST-002",
            "title: Many\nstatus: draft",
            "As a planner, I want to plan and share, so that the team knows.",
            &many,
        ),
        (
            "ST-003",
            "title: Research\nstatus: draft\ntype: spike\ntimebox: 1d",
            "",
            "",
        ),
        ("ST-004", "title: Done\nstatus: done", GOOD, ""),
    ])
}

#[test]
fn the_jev_path_prints_what_node_prints_for_the_same_answers() {
    let p = backlog();
    let fake = Fake::new(ok);
    for extra in [
        &[][..],
        &["--json"],
        &["ST-003", "ST-001"],
        &["ST-002", "--json"],
    ] {
        assert_eq!(p.native(&fake, extra), p.node(extra, None), "{extra:?}");
    }
    let out = p.native(&fake, &["ST-001"]);
    assert!(
        out.starts_with("engine: jev (jev-1.13.0)\n\nST-001  Export invoices  [jev jev-1.13.0]\n")
    );
    assert!(out.ends_with("  tokens: 400 in / 12 out, about $0.0000168\n"));
}

#[test]
fn failures_fall_back_to_code_the_way_node_reports_them() {
    let p = backlog();
    for (code, reason) in [
        (503, "overloaded"),
        (401, "auth_rejected"),
        (422, "validation_error"),
        (429, "rate_limited"),
        (404, "protocol_error"),
    ] {
        let fake = Fake::new(status(code));
        for extra in [&["ST-001", "ST-002"][..], &["ST-001", "--json"]] {
            let native = p.native(&fake, extra);
            assert_eq!(native, p.node(extra, Some(code)), "HTTP {code} {extra:?}");
            assert!(native.contains(reason), "HTTP {code}: {native}");
        }
    }
}

#[test]
fn the_request_carries_the_key_the_model_and_only_the_judged_fields() {
    let p = backlog();
    let fake = Fake::new(ok);
    p.native(&fake, &["ST-001"]);
    let requests = fake.requests();
    assert_eq!(requests.len(), 2, "the canary, then one batched request");
    assert!(requests[0].is_canary());
    let head = requests[1].head.to_lowercase();
    assert!(head.starts_with("post /v1/systemone http/1.1\r\n"));
    assert!(head.contains(&format!("authorization: bearer {}\r\n", KEY.to_lowercase())));
    assert!(head.contains("content-type: application/json\r\n"));
    assert!(requests[1].body.starts_with(
        r#"{"state":{"title":"Export invoices","persona":"billing admin","statement":"As a billing admin, I want invoices exported, so that I can reconcile them.","criteria":["Given an invoice, when I export, then a CSV row is written.","it is fast"],"count":2},"model":"jev-1.13.0","questions":{"negotiable":{"type":"noul","instructions":"#
    ));
    let keys: Vec<String> = requests[1].json()["questions"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        [
            "negotiable",
            "valuable",
            "criterion_1",
            "criterion_2",
            "type",
            "size",
            "split_pattern"
        ]
    );
    assert!(
        !requests[1].body.contains("ready"),
        "no frontmatter leaves the machine"
    );
}

#[test]
fn a_story_with_personal_data_never_leaves_the_machine() {
    let p = Project::new(&[
        (
            "ST-001",
            "title: Mail",
            "As a member, I want resets sent to me@example.com, so that I can log in.",
            "",
        ),
        (
            "ST-002",
            "title: Call",
            "As a member, I want a call on 555-123-4567, so that I can log in.",
            "",
        ),
        ("ST-003", "title: Plain", GOOD, ""),
    ]);
    let fake = Fake::new(ok);
    let out = p.native(&fake, &[]);
    assert_eq!(
        out.matches("Jev unavailable (egress_refused)").count(),
        2,
        "{out}"
    );
    let requests = fake.requests();
    assert_eq!(requests.len(), 2, "the canary and ST-003 only");
    assert!(requests
        .iter()
        .all(|r| !r.body.contains("555-123") && !r.body.contains("example.com")));
}

#[test]
fn a_failed_canary_turns_jev_off_for_the_run() {
    let p = backlog();
    let fake = Fake::new(|request, _| {
        let answers: serde_json::Map<String, Value> = request["questions"]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| (k.clone(), json!({"type": "noul", "noul": 0.9})))
            .collect();
        let body = json!({"model": "jev-1.13.0", "answers": answers, "usage": {"input_tokens": 1, "output_tokens": 1}});
        (200, body.to_string(), vec![])
    });
    let out = p.native(&fake, &["ST-001", "ST-002"]);
    assert_eq!(
        out.matches("Jev unavailable (canary_failed)").count(),
        2,
        "{out}"
    );
    assert_eq!(fake.requests().len(), 1);
}

#[test]
fn a_malformed_answer_is_not_trusted() {
    let p = backlog();
    let fake = Fake::new(|request, n| {
        if n == 1 {
            return ok(request, n);
        }
        let mut body: Value = serde_json::from_str(&ok_body(request)).unwrap();
        body["answers"]["type"]["choice"] = json!("epic");
        (200, body.to_string(), vec![])
    });
    assert!(p
        .native(&fake, &["ST-001"])
        .contains("Jev unavailable (protocol_error)"));
}

#[test]
fn an_overload_is_retried_with_the_server_hint_then_answered() {
    let p = backlog();
    let fake = Fake::new(|request, n| match n {
        2 | 3 => (503, String::new(), vec![("retry-after-ms", "0")]),
        _ => ok(request, n),
    });
    let out = p.native(&fake, &["ST-001"]);
    assert!(out.contains("[jev jev-1.13.0]"), "{out}");
    assert_eq!(fake.requests().len(), 4);
}

#[test]
fn a_redirect_is_refused_and_counted_as_a_transport_error() {
    let p = backlog();
    let fake = Fake::new(|_, _| {
        (
            302,
            String::new(),
            vec![("location", "http://127.0.0.1:9/")],
        )
    });
    assert!(p
        .native(&fake, &["ST-001"])
        .contains("Jev unavailable (transport_error)"));
    assert_eq!(fake.requests().len(), 3);
}
