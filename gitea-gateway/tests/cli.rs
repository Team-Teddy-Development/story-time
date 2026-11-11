use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const TOKEN: &str = "cli-private-token-92837";

fn invoke(args: &[&str], input: &[u8], token: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gitea-gateway"));
    command.args(args).env_remove("GITEA_TOKEN");
    if let Some(token) = token {
        command.env("GITEA_TOKEN", token);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn response(output: &Output, success: bool) -> Value {
    assert_eq!(output.status.success(), success, "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains(TOKEN));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn server(status: u16, body: String) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("fixture accept failed: {e}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        write!(
            stream,
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (format!("http://{address}/prefix"), handle)
}

#[test]
fn commands_read_stdin_and_send_typed_requests() {
    for operation in ["create", "update", "close"] {
        let state = if operation == "close" {
            "closed"
        } else {
            "open"
        };
        let (base, server) = server(
            200,
            json!({"number":7,"title":"Hello","body":"text","state":state}).to_string(),
        );
        let mut input = json!({"base_url":base,"owner":"owner","repo":"repo"});
        match operation {
            "create" => {
                input["title"] = json!("Hello");
                input["body"] = json!("text");
            }
            "update" => {
                input["number"] = json!(7);
                input["body"] = json!("");
            }
            _ => input["number"] = json!(7),
        }
        let output = invoke(&[operation], input.to_string().as_bytes(), Some(TOKEN));
        let result = response(&output, true);
        assert_eq!(
            result,
            json!({"status":"ok","issue":{"number":7,"title":"Hello","body":"text","state":state}})
        );
        let request = server.join().unwrap();
        let (headers, body) = request.split_once("\r\n\r\n").unwrap();
        let method = if operation == "create" {
            "POST"
        } else {
            "PATCH"
        };
        let suffix = if operation == "create" { "" } else { "/7" };
        assert!(headers.starts_with(&format!(
            "{method} /prefix/api/v1/repos/owner/repo/issues{suffix} HTTP/1.1\r\n"
        )));
        assert!(headers
            .to_ascii_lowercase()
            .contains(&format!("authorization: token {TOKEN}")));
        let payload: Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            payload,
            match operation {
                "create" => json!({"title":"Hello","body":"text"}),
                "update" => json!({"body":""}),
                _ => json!({"state":"closed"}),
            }
        );
    }
}

#[test]
fn rejects_invalid_stdin_without_echoing_input() {
    for (operation, input) in [
        ("create", "".to_owned()),
        ("create", format!("{{{TOKEN}")),
        ("create", json!({"base_url":"https://example.com","owner":"o","repo":"r","title":"x","token":TOKEN}).to_string()),
        ("create", json!({"base_url":"https://example.com","owner":"o","repo":"r","title":"  "}).to_string()),
        ("create", json!({"base_url":"https://example.com","owner":"o","repo":"r","title":"x","number":1}).to_string()),
        ("update", json!({"base_url":"https://example.com","owner":"o","repo":"r","number":1}).to_string()),
        ("update", json!({"base_url":"https://example.com","owner":"o","repo":"r","number":1,"title":null,"body":null}).to_string()),
        ("close", json!({"base_url":"https://example.com","owner":"o","repo":"r","number":0}).to_string()),
        ("close", json!({"base_url":"https://example.com","owner":"o","repo":"r","number":-1}).to_string()),
        ("close", json!({"base_url":"https://example.com","owner":"o","repo":"r","number":1,"body":"x"}).to_string()),
        ("close", "{} {}".to_owned()),
        ("close", "[]".to_owned()),
        ("close", "{\"number\":1,\"number\":2}".to_owned()),
        ("close", String::from_utf8(vec![b'x'; 1024 * 1024 + 1]).unwrap()),
    ] {
        let result = response(&invoke(&[operation], input.as_bytes(), Some(TOKEN)), false);
        let code = if input.len() > 1024 * 1024 { "input_too_large" } else { "invalid_input" };
        assert_eq!(result, json!({"status":"error","code":code}));
    }
    let output = invoke(&["close"], &[0xff], Some(TOKEN));
    assert_eq!(response(&output, false)["code"], "invalid_input");
}

#[test]
fn credentials_are_env_only_and_configuration_errors_are_sanitized() {
    let input =
        json!({"base_url":"https://example.com","owner":"o","repo":"r","number":1}).to_string();
    for token in [None, Some("")] {
        assert_eq!(
            response(&invoke(&["close"], input.as_bytes(), token), false)["code"],
            "missing_token"
        );
    }
    assert_eq!(
        response(
            &invoke(&["close", "--token", TOKEN], b"", Some(TOKEN)),
            false
        )["code"],
        "invalid_command"
    );
    let input = json!({"base_url":format!("https://user:{TOKEN}@example.com"),"owner":"o","repo":"r","number":1}).to_string();
    assert_eq!(
        response(&invoke(&["close"], input.as_bytes(), Some(TOKEN)), false)["code"],
        "invalid_configuration"
    );
    let input =
        json!({"base_url":"https://example.com","owner":"o","repo":"r","number":1}).to_string();
    assert_eq!(
        response(
            &invoke(&["close"], input.as_bytes(), Some("bad\ntoken")),
            false
        )["code"],
        "invalid_configuration"
    );
}

#[test]
fn server_errors_and_reflected_credentials_do_not_leak() {
    for (status, body, expected) in [
        (401, TOKEN.to_owned(), "http_status"),
        (302, TOKEN.to_owned(), "http_status"),
        (200, TOKEN.to_owned(), "invalid_response"),
        (
            200,
            json!({"number":7,"title":TOKEN,"body":format!("prefix {TOKEN}"),"state":TOKEN})
                .to_string(),
            "ok",
        ),
    ] {
        let (base, handle) = server(status, body);
        let input = json!({"base_url":base,"owner":"o","repo":"r","number":7}).to_string();
        let result = response(
            &invoke(&["close"], input.as_bytes(), Some(TOKEN)),
            expected == "ok",
        );
        if expected == "ok" {
            assert_eq!(result["issue"]["title"], "[REDACTED]");
            assert_eq!(result["issue"]["body"], "prefix [REDACTED]");
            assert_eq!(result["issue"]["state"], "[REDACTED]");
        } else {
            assert_eq!(result["code"], expected);
            if expected == "http_status" {
                assert_eq!(result["http_status"], status);
            }
        }
        handle.join().unwrap();
    }
}

#[test]
fn help_and_invalid_command_need_no_token() {
    for flag in ["--help", "-h"] {
        let output = invoke(&[flag], b"", None);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("GITEA_TOKEN"));
    }
    for args in [vec![], vec!["unknown"], vec!["--token", TOKEN]] {
        assert_eq!(
            response(&invoke(&args, b"", None), false)["code"],
            "invalid_command"
        );
    }
}
