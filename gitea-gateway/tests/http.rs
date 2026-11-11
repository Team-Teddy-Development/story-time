use gitea_gateway::{CreateIssue, Error, Gateway, UpdateIssue};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

fn server(status: &str, body: String) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let status = status.to_owned();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut data = Vec::new();
        let mut buf = [0; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|v| v.parse().ok())
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + length {
                    break;
                }
            }
        }
        write!(
            stream,
            "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status,
            body.len(),
            body
        )
        .unwrap();
        String::from_utf8(data).unwrap()
    });
    (format!("http://{addr}/prefix"), handle)
}
fn issue() -> String {
    r#"{"number":7,"title":"Hello","body":"text","state":"open"}"#.into()
}

#[test]
fn create_update_close_use_expected_http_requests() {
    for operation in 0..3 {
        let (base, handle) = server("200 OK", issue());
        let gateway = Gateway::new(&base, "secret").unwrap();
        let result = match operation {
            0 => gateway.create(
                "owner",
                "repo",
                &CreateIssue {
                    title: "Hello".into(),
                    body: "text".into(),
                },
            ),
            1 => gateway.update(
                "owner",
                "repo",
                7,
                &UpdateIssue {
                    title: Some("Changed".into()),
                    body: None,
                },
            ),
            _ => gateway.close("owner", "repo", 7),
        }
        .unwrap();
        assert_eq!(result.number, 7);
        let request = handle.join().unwrap();
        let expected = if operation == 0 {
            "POST /prefix/api/v1/repos/owner/repo/issues HTTP/1.1"
        } else {
            "PATCH /prefix/api/v1/repos/owner/repo/issues/7 HTTP/1.1"
        };
        assert!(request.starts_with(expected), "{request}");
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: token secret"));
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        match operation {
            0 => assert_eq!(value["title"], "Hello"),
            1 => {
                assert_eq!(value["title"], "Changed");
                assert!(value.get("body").is_none());
            }
            _ => assert_eq!(value["state"], "closed"),
        }
    }
}

#[test]
fn rejects_unsafe_configuration_and_redacts_debug() {
    for base in [
        "http://example.com",
        "http://localhost",
        "https://user:password@example.com",
        "https://example.com/?token=secret",
        "file:///tmp/a",
    ] {
        assert_eq!(
            Gateway::new(base, "secret").unwrap_err(),
            Error::InvalidConfiguration
        );
    }
    assert!(Gateway::new("http://[::1]", "secret").is_ok());
    let gateway = Gateway::new("https://example.com", "secret").unwrap();
    assert_eq!(
        gateway.close("..", "repo", 1),
        Err(Error::InvalidConfiguration)
    );
    assert!(!format!("{gateway:?}").contains("secret"));
    assert_eq!(
        Gateway::new("https://example.com", "bad\nsecret").unwrap_err(),
        Error::InvalidConfiguration
    );
}

#[test]
fn bounded_response_and_sanitized_status() {
    let (base, handle) = server("200 OK", "x".repeat(1024 * 1024 + 1));
    assert_eq!(
        Gateway::new(&base, "secret").unwrap().close("a", "b", 1),
        Err(Error::ResponseTooLarge)
    );
    handle.join().unwrap();
    let (base, handle) = server("302 Found", "secret".into());
    let error = Gateway::new(&base, "secret")
        .unwrap()
        .close("a", "b", 1)
        .unwrap_err();
    assert_eq!(error, Error::HttpStatus(302));
    assert!(!format!("{error:?} {error}").contains("secret"));
    handle.join().unwrap();
}
