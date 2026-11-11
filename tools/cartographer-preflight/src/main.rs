use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, BufRead, Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CONTEXT: &str = "admin@coffee-cluster-1";
const NAMESPACE: &str = "cartographer";
const LIMIT: usize = 65536;
const TOOL: &str = "cartographer_project_preflight";
const ENV: [&str; 4] = [
    "CARTOGRAPHER_AUTHORITY_URL",
    "CARTOGRAPHER_AUTHORITY_V2_URL",
    "CARTOGRAPHER_ORIGIN",
    "CARTOGRAPHER_ORGANIZATION",
];
const ROLES: [(&str, &str); 4] = [
    ("panel", "cartographer-callback-probe"),
    ("authority_v1", "cartographer-authority"),
    ("authority_v2", "cartographer-authority-v2"),
    ("bridge", "cartographer-bridge"),
];
const METADATA: &str = "{range .items[*]}{.metadata.name}{\"\\t\"}{.spec.replicas}{\"\\t\"}{.status.readyReplicas}{\"\\n\"}{end}";

#[derive(Debug)]
struct Deployment {
    desired: u32,
    ready: u32,
}

fn metadata(raw: &str) -> Result<BTreeMap<String, Deployment>, String> {
    let mut result = BTreeMap::new();
    for line in raw.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 3 || fields[0].is_empty() || fields[0].len() > 253 {
            return Err("malformed deployment projection".into());
        }
        let desired = fields[1].parse().map_err(|_| "malformed desired count")?;
        let ready = if fields[2].is_empty() {
            0
        } else {
            fields[2].parse().map_err(|_| "malformed ready count")?
        };
        if result
            .insert(fields[0].into(), Deployment { desired, ready })
            .is_some()
        {
            return Err("duplicate deployment projection".into());
        }
    }
    Ok(result)
}

fn args(name: Option<&str>, projection: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "--context",
        CONTEXT,
        "--namespace",
        NAMESPACE,
        "--request-timeout=10s",
        "get",
        "deployments",
    ]
    .iter()
    .map(|s| (*s).into())
    .collect();
    if let Some(name) = name {
        args.push(name.into());
    }
    args.extend(["-o".into(), format!("jsonpath={projection}")]);
    args
}

fn env_projection() -> String {
    let mut projection = "{range .spec.template.spec.containers[*]}{.name}".to_string();
    for key in ENV {
        projection.push_str(&format!("{{\"\\t\"}}{{.env[?(@.name==\"{key}\")].value}}"));
    }
    projection.push_str("{\"\\n\"}{end}");
    projection
}

fn safe_value(index: usize, value: &str) -> bool {
    if value.is_empty()
        || value.len() > 512
        || !value.is_ascii()
        || value.bytes().any(|b| b <= 32 || b == 127)
    {
        return false;
    }
    if index == 3 {
        return value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b":._-".contains(&b));
    }
    let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    else {
        return false;
    };
    !rest.is_empty() && !rest.starts_with('/') && !rest.contains(['@', '?', '#', '\\'])
}

fn selected_env(raw: &str, panel: bool) -> Result<Value, String> {
    let mut values: [Vec<String>; 4] = Default::default();
    for line in raw.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 5 || fields[0].is_empty() {
            return Err("malformed selected environment projection".into());
        }
        for (index, value) in fields[1..].iter().enumerate() {
            if !value.is_empty() {
                values[index].push((*value).into());
            }
        }
    }
    let checks: Vec<_> = ENV
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let entries = &values[index];
            let required = panel || index < 2;
            let pass = if entries.is_empty() {
                !required
            } else {
                entries.iter().all(|v| safe_value(index, v))
                    && entries.iter().all(|v| v == &entries[0])
            };
            let observed: Vec<_> = entries
                .iter()
                .map(|v| {
                    if safe_value(index, v) {
                        v.as_str()
                    } else {
                        "[redacted: unsafe value]"
                    }
                })
                .collect();
            json!({"prerequisite":key,"pass":pass,"observed":observed,"required":required,
            "expected":if required { "one nonempty safe literal value, consistent across containers" } else { "absent or one safe literal value, consistent across containers" }})
        })
        .collect();
    Ok(json!({"pass": checks.iter().all(|v| v["pass"] == true), "checks":checks}))
}

