include!("../../../tests/empty-push.rs");

use serde_json::{json, Value};

fn native_bin() -> std::ffi::OsString {
    std::env::var_os("STORY_TIME_TEST_BIN").unwrap_or_else(|| env!("CARGO_BIN_EXE_story").into())
}

fn set_config(fixture: &Fixture, value: Value) {
    fs::write(
        fixture.0.join(".story-time/settings.json"),
        value.to_string(),
    )
    .unwrap();
}
fn put_story(fixture: &Fixture, id: &str, status: &str, remote: Option<u64>) {
    let mut value = json!({"id":id,"title":"Native push","status":status,"type":"story","created":"2026-11-11","sections":{"Story":"As a maintainer I need native push.","Acceptance Criteria":"- [ ] Given an empty selection, When planned, Then it succeeds."}});
    if let Some(remote) = remote {
        value["remote"] = json!(remote);
    }
    fs::write(
        fixture.0.join("backlog").join(format!("{id}.md")),
        story_time_native::story::serialize(&value).unwrap(),
    )
    .unwrap();
}
#[test]
fn native_empty_push_runs_without_node_on_path() {
    let f = Fixture::new("ask");
    let output = Command::new(native_bin())
        .args(["push", "--all", "--dry-run", "--json"])
        .current_dir(&f.0)
        .env_clear()
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"ok":true,"plan":[]})
    );
}
#[test]
fn create_update_skip_plan_is_sorted_read_only_and_credential_free() {
    let f = Fixture::new("auto");
    put_story(&f, "ST-003", "done", None);
    put_story(&f, "ST-002", "done", Some(8));
    put_story(&f, "ST-001", "ready", None);
    let before = fs::read(f.0.join("backlog/ST-001.md")).unwrap();
    let output = f.run(&["push", "--all", "--dry-run", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"ok":true,"plan":[{"id":"ST-001","action":"create"},{"id":"ST-002","action":"update","remoteId":"8"},{"id":"ST-003","action":"skip"}]})
    );
    assert_eq!(fs::read(f.0.join("backlog/ST-001.md")).unwrap(), before);
}
#[test]
fn invalid_selection_and_config_fail_before_any_tracker() {
    let f = Fixture::new("ask");
    for (args, code) in [
        (vec!["push"], 2),
        (vec!["push", "--all", "ST-001"], 2),
        (vec!["push", "--all", "--dir"], 2),
        (vec!["push", "../escape", "--dry-run"], 1),
        (vec!["push", "ST-404", "--dry-run"], 1),
        (vec!["push", "--unknown", "x"], 2),
    ] {
        assert_eq!(f.run(&args).status.code(), Some(code), "{args:?}");
    }
    set_config(
        &f,
        json!({"tracker":"gitea","sync":"auto","gitea":{"baseUrl":"http://127.0.0.1:1","owner":"../wrong","repo":"test"}}),
    );
    assert_eq!(
        f.run(&["push", "--all", "--dry-run"]).status.code(),
        Some(1)
    );
}
#[test]
fn all_empty_real_push_is_noop_and_missing_directory_is_empty() {
    let f = Fixture::new("ask");
    fs::remove_dir(f.0.join("backlog")).unwrap();
    let output = f.run(&["push", "--all", "--json"]);
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"ok":true,"pushed":[],"skipped":[]})
    );
    assert!(!f.0.join("backlog").exists());
}
#[test]
fn explicit_and_legacy_config_paths_preserve_relative_backlog() {
    let f = Fixture::new("ask");
    fs::rename(
        f.0.join(".story-time/settings.json"),
        f.0.join(".story-time.json"),
    )
    .unwrap();
    assert!(f.run(&["push", "--all", "--dry-run"]).status.success());
    assert!(f
        .run(&["push", "--all", "--dry-run", "--config", ".story-time.json"])
        .status
        .success());
}

fn read_request(stream: &mut std::net::TcpStream) -> (String, Value) {
    use std::io::Read;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        bytes.push(byte[0]);
    }
    let header = String::from_utf8(bytes).unwrap();
    let length = header
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (header, value)
}
#[test]
fn gitea_cli_create_links_then_update_reuses_issue_without_node() {
    use std::{io::Write, net::TcpListener, thread};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let f = Fixture::new("auto");
    set_config(
        &f,
        json!({"tracker":"gitea","sync":"auto","gitea":{"baseUrl":base,"owner":"acme","repo":"shop"}}),
    );
    put_story(&f, "ST-001", "draft", None);
    let handle = thread::spawn(move || {
        let responses = vec![
            json!([{"name":"story","id":1}]),
            json!([]),
            json!({"number":11}),
            json!({"labels":[{"name":"customer","id":44}]}),
            json!([{"name":"story","id":1}]),
            json!([]),
            json!({}),
            json!({}),
        ];
        let mut requests = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let (header, body) = read_request(&mut stream);
            assert!(header
                .to_ascii_lowercase()
                .contains("authorization: token test-only"));
            let response = response.to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
            requests.push((header.lines().next().unwrap().to_owned(), body));
        }
        requests
    });
    let run = || {
        Command::new(native_bin())
            .args(["push", "ST-001", "--json"])
            .current_dir(&f.0)
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("GITEA_TOKEN", "test-only")
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let linked = fs::read_to_string(f.0.join("backlog/ST-001.md")).unwrap();
    assert!(linked.contains("remote: 11\n"));
    assert!(linked.contains("pushed to tracker as 11"));
    let second = run();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let result: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(result["pushed"][0]["action"], "update");
    let requests = handle.join().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|(line, _)| line.starts_with("POST ") && line.contains("/issues "))
            .count(),
        1
    );
    let (line, body) = requests.last().unwrap();
    assert!(
        line.starts_with("POST /api/v1/repos/acme/shop/issues/11/labels "),
        "{line}"
    );
    assert_eq!(body, &json!({"labels":[1]}));
    assert!(
        !requests.iter().any(|(line, _)| line.starts_with("DELETE ")),
        "the customer label stays on the issue"
    );
}
