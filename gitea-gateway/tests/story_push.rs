use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const STORY: &str = include_str!("fixtures/ready.md");
const TOKEN: &str = "story-private-token-481927";

fn project(base: &str) -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".story-time")).unwrap();
    fs::create_dir(root.path().join("backlog")).unwrap();
    let mut settings: Value = serde_json::from_str(include_str!("fixtures/settings.json")).unwrap();
    settings["gitea"]["baseUrl"] = json!(base);
    fs::write(
        root.path().join(".story-time/settings.json"),
        settings.to_string(),
    )
    .unwrap();
    fs::write(root.path().join("backlog/ST-001.md"), STORY).unwrap();
    root
}

fn invoke(root: &TempDir, args: &[&str], token: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gitea-gateway"));
    cmd.arg("story-push")
        .arg("--root")
        .arg(root.path())
        .args(args)
        .env_remove("GITEA_TOKEN");
    if let Some(token) = token {
        cmd.env("GITEA_TOKEN", token);
    }
    cmd.output().unwrap()
}

fn result(output: Output, success: bool) -> Value {
    assert_eq!(output.status.success(), success, "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains(TOKEN));
    serde_json::from_slice(&output.stdout).unwrap()
}

type Requests = thread::JoinHandle<Vec<(String, Value)>>;

fn fixture(responses: Vec<(u16, Value)>) -> (String, Requests) {
    fixture_with_hook(responses, || {})
}

fn fixture_with_hook(
    responses: Vec<(u16, Value)>,
    before_response: impl Fn() + Send + 'static,
) -> (String, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}/prefix", listener.local_addr().unwrap());
    let response_base = base.clone();
    let handle = thread::spawn(move || {
        responses.into_iter().map(|(status, mut body)| {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                    Err(e) => panic!("fixture accept failed: {e}"),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let length: usize = headers.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().unwrap())
                    }).unwrap();
                    if bytes.len() >= end + 4 + length { break; }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            let (headers, payload) = request.split_once("\r\n\r\n").unwrap();
            assert!(headers.to_ascii_lowercase().contains(&format!("authorization: token {TOKEN}")));
            if body["html_url"] == "$target" { body["html_url"] = json!(format!("{response_base}/owner/repo/issues/7")); }
            let body = body.to_string();
            before_response();
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            (headers.lines().next().unwrap().to_string(), serde_json::from_str(payload).unwrap())
        }).collect()
    });
    (base, handle)
}

fn issue() -> Value {
    json!({"number":7, "html_url":"$target"})
}

#[test]
fn repeat_push_creates_then_updates_and_preserves_local_text() {
    let (base, server) = fixture(vec![(201, issue()), (200, issue())]);
    let root = project(&base);
    let created = result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), true);
    assert_eq!(created["plan"][0]["action"], "create");
    assert_eq!(created["plan"][0]["completed"], true);
    let linked = fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap();
    assert!(linked.contains("remote: 7\n"));
    assert!(linked.contains(&format!("remote_url: \"{base}/owner/repo/issues/7\"")));
    assert_eq!(
        linked.replace(
            &format!("remote: 7\nremote_url: \"{base}/owner/repo/issues/7\"\n"),
            ""
        ),
        STORY
    );
    let edited = linked
        .replace("Reset password: email", "Reset password: secure email")
        .replace(
            "then I receive one link.",
            "then I receive one expiring link.",
        );
    fs::write(root.path().join("backlog/ST-001.md"), &edited).unwrap();
    let updated = result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), true);
    assert_eq!(updated["plan"][0]["action"], "update");
    assert_eq!(
        fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap(),
        edited
    );
    let requests = server.join().unwrap();
    assert_eq!(
        requests[0].0,
        "POST /prefix/api/v1/repos/owner/repo/issues HTTP/1.1"
    );
    assert_eq!(
        requests[1].0,
        "PATCH /prefix/api/v1/repos/owner/repo/issues/7 HTTP/1.1"
    );
    assert_eq!(requests[1].1["state"], "open");
    assert_eq!(requests[1].1["title"], "Reset password: secure email");
    assert!(requests[1].1["body"]
        .as_str()
        .unwrap()
        .contains("then I receive one expiring link."));
    assert!(requests[0].1.get("state").is_none());
    assert_eq!(requests[0].1["title"], "Reset password: email");
    let body = requests[0].1["body"].as_str().unwrap();
    assert!(body.contains("### Acceptance Criteria\n- [ ] Given an email"));
    assert!(body.contains("### Notes\nUses existing mailer."));
    assert!(body.contains("<sub>story-time ST-001 · WSJF 5.33</sub>"));
    assert!(!body.contains("Local test commands"));
    assert!(!body.contains("2026-11-01 created"));
    assert_eq!(
        created["plan"][0]["labels_deferred"],
        json!(["story", "size:3", "value:8", "epic:EP-1"])
    );
    assert!(!created["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn terminal_unlinked_skip_and_linked_terminal_close_then_active_reopen() {
    for state in ["done", "split", "archived"] {
        let root = project("https://gitea.example");
        let source = STORY.replace("status: ready", &format!("status: {state}"));
        fs::write(root.path().join("backlog/ST-001.md"), &source).unwrap();
        let output = result(invoke(&root, &["backlog/ST-001.md"], None), true);
        assert_eq!(output["plan"][0]["action"], "skip");
        assert_eq!(
            fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap(),
            source
        );
    }
    let (base, server) = fixture(vec![(200, issue()), (200, issue())]);
    let root = project(&base);
    let linked = STORY.replace("status: ready", "status: done").replacen(
        "---\n",
        &format!("---\nremote: 7\nremote_url: '{base}/owner/repo/issues/7'\n"),
        1,
    );
    fs::write(root.path().join("backlog/ST-001.md"), linked).unwrap();
    result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), true);
    let closed = fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap();
    fs::write(
        root.path().join("backlog/ST-001.md"),
        closed.replace("status: done", "status: ready"),
    )
    .unwrap();
    result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), true);
    let requests = server.join().unwrap();
    assert_eq!(requests[0].1["state"], "closed");
    assert_eq!(requests[1].1["state"], "open");
}

