use gitea_gateway::{CreateIssue, Error, Gateway, Issue, UpdateIssue};
use serde::{Deserialize, Serialize};
use std::{
    env,
    io::{self, Read, Write},
    process::ExitCode,
};

const MAX_INPUT: u64 = 1024 * 1024;
const HELP: &str = "Gitea issue gateway\n\nUsage: gitea-gateway <create|update|close>\n\nRead one JSON object from stdin. Set credentials only through GITEA_TOKEN.\nAll requests require base_url, owner, and repo.\ncreate: title, optional body (default empty).\nupdate: number, optional title and body (at least one required).\nclose: number.\nResponses: {\"status\":\"ok\",\"issue\":{...}} or {\"status\":\"error\",\"code\":\"...\"}.\nSee docs/gitea-cli.md for examples and error codes.\n";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    base_url: String,
    owner: String,
    repo: String,
    title: String,
    #[serde(default)]
    body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    base_url: String,
    owner: String,
    repo: String,
    number: u64,
    title: Option<String>,
    body: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Close {
    base_url: String,
    owner: String,
    repo: String,
    number: u64,
}

enum Request {
    Create(Create),
    Update(Update),
    Close(Close),
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Output {
    Ok {
        issue: Issue,
    },
    Error {
        code: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        http_status: Option<u16>,
    },
}

impl Output {
    fn error(code: &'static str) -> Self {
        Self::Error {
            code,
            http_status: None,
        }
    }
}

fn parse(command: &str, input: &[u8]) -> Result<Request, Output> {
    let invalid = |_| Output::error("invalid_input");
    match command {
        "create" => {
            let value: Create = serde_json::from_slice(input).map_err(invalid)?;
            if value.title.trim().is_empty() {
                return Err(Output::error("invalid_input"));
            }
            Ok(Request::Create(value))
        }
        "update" => {
            let value: Update = serde_json::from_slice(input).map_err(invalid)?;
            if value.number == 0
                || (value.title.is_none() && value.body.is_none())
                || value.title.as_ref().is_some_and(|s| s.trim().is_empty())
            {
                return Err(Output::error("invalid_input"));
            }
            Ok(Request::Update(value))
        }
        "close" => {
            let value: Close = serde_json::from_slice(input).map_err(invalid)?;
            if value.number == 0 {
                return Err(Output::error("invalid_input"));
            }
            Ok(Request::Close(value))
        }
        _ => Err(Output::error("invalid_command")),
    }
}

fn execute(request: Request, token: &str) -> Result<Issue, Error> {
    match request {
        Request::Create(r) => Gateway::new(&r.base_url, token)?.create(
            &r.owner,
            &r.repo,
            &CreateIssue {
                title: r.title,
                body: r.body,
            },
        ),
        Request::Update(r) => Gateway::new(&r.base_url, token)?.update(
            &r.owner,
            &r.repo,
            r.number,
            &UpdateIssue {
                title: r.title,
                body: r.body,
            },
        ),
        Request::Close(r) => Gateway::new(&r.base_url, token)?.close(&r.owner, &r.repo, r.number),
    }
}

fn run(command: &str) -> Output {
    let mut input = Vec::new();
    if io::stdin()
        .take(MAX_INPUT + 1)
        .read_to_end(&mut input)
        .is_err()
    {
        return Output::error("input_io");
    }
    if input.len() as u64 > MAX_INPUT {
        return Output::error("input_too_large");
    }
    let request = match parse(command, &input) {
        Ok(request) => request,
        Err(error) => return error,
    };
    let token = match env::var("GITEA_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => return Output::error("missing_token"),
    };
    match execute(request, &token) {
        Ok(mut issue) => {
            issue.title = issue.title.replace(&token, "[REDACTED]");
            issue.body = issue.body.map(|s| s.replace(&token, "[REDACTED]"));
            issue.state = issue.state.replace(&token, "[REDACTED]");
            Output::Ok { issue }
        }
        Err(error) => match error {
            Error::InvalidConfiguration => Output::error("invalid_configuration"),
            Error::Transport => Output::error("transport"),
            Error::HttpStatus(status) => Output::Error {
                code: "http_status",
                http_status: Some(status),
            },
            Error::ResponseTooLarge => Output::error("response_too_large"),
            Error::InvalidResponse => Output::error("invalid_response"),
        },
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return if io::stdout().lock().write_all(HELP.as_bytes()).is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let output = match args.as_slice() {
        [command] if matches!(command.to_str(), Some("create" | "update" | "close")) => {
            run(command.to_str().unwrap())
        }
        _ => Output::error("invalid_command"),
    };
    let success = matches!(output, Output::Ok { .. });
    let mut stdout = io::stdout().lock();
    if serde_json::to_writer(&mut stdout, &output).is_err() || writeln!(stdout).is_err() {
        return ExitCode::FAILURE;
    }
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
