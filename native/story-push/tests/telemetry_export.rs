use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Output},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const FAKE_TOKEN: &str = "ghs_FAKEoidcTOKENfortelemetrytests0123456789";
const BODY_SENTINEL: &str = "SENTINEL-story-body-must-not-leave-the-machine";
const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-telemetry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::write(
            path.join("backlog/ST-001.md"),
            format!(
                "---\nid: ST-001\ntitle: Telemetry fixture\nstatus: ready\nsize: 1\n---\n\
                 ## Story\nAs a user, I want {BODY_SENTINEL}.\n\n\
                 ## Acceptance Criteria\n- [ ] Given A, when B, then C\n"
            ),
        )
        .unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> (Output, Duration) {
        let started = Instant::now();
        let output = Command::new(env!("CARGO_BIN_EXE_story"))
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.0)
            .envs(env.iter().copied())
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
        (output, started.elapsed())
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Request {
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}
impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

enum Mode {
    Accept,
    Hang,
}

/// A one-thread OTLP/HTTP receiver that records each POST and answers 200, or holds the
/// connection open without answering when `Mode::Hang`.
struct Receiver {
    url: String,
    requests: mpsc::Receiver<Request>,
    connections: Arc<AtomicUsize>,
}
impl Receiver {
    fn start(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (sender, requests) = mpsc::channel();
        let connections = Arc::new(AtomicUsize::new(0));
        let counter = connections.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                counter.fetch_add(1, Ordering::SeqCst);
                match mode {
                    Mode::Hang => {
                        thread::spawn(move || {
                            thread::sleep(Duration::from_secs(30));
                            drop(stream);
                        });
                    }
                    Mode::Accept => {
                        if let Some(request) = read_request(&stream) {
                            let mut stream = stream;
                            let _ = stream.write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                            );
                            let _ = sender.send(request);
                        }
                    }
                }
            }
        });
        Self {
            url,
            requests,
            connections,
        }
    }
    fn next(&self) -> Request {
        self.requests
            .recv_timeout(Duration::from_secs(10))
            .expect("the exporter posted one request")
    }
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let path = line.split_whitespace().nth(1)?.to_owned();
    let mut headers = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        let (key, value) = trimmed.split_once(':')?;
        headers.push((key.trim().to_owned(), value.trim().to_owned()));
    }
    let length = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        path,
        headers,
        body,
    })
}

fn traceparent() -> String {
    format!("00-{TRACE_ID}-00f067aa0ba902b7-01")
}

fn auth_header() -> String {
    format!("Authorization=Bearer%20{FAKE_TOKEN}")
}

fn attributes(list: &Value) -> Vec<(String, Value)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let value = item["value"].as_object().unwrap();
            assert_eq!(value.len(), 1, "one typed value per attribute: {item}");
            (
                item["key"].as_str().unwrap().to_owned(),
                value.values().next().unwrap().clone(),
            )
        })
        .collect()
}

fn keys(list: &[(String, Value)]) -> BTreeSet<&str> {
    list.iter().map(|(key, _)| key.as_str()).collect()
}

fn attribute<'a>(list: &'a [(String, Value)], key: &str) -> &'a Value {
    &list.iter().find(|(name, _)| name == key).unwrap().1
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[test]
fn successful_and_failing_commands_post_one_structured_log_event_each() {
    let sandbox = Sandbox::new();
    let receiver = Receiver::start(Mode::Accept);
    let generic = receiver.url.clone() + "/";
    let logs = receiver.url.clone() + "/custom/logs";
    let baseline = sandbox.run(&["show", "ST-001"], &[]).0;

    let before = now_nanos();
    let (ok, _) = sandbox.run(
        &["show", "ST-001"],
        &[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", &generic),
            ("OTEL_EXPORTER_OTLP_HEADERS", &auth_header()),
            ("TRACEPARENT", &traceparent()),
        ],
    );
    let success = receiver.next();
    let (failed, _) = sandbox.run(
        &["show", "ST-999"],
        &[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:9/ignored"),
            ("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT", &logs),
        ],
    );
    let failure = receiver.next();
    let after = now_nanos();

    assert_eq!(ok.stdout, baseline.stdout);
    assert_eq!(ok.stderr, baseline.stderr);
    assert_eq!(ok.status.code(), Some(0));
    assert_ne!(failed.status.code(), Some(0));

    assert_eq!(success.path, "/v1/logs");
    assert_eq!(failure.path, "/custom/logs");
    assert_eq!(success.header("content-type"), Some("application/json"));
    assert_eq!(
        success.header("authorization"),
        Some(format!("Bearer {FAKE_TOKEN}").as_str())
    );
    assert_eq!(failure.header("authorization"), None);

    let expected_resource = BTreeSet::from(["service.name", "service.version"]);
    let expected_record = BTreeSet::from([
        "story.operation",
        "story.outcome",
        "story.exit_code",
        "story.duration_ms",
    ]);
    let mut trace_ids = Vec::new();
    for (request, severity, number, outcome, code) in [
        (&success, "INFO", 9, "success", 0),
        (
            &failure,
            "ERROR",
            17,
            "failure",
            failed.status.code().unwrap(),
        ),
    ] {
        let payload = request.json();
        let resource_logs = payload["resourceLogs"].as_array().unwrap();
        assert_eq!(resource_logs.len(), 1);
        let resource = attributes(&resource_logs[0]["resource"]["attributes"]);
        assert_eq!(keys(&resource), expected_resource);
        assert_eq!(attribute(&resource, "service.name"), "story-time");
        let scopes = resource_logs[0]["scopeLogs"].as_array().unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0]["scope"]["name"], "story-time");
        let records = scopes[0]["logRecords"].as_array().unwrap();
        assert_eq!(records.len(), 1, "one event per command");
        let record = &records[0];
        let time: u128 = record["timeUnixNano"].as_str().unwrap().parse().unwrap();
        assert!(
            before <= time && time <= after,
            "timestamp {time} within the run"
        );
        assert_eq!(record["severityText"], severity);
        assert_eq!(record["severityNumber"], number);
        assert_eq!(
            record["body"]["stringValue"],
            format!("story show {outcome}")
        );
        let fields = attributes(&record["attributes"]);
        assert_eq!(keys(&fields), expected_record);
        assert_eq!(attribute(&fields, "story.operation"), "show");
        assert_eq!(attribute(&fields, "story.outcome"), outcome);
        assert_eq!(attribute(&fields, "story.exit_code"), &code.to_string());
        let duration = attribute(&fields, "story.duration_ms").as_f64().unwrap();
        assert!((0.0..10_000.0).contains(&duration), "duration {duration}");
        let trace_id = record["traceId"].as_str().unwrap().to_owned();
        assert_eq!(trace_id.len(), 32);
        assert!(trace_id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        assert_ne!(trace_id, "0".repeat(32));
        trace_ids.push(trace_id);
    }
    assert_eq!(
        trace_ids[0], TRACE_ID,
        "TRACEPARENT sets the correlation id"
    );
    assert_ne!(
        trace_ids[1], TRACE_ID,
        "without TRACEPARENT each run gets its own id"
    );
}