#[test]
fn dry_run_needs_no_token_network_or_writes_even_when_sync_off() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = project(&format!("http://{}", listener.local_addr().unwrap()));
    let settings_path = root.path().join(".story-time/settings.json");
    let config = fs::read_to_string(&settings_path)
        .unwrap()
        .replace("ask", "off");
    fs::write(&settings_path, &config).unwrap();
    let output = result(
        invoke(&root, &["--dry-run", "backlog/ST-001.md"], None),
        true,
    );
    assert_eq!(output["plan"][0]["action"], "create");
    assert!(output["plan"][0]["body"]
        .as_str()
        .unwrap()
        .contains("Acceptance Criteria"));
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    assert_eq!(
        fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap(),
        STORY
    );
    assert_eq!(fs::read_to_string(settings_path).unwrap(), config);
    assert_eq!(
        result(invoke(&root, &["backlog/ST-001.md"], None), false)["code"],
        "sync_off"
    );
}

#[test]
fn validates_all_files_configuration_and_links_before_any_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = project(&format!("http://{}", listener.local_addr().unwrap()));
    for invalid in [
        "no frontmatter",
        "---\nid: x\nstatus: broken\ntitle: x\n---\n",
        "---\nid: ST-002\ntitle: x\nstatus: ready\nremote: bad\n---\n",
    ] {
        fs::write(root.path().join("backlog/invalid.md"), invalid).unwrap();
        assert_eq!(
            result(
                invoke(
                    &root,
                    &["backlog/ST-001.md", "backlog/invalid.md"],
                    Some(TOKEN)
                ),
                false
            )["code"],
            "invalid_story"
        );
    }
    for args in [
        vec!["backlog/ST-001.md", "missing.md"],
        vec!["../outside.md"],
        vec!["backlog/ST-001.md", "backlog/ST-001.md"],
        vec![],
    ] {
        result(invoke(&root, &args, Some(TOKEN)), false);
    }
    let wrong = STORY.replacen(
        "---\n",
        "---\nremote: 7\nremote_url: https://other.example/owner/repo/issues/7\n",
        1,
    );
    fs::write(root.path().join("backlog/ST-001.md"), wrong).unwrap();
    assert_eq!(
        result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), false)["code"],
        "target_mismatch"
    );
    fs::write(root.path().join("backlog/ST-001.md"), STORY).unwrap();
    let settings = root.path().join(".story-time/settings.json");
    let original = fs::read_to_string(&settings).unwrap();
    for config in [
        "[]".to_owned(),
        original.replace("\"gitea\"", "\"github\""),
        original.replace("\"ask\"", "\"invalid\""),
        original.replace("owner", "../owner"),
        original.replace("http://127.0.0.1", "http://example.com"),
    ] {
        fs::write(&settings, config).unwrap();
        assert_eq!(
            result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), false)["code"],
            "invalid_configuration"
        );
    }
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
}

#[test]
fn network_failures_and_wrong_response_targets_never_modify_local_links() {
    for (status, body, expected) in [
        (401, json!({"message":TOKEN}), "http_status"),
        (302, issue(), "http_status"),
        (
            200,
            json!({"number":0, "html_url":"$target"}),
            "target_mismatch",
        ),
        (
            200,
            json!({"number":7, "html_url":"https://other.example/owner/repo/issues/7"}),
            "target_mismatch",
        ),
        (200, json!({"number":7}), "invalid_response"),
        (
            200,
            json!({"number":7, "html_url":format!("https://{TOKEN}.example/owner/repo/issues/7")}),
            "target_mismatch",
        ),
    ] {
        let (base, server) = fixture(vec![(status, body)]);
        let root = project(&base);
        let output = result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), false);
        assert_eq!(output["code"], expected);
        assert_eq!(
            fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap(),
            STORY
        );
        server.join().unwrap();
    }
}

#[test]
fn existing_issue_number_must_match_response() {
    let (base, server) = fixture(vec![(200, issue())]);
    let root = project(&base);
    let original = STORY.replacen(
        "---\n",
        &format!("---\nremote: 8\nremote_url: {base}/owner/repo/issues/8\n"),
        1,
    );
    fs::write(root.path().join("backlog/ST-001.md"), &original).unwrap();
    assert_eq!(
        result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), false)["code"],
        "target_mismatch"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap(),
        original
    );
    server.join().unwrap();
}

