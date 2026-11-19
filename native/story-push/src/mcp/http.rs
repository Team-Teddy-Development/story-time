//! `story mcp --http ADDR --clients FILE`: the stdio tools over HTTP (ADR 0007). One `POST /mcp`
//! carries one JSON-RPC message and gets one JSON response; the server offers no event stream.
//! Every request names its own protocol era, so the process holds no protocol state.

use super::{
    caller::{Caller, Carrier, Clients},
    response, RpcError, Server, LEGACY_VERSIONS, LINE_LIMIT, MODERN, SUPPORTED, VERSION_KEY,
};
use crate::Failure;
use serde_json::{json, Value};
use std::{
    io::{Cursor, Read},
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};
use tiny_http::{Header, Method, Request, Response};

pub(super) const USAGE: &str = "[--http ADDR --clients FILE [--allowed-origin ORIGIN]... [--workers N] [--request-timeout-seconds N]]";

pub(super) struct Http {
    address: String,
    clients: Clients,
    origins: Vec<String>,
    workers: usize,
    timeout: Duration,
}

struct Shared {
    server: Server,
    http: Http,
}

type Reply = Response<Cursor<Vec<u8>>>;

/// Splits the HTTP flags off `args`. `--http` needs `--clients`, so the server never listens
/// without authentication.
pub(super) fn launch(args: &[String]) -> Result<(Vec<String>, Option<Http>), Failure> {
    let refuse = |message: String| Failure {
        code: 2,
        message: format!("story mcp: {message}"),
    };
    let mut rest = Vec::new();
    let mut flags = std::collections::HashMap::new();
    let mut origins = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--http"
            | "--clients"
            | "--allowed-origin"
            | "--workers"
            | "--request-timeout-seconds" => {
                let value = args
                    .next()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| refuse(format!("{arg} needs a value.")))?;
                if arg == "--allowed-origin" {
                    origins.push(value.clone());
                }
                flags.insert(&arg[2..], value.clone());
            }
            _ => rest.push(arg.clone()),
        }
    }
    let Some(address) = flags.get("http").cloned() else {
        return match flags.keys().next() {
            Some(flag) => Err(refuse(format!("--{flag} needs --http."))),
            None => Ok((rest, None)),
        };
    };
    let number = |name: &str, default: u64, max: u64| match flags.get(name) {
        None => Ok(default),
        Some(text) => text
            .parse::<u64>()
            .ok()
            .filter(|n| (1..=max).contains(n))
            .ok_or_else(|| refuse(format!("--{name} must be a whole number from 1 to {max}."))),
    };
    let workers = number("workers", 8, 64)? as usize;
    let timeout = Duration::from_secs(number("request-timeout-seconds", 30, 600)?);
    let path = flags
        .get("clients")
        .ok_or_else(|| refuse("--http needs --clients.".into()))?;
    let clients = Clients::load(Path::new(path))
        .map_err(|reason| refuse(format!("--clients {path}: {reason}.")))?;
    Ok((
        rest,
        Some(Http {
            address,
            clients,
            origins,
            workers,
            timeout,
        }),
    ))
}

impl Http {
    /// Refuses a clients file that names an org outside `held`, the orgs of the registry.
    pub(super) fn check_orgs(&self, held: &[&str]) -> Result<(), Failure> {
        match self.clients.unknown_org(held) {
            None => Ok(()),
            Some((client, org)) => Err(Failure {
                code: 2,
                message: format!(
                    "story mcp: --clients: client {client} names the org {org}, which the registry does not hold."
                ),
            }),
        }
    }

    /// Serves until the process ends. A fixed set of workers takes requests from a bounded
    /// queue; a request that finds the queue full gets a retryable 503.
    pub(super) fn serve(self, server: Server) -> Result<(), String> {
        let listener = tiny_http::Server::http(self.address.as_str())
            .map_err(|e| format!("--http {}: {e}", self.address))?;
        let address = listener
            .server_addr()
            .to_ip()
            .ok_or("--http needs an IP address and a port")?;
        let (queue, jobs) = mpsc::sync_channel::<Request>(self.workers * 4);
        let jobs = Arc::new(Mutex::new(jobs));
        let shared = Arc::new(Shared { server, http: self });
        for _ in 0..shared.http.workers {
            let (jobs, shared) = (jobs.clone(), shared.clone());
            thread::spawn(move || loop {
                let job = jobs.lock().map(|jobs| jobs.recv());
                match job {
                    Ok(Ok(request)) => answer(&shared, request),
                    _ => return,
                }
            });
        }
        eprintln!("listening on {address}");
        for request in listener.incoming_requests() {
            if let Err(mpsc::TrySendError::Full(request)) = queue.try_send(request) {
                reply(
                    request,
                    text(503, "busy").with_header(header("Retry-After", "1")),
                );
            }
        }
        Ok(())
    }
}

