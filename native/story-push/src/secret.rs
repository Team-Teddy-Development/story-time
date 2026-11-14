//! `story secret set|status|clear <github|ado|gitea|jev>`: one token per tracker in
//! `.story-time/.<name>.env` (mode 0600, git-ignored), as the Node CLI's FileSecretStore keeps it.

use crate::{
    config,
    js_space,
    lenient_story::{line_terminator, trim as js_trim},
    table, Failure,
};
use std::{
    fs,
    io::{IsTerminal, Read, Write},
    path::Path,
    process::{Command, Stdio},
};

struct Provider {
    name: &'static str,
    file: &'static str,
    variable: &'static str,
    label: &'static str,
}

const PROVIDERS: [Provider; 4] = [
    Provider {
        name: "github",
        file: ".github.env",
        variable: "GH_TOKEN",
        label: "GitHub",
    },
    Provider {
        name: "ado",
        file: ".ado.env",
        variable: "AZURE_DEVOPS_EXT_PAT",
        label: "Azure DevOps",
    },
    Provider {
        name: "gitea",
        file: ".gitea.env",
        variable: "GITEA_TOKEN",
        label: "Gitea",
    },
    Provider {
        name: "jev",
        file: ".jev.env",
        variable: "TYPESAFE_API_KEY",
        label: "TypeSafe (Jev)",
    },
];

const GITIGNORE: &str = "# story-time: tracker tokens stay on this machine.\n*.env\n";

fn usage(message: &str) -> Failure {
    Failure {
        code: 2,
        message: message.into(),
    }
}

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = loaded.root.join(".story-time");
    let mut positional = flags.positional.iter().map(String::as_str);
    let (action, name) = (positional.next(), positional.next());
    if action == Some("status") {
        return status(&dir, flags.has("json"));
    }
    let (Some(action @ ("set" | "clear")), Some(name)) = (action, name.filter(|n| !n.is_empty()))
    else {
        return Err(usage(
            "Usage: story secret set|clear <github|ado|gitea|jev>, or story secret status",
        ));
    };
    let provider = PROVIDERS.iter().find(|p| p.name == name).ok_or_else(|| {
        usage(&format!(
            "There is no token support for \"{name}\". Use: github, ado, gitea, jev."
        ))
    })?;
    let path = dir.join(provider.file);
    if action == "clear" {
        return match fs::remove_file(&path) {
            Ok(()) => Ok(format!("Removed the {name} token.\n")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(format!("No {name} token was stored.\n"))
            }
            Err(e) => Err(e.to_string().into()),
        };
    }
    if positional.next().is_some() {
        return Err(usage("Pass the token at the hidden prompt or on stdin, not as an argument (arguments end up in shell history and process lists)."));
    }
    let kind = if name == "jev" { "API key" } else { "token" };
    let token = read_token(&format!(
        "Paste your {} {kind} (input hidden): ",
        provider.label
    ))?;
    if token.is_empty() {
        return Err("No token received.".into());
    }
    if token.chars().any(js_space) {
        return Err("The token contains whitespace; paste only the token.".into());
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    ensure_gitignore(&dir)?;
    let tmp = dir.join(format!("{}.{}.tmp", provider.file, std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(&tmp)
        .and_then(|mut file| {
            write!(
                file,
                "# story-time: tracker token. Never commit this file.\n{}={token}\n",
                provider.variable
            )
        })
        .and_then(|()| fs::rename(&tmp, &path))
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "Saved the {name} token ({}) to .story-time/{} with mode 600. It is git-ignored.\n",
        mask(&token),
        provider.file
    ))
}

fn status(dir: &Path, json: bool) -> Result<String, Failure> {
    let rows: Vec<Vec<String>> = PROVIDERS
        .iter()
        .map(|p| {
            // The file is inspected even when the environment wins: a leaked or loose token file still matters.
            let file = match load(dir, p) {
                Ok(Some(token)) => Some(format!("set ({})  .story-time/{}", mask(&token), p.file)),
                Ok(None) => None,
                Err(message) => Some(format!("unusable: {message}")),
            };
            let env = std::env::var_os(p.variable)
                .filter(|v| !v.is_empty())
                .map(|_| format!("set via {} (environment)", p.variable));
            let state = match (env, file) {
                (Some(env), Some(file)) => format!("{env}; file {file}"),
                (Some(only), None) | (None, Some(only)) => only,
                (None, None) => "not set".into(),
            };
            vec![p.name.to_owned(), state]
        })
        .collect();
    if !json {
        return Ok(table(&["Tracker", "Token"], rows));
    }
    let object: serde_json::Map<_, _> = rows
        .into_iter()
        .map(|mut row| (row.remove(0), row.remove(0).into()))
        .collect();
    let text = serde_json::to_string_pretty(&object).map_err(|e| e.to_string())?;
    Ok(format!("{text}\n"))
}

