use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn real_stdio_lists_tool_and_rejects_unknown_tool_without_cluster_access() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cartographer-preflight"))
        .arg("stdio")
        .env("PATH", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for request in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"unknown","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"cartographer_project_preflight","arguments":{}}}),
        ] {
            writeln!(input, "{request}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 4);
    assert_eq!(
        replies[0]["result"]["serverInfo"]["name"],
        "cartographer-preflight"
    );
    let tool = &replies[1]["result"]["tools"][0];
    assert_eq!(tool["name"], "cartographer_project_preflight");
    assert_eq!(
        tool["inputSchema"],
        json!({"type":"object","properties":{},"additionalProperties":false})
    );
    assert_eq!(replies[2]["error"]["code"], -32602);
    assert_eq!(replies[3]["result"]["isError"], true);
    let error: Value =
        serde_json::from_str(replies[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["kind"], "inspection_error");
    assert_eq!(error["error"], "kubectl could not start");
}

#[test]
fn real_cli_distinguishes_transport_failure_and_rejects_caller_targets() {
    let binary = env!("CARGO_BIN_EXE_cartographer-preflight");
    let output = Command::new(binary)
        .arg("inspect")
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["kind"], "inspection_error");
    assert_eq!(error["error"], "kubectl could not start");
    let output = Command::new(binary)
        .args(["inspect", "--namespace", "other"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Usage: cartographer-preflight [inspect|stdio]\n"
    );
}