fn answer(shared: &Arc<Shared>, mut request: Request) {
    let path = request.url().split('?').next().unwrap_or_default();
    let response = match path {
        "/healthz" | "/readyz" if *request.method() == Method::Get => text(200, "ok"),
        "/healthz" | "/readyz" => {
            text(405, "method not allowed").with_header(header("Allow", "GET"))
        }
        "/mcp" => mcp(shared, &mut request, None),
        // `/mcp/<token>`, for a client that cannot send a header. One segment only.
        _ => match path
            .strip_prefix("/mcp/")
            .filter(|token| !token.contains('/'))
        {
            Some(token) => {
                let token = token.to_owned();
                mcp(shared, &mut request, Some(&token))
            }
            None => text(404, "not found"),
        },
    };
    reply(request, response);
}

/// `path_token` is the token of a request to `/mcp/<token>`. Such a request is judged by that
/// token only, and only a client that is marked for it is found. The token goes to no log and
/// to no answer.
fn mcp(shared: &Arc<Shared>, request: &mut Request, path_token: Option<&str>) -> Reply {
    if *request.method() != Method::Post {
        return text(405, "method not allowed").with_header(header("Allow", "POST"));
    }
    let named = |name: &'static str| {
        request
            .headers()
            .iter()
            .find(|h| h.field.equiv(name))
            .map(|h| h.value.as_str().to_owned())
    };
    if named("Origin").is_some_and(|origin| !shared.http.origins.contains(&origin)) {
        return text(403, "origin not allowed");
    }
    let authorization = named("Authorization").unwrap_or_default();
    let presented = match path_token {
        Some(token) => Some((token, Carrier::Path)),
        None => authorization
            .split_once(' ')
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| (token.trim(), Carrier::Header)),
    };
    let Some(caller) =
        presented.and_then(|(token, carrier)| shared.http.clients.find(token, carrier))
    else {
        return text(401, "unauthorized").with_header(header("WWW-Authenticate", "Bearer"));
    };
    let headers = Headers {
        version: named("MCP-Protocol-Version"),
        method: named("Mcp-Method"),
        name: named("Mcp-Name"),
    };
    if request.body_length().is_some_and(|n| n > LINE_LIMIT) {
        return text(413, "request exceeds input limit");
    }
    let mut body = Vec::new();
    let read = request
        .as_reader()
        .take(LINE_LIMIT as u64 + 1)
        .read_to_end(&mut body);
    if read.is_err() {
        return text(400, "unreadable request body");
    }
    if body.len() > LINE_LIMIT {
        return text(413, "request exceeds input limit");
    }
    if let Some(refusal) = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|message| headers.refusal(&message))
    {
        return json_reply(&refusal);
    }
    let mut legacy = headers
        .version
        .as_deref()
        .is_some_and(|v| LEGACY_VERSIONS.contains(&v));
    let (done, result) = mpsc::channel();
    let (worker, caller): (Arc<Shared>, Caller) = (shared.clone(), caller.clone());
    thread::spawn(move || {
        let _ = done.send(worker.server.handle(&caller, &mut legacy, &body));
    });
    match result.recv_timeout(shared.http.timeout) {
        Ok(Some(message)) => json_reply(&message),
        Ok(None) => Response::from_data(Vec::new()).with_status_code(202),
        Err(mpsc::RecvTimeoutError::Timeout) => text(504, "request timed out"),
        Err(mpsc::RecvTimeoutError::Disconnected) => text(500, "internal error"),
    }
}

/// The MCP request headers. A header that is present must agree with the body.
struct Headers {
    version: Option<String>,
    method: Option<String>,
    name: Option<String>,
}