fn inspect(mut run: impl FnMut(&[String]) -> Result<String, String>) -> Result<Value, String> {
    let deployments = metadata(&run(&args(None, METADATA))?)?;
    let mut checks = Vec::new();
    for (role, name) in ROLES {
        let deployment = deployments.get(name);
        let pass = deployment.is_some_and(|d| d.desired > 0 && d.ready == d.desired);
        checks.push(json!({"prerequisite":format!("{role}_replicas"),"pass":pass,
            "observed":deployment.map(|d| json!({"deployment":name,"desired":d.desired,"ready":d.ready})),
            "expected":{"deployment":name,"desired":"> 0","ready":"equals desired"}}));
    }
    let mut selected = BTreeMap::new();
    for (role, name) in [ROLES[0], ROLES[3]] {
        if deployments.contains_key(name) {
            let values =
                selected_env(&run(&args(Some(name), &env_projection()))?, role == "panel")?;
            checks.push(json!({"prerequisite":format!("{role}_selected_env"),
                "pass":values["pass"],"observed":values["checks"],
                "expected":"required role-specific literals present; all observed literals safe"}));
            selected.insert(role, name);
        } else {
            checks.push(
                json!({"prerequisite":format!("{role}_selected_env"),"pass":false,
                "observed":null,"expected":"deployment discovered in metadata"}),
            );
        }
    }
    Ok(json!({"context":CONTEXT,"namespace":NAMESPACE,
        "pass":checks.iter().all(|v| v["pass"] == true),
        "checks":checks,"selected_workloads":selected}))
}