/// Refuses a group- or world-readable file, or one git tracks, the way ssh refuses a loose private key.
fn load(dir: &Path, provider: &Provider) -> Result<Option<String>, String> {
    let path = dir.join(provider.file);
    let meta = match fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let shown = path.display();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "{shown} has permissions {:o}; tokens must be private. Run: chmod 600 \"{shown}\"",
                mode & 0o777
            ));
        }
    }
    let tracked = Command::new("git")
        .args(["ls-files", "--error-unmatch"])
        .arg(&path)
        .current_dir(dir)
        .output()
        .is_ok_and(|out| out.status.success());
    if tracked {
        return Err(format!(
            "{shown} is committed to git. Treat the token as leaked: rotate it in {}, then run git rm --cached \"{shown}\" and story secret set {}.",
            provider.label, provider.name
        ));
    }
    let text = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(env_value(
        &String::from_utf8_lossy(&text),
        provider.variable,
    ))
}

/// The last `variable=value` line's value with one pair of matching quotes removed; empty counts as unset.
fn env_value(text: &str, variable: &str) -> Option<String> {
    let mut found = None;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let rest = line.trim_start_matches(js_space);
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let key = &rest[..end];
        if key.is_empty() || key.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let Some(value) = rest[end..].trim_start_matches(js_space).strip_prefix('=') else {
            continue;
        };
        let value = js_trim(value);
        if key != variable || value.contains(line_terminator) {
            continue;
        }
        let quoted = value.len() >= 2
            && (value.starts_with('"') || value.starts_with('\''))
            && value.ends_with(&value[..1]);
        found = Some(if quoted {
            &value[1..value.len() - 1]
        } else {
            value
        });
    }
    found.filter(|v| !v.is_empty()).map(str::to_owned)
}

/// Shows only the last four UTF-16 units of a token longer than eight, as JavaScript's `slice(-4)` does.
fn mask(token: &str) -> String {
    let units: Vec<u16> = token.encode_utf16().collect();
    if units.len() > 8 {
        format!("…{}", String::from_utf16_lossy(&units[units.len() - 4..]))
    } else {
        "…".into()
    }
}

fn ensure_gitignore(dir: &Path) -> Result<(), String> {
    let file = dir.join(".gitignore");
    let current = match fs::read(&file) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    if current.split(line_terminator).any(|line| line == "*.env") {
        return Ok(());
    }
    let text = if current.is_empty() {
        GITIGNORE.to_owned()
    } else {
        format!("{}\n*.env\n", current.trim_end_matches(js_space))
    };
    fs::write(&file, text).map_err(|e| e.to_string())
}

/// The first line of piped stdin, or a line typed at a hidden prompt on a terminal.
fn read_token(prompt: &str) -> Result<String, Failure> {
    let mut stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let mut bytes = Vec::new();
        stdin.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&bytes);
        let line = text.split('\n').next().unwrap_or_default();
        return Ok(js_trim(line.strip_suffix('\r').unwrap_or(line)).to_owned());
    }
    eprint!("{prompt}");
    let saved = stty(&["-g"])?;
    stty(&["raw", "-echo"])?;
    let typed = read_hidden(&mut stdin);
    let restored = stty(&[saved.trim()]);
    eprintln!();
    restored?;
    typed
}

fn stty(args: &[&str]) -> Result<String, Failure> {
    let out = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .output()
        .map_err(|_| "Could not hide terminal input; pipe the token on stdin instead.")?;
    if !out.status.success() {
        return Err("Could not hide terminal input; pipe the token on stdin instead.".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn read_hidden(stdin: &mut std::io::Stdin) -> Result<String, Failure> {
    let mut typed: Vec<u8> = Vec::new();
    for byte in stdin.lock().bytes() {
        match byte.map_err(|e| e.to_string())? {
            b'\r' | b'\n' => break,
            0x03 => return Err("Cancelled.".into()),
            0x7f | 0x08 => while typed.pop().is_some_and(|b| b & 0xc0 == 0x80) {},
            b => typed.push(b),
        }
    }
    Ok(js_trim(&String::from_utf8_lossy(&typed)).to_owned())
}