impl Headers {
    /// The JSON-RPC error response for a request whose headers contradict its body, or that
    /// names no supported protocol era. `None` lets the shared dispatch answer.
    fn refusal(&self, message: &Value) -> Option<Value> {
        let method = message.get("method")?.as_str()?;
        let id = message
            .get("id")
            .filter(|id| id.is_string() || id.is_number())?;
        let mismatch = |header: &str| RpcError {
            code: -32020,
            message: "Header mismatch".into(),
            data: Some(json!({ "header": header })),
        };
        let unsupported = |requested: Value| RpcError {
            code: -32022,
            message: "Unsupported protocol version".into(),
            data: Some(json!({"supported": SUPPORTED, "requested": requested})),
        };
        let params = message.get("params");
        let in_body = params
            .and_then(|p| p.get("_meta"))
            .and_then(|m| m.get(VERSION_KEY))
            .and_then(Value::as_str);
        let fault = if self.method.as_deref().is_some_and(|h| h != method) {
            mismatch("Mcp-Method")
        } else if self.name.is_some()
            && self.name.as_deref() != params.and_then(|p| p.get("name")).and_then(Value::as_str)
        {
            mismatch("Mcp-Name")
        } else {
            match (self.version.as_deref(), in_body) {
                (Some(header), Some(body)) if header != body => mismatch("MCP-Protocol-Version"),
                (Some(header), _) if header != MODERN && !LEGACY_VERSIONS.contains(&header) => {
                    unsupported(json!(header))
                }
                (None, None)
                    if matches!(
                        method,
                        "ping" | "server/discover" | "tools/list" | "tools/call"
                    ) =>
                {
                    unsupported(Value::Null)
                }
                _ => return None,
            }
        };
        Some(response(id.clone(), Err(fault)))
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("a static header is valid")
}

fn text(status: u16, body: &str) -> Reply {
    Response::from_data(body.as_bytes().to_vec())
        .with_status_code(status)
        .with_header(header("Content-Type", "text/plain; charset=utf-8"))
}

fn json_reply(message: &Value) -> Reply {
    Response::from_data(message.to_string().into_bytes())
        .with_header(header("Content-Type", "application/json"))
}

fn reply(request: Request, response: Reply) {
    // The client may have closed the connection; there is no one left to tell.
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(version: Option<&str>, method: Option<&str>, name: Option<&str>) -> Headers {
        Headers {
            version: version.map(str::to_owned),
            method: method.map(str::to_owned),
            name: name.map(str::to_owned),
        }
    }

    fn code(headers: &Headers, message: Value) -> Option<i64> {
        headers
            .refusal(&message)
            .map(|response| response["error"]["code"].as_i64().unwrap())
    }

    #[test]
    fn a_present_header_must_agree_with_the_body() {
        let call = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
            "name": "story_show", "_meta": {VERSION_KEY: MODERN}}});
        assert_eq!(code(&headers(None, None, None), call.clone()), None);
        let agreeing = headers(Some(MODERN), Some("tools/call"), Some("story_show"));
        assert_eq!(code(&agreeing, call.clone()), None);
        for wrong in [
            headers(None, Some("tools/list"), None),
            headers(None, None, Some("story_list")),
            headers(Some("2025-11-25"), None, None),
        ] {
            assert_eq!(code(&wrong, call.clone()), Some(-32020));
        }
        let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});
        assert_eq!(
            code(&headers(None, None, Some("story_show")), list),
            Some(-32020)
        );
    }

    #[test]
    fn a_request_that_names_no_supported_era_gets_minus_32022() {
        let list = json!({"jsonrpc": "2.0", "id": "a", "method": "tools/list"});
        assert_eq!(code(&headers(None, None, None), list.clone()), Some(-32022));
        assert_eq!(
            code(&headers(Some("1999-01-01"), None, None), list.clone()),
            Some(-32022)
        );
        assert_eq!(
            code(&headers(Some("2025-06-18"), None, None), list.clone()),
            None
        );
        assert_eq!(code(&headers(Some(MODERN), None, None), list), None);
    }

    #[test]
    fn initialize_notifications_and_malformed_messages_go_to_the_shared_dispatch() {
        let none = headers(None, None, None);
        for message in [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
            json!({"jsonrpc": "2.0", "method": "tools/list"}),
            json!({"jsonrpc": "2.0", "id": 1, "method": "resources/list"}),
            json!({"jsonrpc": "2.0", "id": 1}),
            json!([1, 2]),
        ] {
            assert_eq!(code(&none, message), None);
        }
    }
}