fn kubectl(args: &[String]) -> Result<String, String> {
    let mut child = Command::new("kubectl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "kubectl could not start".to_string())?;
    let stdout = child.stdout.take().ok_or("kubectl stdout unavailable")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => break Err("kubectl exceeded 15 second deadline"),
            Err(_) => break Err("kubectl wait failed"),
        }
    };
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let bytes = receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "kubectl stdout deadline exceeded")?
        .map_err(|_| "kubectl stdout read failed")?;
    if !status?.success() {
        return Err("kubectl request failed; stderr suppressed for credential safety".into());
    }
    if bytes.len() > LIMIT {
        return Err("kubectl projection exceeded output limit".into());
    }
    String::from_utf8(bytes).map_err(|_| "kubectl projection is not UTF-8".into())
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn rpc(request: Value, mut run: impl FnMut(&[String]) -> Result<String, String>) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if !request.is_object() || request["jsonrpc"] != "2.0" || !request["method"].is_string() {
        return Some(rpc_error(id, -32600, "Invalid Request"));
    }
    request.get("id")?;
    let result = match request["method"].as_str().unwrap() {
        "initialize" => json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},
            "serverInfo":{"name":"cartographer-preflight","version":env!("CARGO_PKG_VERSION")}}),
        "ping" => json!({}),
        "tools/list" => json!({"tools":[{"name":TOOL,
            "description":"Read-only production prerequisite metadata and four allowlisted literal env values in the fixed Cartographer namespace. No secret resolution or network health probes.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":true}}]}),
        "tools/call" => {
            let params = &request["params"];
            if params["name"] != TOOL {
                return Some(rpc_error(id, -32602, "Unknown tool"));
            }
            if !params.is_object()
                || params
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|k| k != "name" && k != "arguments")
                || !params["arguments"]
                    .as_object()
                    .is_some_and(|v| v.is_empty())
            {
                return Some(rpc_error(
                    id,
                    -32602,
                    "Tool arguments must be an empty object",
                ));
            }
            match inspect(&mut run) {
                Ok(report) => {
                    json!({"content":[{"type":"text","text":report.to_string()}],"isError":false})
                }
                Err(message) => {
                    json!({"content":[{"type":"text","text":json!({"error":message,"kind":"inspection_error"}).to_string()}],"isError":true})
                }
            }
        }
        _ => return Some(rpc_error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn serve(input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut input = input;
    loop {
        let mut bytes = Vec::new();
        let count = input
            .by_ref()
            .take((LIMIT + 1) as u64)
            .read_until(b'\n', &mut bytes)?;
        if count == 0 {
            return Ok(());
        }
        if count > LIMIT {
            writeln!(
                output,
                "{}",
                rpc_error(Value::Null, -32600, "Request exceeds input limit")
            )?;
            output.flush()?;
            return Ok(());
        }
        let reply = match serde_json::from_slice(&bytes) {
            Ok(request) => rpc(request, kubectl),
            Err(_) => Some(rpc_error(Value::Null, -32700, "Parse error")),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let code = match args.as_slice() {
        [mode] if mode == "inspect" => match inspect(kubectl) {
            Ok(report) => {
                println!("{report}");
                if report["pass"] == true {
                    0
                } else {
                    1
                }
            }
            Err(message) => {
                println!("{}", json!({"kind":"inspection_error","error":message}));
                2
            }
        },
        [] => match serve(io::stdin().lock(), io::stdout().lock()) {
            Ok(()) => 0,
            Err(_) => 2,
        },
        [mode] if mode == "stdio" => match serve(io::stdin().lock(), io::stdout().lock()) {
            Ok(()) => 0,
            Err(_) => 2,
        },
        _ => {
            eprintln!("Usage: cartographer-preflight [inspect|stdio]");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployments(v2: u32) -> String {
        format!("cartographer-callback-probe\t1\t1\ncartographer-authority\t1\t1\ncartographer-authority-v2\t{v2}\t{v2}\ncartographer-bridge\t1\t1\n")
    }
    fn env() -> String {
        "app\thttp://cartographer-authority:3000\thttp://cartographer-authority-v2:3000\thttps://cartographer.example.com\tteam-teddy\n".into()
    }
    fn fixture(v2: u32, environment: String) -> Result<Value, String> {
        inspect(|args| {
            Ok(if args.len() == 9 {
                deployments(v2)
            } else {
                environment.clone()
            })
        })
    }
    #[test]
    fn complete_positive() {
        assert_eq!(fixture(1, env()).unwrap()["pass"], true);
    }
    #[test]
    fn v2_scaled_to_zero_is_a_blocker() {
        let report = fixture(0, env()).unwrap();
        assert_eq!(report["pass"], false);
        let checks = report["checks"].as_array().unwrap();
        let v2 = checks
            .iter()
            .find(|check| check["prerequisite"] == "authority_v2_replicas")
            .unwrap();
        assert_eq!(v2["observed"]["desired"], 0);
        assert_eq!(v2["pass"], false);
        assert!(checks
            .iter()
            .filter(|check| check["prerequisite"] != "authority_v2_replicas")
            .all(|check| check["pass"] == true));
    }
    #[test]
    fn missing_selected_env_is_a_blocker() {
        assert_eq!(
            fixture(1, env().replace("team-teddy", "")).unwrap()["pass"],
            false
        );
    }
    #[test]
    fn malformed_output_is_an_error() {
        assert!(inspect(|_| Ok("not a projection".into())).is_err());
        assert!(selected_env("app\tonly-one-value", true).is_err());
    }
    #[test]
    fn bridge_does_not_require_panel_origin_or_organization() {
        let raw = "bridge\thttp://authority\thttp://authority-v2\t\t\n";
        assert_eq!(selected_env(raw, false).unwrap()["pass"], true);
        assert_eq!(selected_env(raw, true).unwrap()["pass"], false);
    }
    #[test]
    fn canonical_organization_is_safe() {
        assert!(safe_value(3, "cartographer:organization:team-teddy"));
    }
    #[test]
    fn transport_failure_is_not_absence() {
        assert_eq!(
            inspect(|_| Err("request failed".into())).unwrap_err(),
            "request failed"
        );
    }
    #[test]
    fn missing_panel_is_not_guessed_ui() {
        let report = inspect(|a| {
            Ok(if a.len() == 9 {
                deployments(1).replace("cartographer-callback-probe", "cartographer-ui")
            } else {
                env()
            })
        })
        .unwrap();
        assert_eq!(report["pass"], false);
        assert!(report["selected_workloads"].get("panel").is_none());
    }
    #[test]
    fn unknown_tool_and_nonempty_args_never_invoke_kubectl() {
        for params in [
            json!({"name":"other","arguments":{}}),
            json!({"name":TOOL,"arguments":{"context":"other"}}),
            json!({"name":TOOL,"arguments":{},"command":"get secrets"}),
        ] {
            let reply = rpc(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":params}),
                |_| panic!("must not invoke kubectl"),
            )
            .unwrap();
            assert_eq!(reply["error"]["code"], -32602);
        }
    }
    #[test]
    fn projections_and_targets_are_narrow_and_fixed() {
        inspect(|a| {
            assert_eq!(
                &a[..7],
                [
                    "--context",
                    CONTEXT,
                    "--namespace",
                    NAMESPACE,
                    "--request-timeout=10s",
                    "get",
                    "deployments"
                ]
            );
            if a.len() == 9 {
                assert_eq!(a[8], format!("jsonpath={METADATA}"));
                Ok(deployments(1))
            } else {
                assert!(
                    ["cartographer-callback-probe", "cartographer-bridge"].contains(&a[7].as_str())
                );
                assert_eq!(a[9], format!("jsonpath={}", env_projection()));
                assert!(!a[9].contains("valueFrom"));
                Ok(env())
            }
        })
        .unwrap();
    }
    #[test]
    fn unsafe_url_values_are_redacted() {
        let raw = env().replace(
            "http://cartographer-authority:3000",
            "https://user:password@example.com/?token=secret",
        );
        let report = fixture(1, raw).unwrap();
        assert_eq!(report["pass"], false);
        assert!(!report.to_string().contains("password"));
        assert!(!report.to_string().contains("token=secret"));
    }
    #[test]
    fn protocol_dispatch_reports_production_blockers_as_data() {
        let request = json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":TOOL,"arguments":{}}});
        let reply = rpc(request, |a| {
            Ok(if a.len() == 9 { deployments(0) } else { env() })
        })
        .unwrap();
        assert_eq!(reply["result"]["isError"], false);
        let report: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(report["pass"], false);
    }
}