#[test]
fn crlf_yaml_lists_and_numeric_string_links_survive_updates() {
    let (base, server) = fixture(vec![(200, issue())]);
    let root = project(&base);
    let original = STORY
        .replacen(
            "---\n",
            &format!("---\nremote: '7'\nremote_url: '{base}/owner/repo/issues/7'\n"),
            1,
        )
        .replace('\n', "\r\n");
    fs::write(root.path().join("backlog/ST-001.md"), &original).unwrap();
    result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), true);
    let updated = fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap();
    assert!(updated.contains("remote: 7\r\n"));
    assert!(updated.contains("depends_on:\r\n  - ST-000\r\n"));
    assert_eq!(
        updated.split("\r\n---\r\n").nth(1),
        original.split("\r\n---\r\n").nth(1)
    );
    server.join().unwrap();
}

#[test]
fn changed_local_file_is_not_overwritten_and_remote_identity_is_reported() {
    let (sender, receiver) = std::sync::mpsc::channel::<std::path::PathBuf>();
    let receiver = std::sync::Mutex::new(receiver);
    let (base, server) = fixture_with_hook(vec![(201, issue())], move || {
        let path = receiver.lock().unwrap().recv().unwrap();
        fs::write(
            path,
            STORY.replace("Uses existing mailer.", "Concurrent edit."),
        )
        .unwrap();
    });
    let root = project(&base);
    sender.send(root.path().join("backlog/ST-001.md")).unwrap();
    let output = result(invoke(&root, &["backlog/ST-001.md"], Some(TOKEN)), false);
    assert_eq!(output["code"], "story_changed");
    assert_eq!(output["plan"][0]["remote"], 7);
    assert_eq!(output["plan"][0]["completed"], false);
    assert_eq!(
        fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap(),
        STORY.replace("Uses existing mailer.", "Concurrent edit.")
    );
    server.join().unwrap();
}

#[test]
fn partial_batch_failure_retains_successful_links_and_reports_progress() {
    let (base, server) = fixture(vec![(201, issue()), (401, json!({"message":TOKEN}))]);
    let root = project(&base);
    let second = STORY.replace("ST-001", "ST-002");
    fs::write(root.path().join("backlog/ST-002.md"), &second).unwrap();
    let output = result(
        invoke(
            &root,
            &["backlog/ST-001.md", "backlog/ST-002.md"],
            Some(TOKEN),
        ),
        false,
    );
    assert_eq!(output["code"], "http_status");
    assert_eq!(output["plan"][0]["completed"], true);
    assert_eq!(output["plan"][1]["completed"], false);
    assert!(fs::read_to_string(root.path().join("backlog/ST-001.md"))
        .unwrap()
        .contains("remote: 7\n"));
    assert_eq!(
        fs::read_to_string(root.path().join("backlog/ST-002.md")).unwrap(),
        second
    );
    server.join().unwrap();
}

#[test]
fn spike_fixture_renders_question_timebox_and_exit_criteria() {
    let (base, server) = fixture(vec![(201, issue())]);
    let root = project(&base);
    let spike = include_str!("fixtures/spike.md");
    fs::write(root.path().join("backlog/ST-003.md"), spike).unwrap();
    result(invoke(&root, &["backlog/ST-003.md"], Some(TOKEN)), true);
    let requests = server.join().unwrap();
    let body = requests[0].1["body"].as_str().unwrap();
    assert!(body.contains("### Question\nCan the existing store"));
    assert!(body.contains("Timebox: 2d"));
    assert!(body.contains("### Exit Criteria\n- [ ] Report measured expiry latency"));
    assert!(!body.contains("Acceptance Criteria"));
    assert!(!body.contains("local test instructions"));
    assert!(fs::read_to_string(root.path().join("backlog/ST-003.md"))
        .unwrap()
        .contains("remote: 7\n"));
}

#[test]
fn missing_credentials_and_invalid_arguments_are_sanitized() {
    let root = project("https://gitea.example");
    assert_eq!(
        result(invoke(&root, &["backlog/ST-001.md"], None), false)["code"],
        "missing_token"
    );
    assert_eq!(
        result(invoke(&root, &["--token", TOKEN], None), false)["code"],
        "invalid_arguments"
    );
    assert_eq!(
        result(invoke(&root, &["--root"], None), false)["code"],
        "invalid_arguments"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("backlog/ST-001.md")).unwrap(),
        STORY
    );
}

#[cfg(unix)]
#[test]
fn story_symlink_cannot_escape_project_root() {
    let root = project("https://gitea.example");
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("outside.md"), STORY).unwrap();
    std::os::unix::fs::symlink(
        external.path().join("outside.md"),
        root.path().join("backlog/escape.md"),
    )
    .unwrap();
    assert_eq!(
        result(
            invoke(&root, &["--dry-run", "backlog/escape.md"], None),
            false
        )["code"],
        "invalid_path"
    );
}