#[test]
fn credentials_and_story_bodies_never_reach_the_payload() {
    let sandbox = Sandbox::new();
    let receiver = Receiver::start(Mode::Accept);
    let env = [
        ("OTEL_EXPORTER_OTLP_ENDPOINT", receiver.url.as_str()),
        ("OTEL_EXPORTER_OTLP_HEADERS", &auth_header()),
        ("GITHUB_TOKEN", FAKE_TOKEN),
        ("GITEA_TOKEN", FAKE_TOKEN),
    ];
    let cases: [(&[&str], &str); 3] = [
        (&["show", "ST-001"], "show"),
        (&["show", FAKE_TOKEN], "show"),
        (&[FAKE_TOKEN, BODY_SENTINEL], "unknown"),
    ];
    for (args, operation) in cases {
        let (output, _) = sandbox.run(args, &env);
        let request = receiver.next();
        let text = String::from_utf8(request.body.clone()).unwrap();
        assert!(
            !text.contains(FAKE_TOKEN),
            "token leaked for {args:?}: {text}"
        );
        assert!(!text.contains("ghs_"), "token prefix leaked for {args:?}");
        assert!(
            !text.contains(BODY_SENTINEL),
            "story body leaked for {args:?}"
        );
        assert!(!text.contains("Telemetry fixture"), "story title leaked");
        let record = &request.json()["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0];
        let fields = attributes(&record["attributes"]);
        assert_eq!(attribute(&fields, "story.operation"), operation);
        if args == ["show", "ST-001"] {
            assert!(String::from_utf8_lossy(&output.stdout).contains(BODY_SENTINEL));
        }
    }
}

#[test]
fn unset_endpoint_makes_no_connection_and_leaves_output_unchanged() {
    let sandbox = Sandbox::new();
    let receiver = Receiver::start(Mode::Accept);
    let baseline = sandbox.run(&["show", "ST-001"], &[]).0;
    let other_signal = receiver.url.clone() + "/v1/traces";
    for args in [&["show", "ST-001"][..], &["show", "ST-999"][..]] {
        let plain = sandbox.run(args, &[]).0;
        let (output, _) = sandbox.run(
            args,
            &[
                ("OTEL_EXPORTER_OTLP_ENDPOINT", ""),
                ("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT", &other_signal),
                ("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT", &other_signal),
                ("OTEL_EXPORTER_OTLP_HEADERS", &auth_header()),
                ("TRACEPARENT", &traceparent()),
            ],
        );
        assert_eq!(output.stdout, plain.stdout);
        assert_eq!(output.stderr, plain.stderr);
        assert_eq!(output.status.code(), plain.status.code());
    }
    assert_eq!(baseline.status.code(), Some(0));
    thread::sleep(Duration::from_millis(200));
    assert_eq!(receiver.connections.load(Ordering::SeqCst), 0);
}

#[test]
fn unreachable_or_slow_backend_never_changes_the_result_and_is_time_bounded() {
    let sandbox = Sandbox::new();
    let refused = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let slow = Receiver::start(Mode::Hang);
    for args in [&["show", "ST-001"][..], &["show", "ST-999"][..]] {
        let (plain, _) = sandbox.run(args, &[]);
        for endpoint in [refused.as_str(), slow.url.as_str()] {
            let (output, elapsed) = sandbox.run(
                args,
                &[
                    ("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint),
                    ("OTEL_EXPORTER_OTLP_HEADERS", &auth_header()),
                ],
            );
            assert_eq!(output.stdout, plain.stdout, "{endpoint}");
            assert_eq!(output.stderr, plain.stderr, "{endpoint}");
            assert_eq!(output.status.code(), plain.status.code(), "{endpoint}");
            assert!(
                elapsed < Duration::from_millis(3_500),
                "{endpoint} took {elapsed:?}; the export bound is 2 s"
            );
        }
    }
    assert_eq!(
        slow.connections.load(Ordering::SeqCst),
        2,
        "the exporter reached the slow backend and gave up"
    );
}
