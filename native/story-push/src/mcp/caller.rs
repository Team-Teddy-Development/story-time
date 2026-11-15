//! Who makes a request (ADR 0007). The stdio server has one local caller that the process flags
//! govern. The HTTP server reads its clients from a TOML file that holds a SHA-256 hash for each
//! token, never a token.

use crate::{adoption::sha, toml_subset};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Scope {
    Read,
    Write,
    Comment,
}

#[derive(Clone, Debug)]
pub(super) struct Caller {
    /// `None` for the local stdio caller.
    pub(super) client_id: Option<String>,
    scopes: Vec<Scope>,
    /// Story prefixes that the write and comment scopes cover; empty covers every prefix.
    prefixes: Vec<String>,
}

impl Caller {
    pub(super) fn local() -> Self {
        Self {
            client_id: None,
            scopes: vec![Scope::Read, Scope::Write, Scope::Comment],
            prefixes: Vec::new(),
        }
    }

    pub(super) fn has(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }

    /// Whether the caller may use `scope` on a story with `prefix`. Reads cover the whole
    /// server. A caller limited to prefixes is refused when the target has no known prefix.
    pub(super) fn permits(&self, scope: Scope, prefix: Option<&str>) -> bool {
        if !self.has(scope) {
            return false;
        }
        if scope == Scope::Read || self.prefixes.is_empty() {
            return true;
        }
        prefix.is_some_and(|p| self.prefixes.iter().any(|allowed| allowed == p))
    }
}

pub(super) struct Clients(Vec<(String, Caller)>);

impl Clients {
    pub(super) fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::parse(&text)
    }

    fn parse(text: &str) -> Result<Self, String> {
        let document = toml_subset::parse(text)?;
        let entries = document
            .get("client")
            .and_then(Value::as_array)
            .filter(|entries| !entries.is_empty())
            .ok_or("it needs at least one [[client]] table")?;
        let mut clients: Vec<(String, Caller)> = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let at = format!("client {}", index + 1);
            let text = |key: &str| {
                entry
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("{at}: {key} must be a non-empty string"))
            };
            let list = |key: &str| -> Result<Vec<&str>, String> {
                match entry.get(key) {
                    None => Ok(Vec::new()),
                    Some(Value::Array(items)) => items
                        .iter()
                        .map(|item| item.as_str().filter(|s| !s.is_empty()))
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| format!("{at}: {key} must hold non-empty strings")),
                    Some(_) => Err(format!("{at}: {key} must be an array")),
                }
            };
            let id = text("id")?;
            let hash = text("token_sha256")?;
            if hash.len() != 64 || !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
                return Err(format!(
                    "{at}: token_sha256 must be 64 lowercase hexadecimal digits"
                ));
            }
            let mut scopes = Vec::new();
            for name in list("scopes")? {
                let scope = match name {
                    "read" => Scope::Read,
                    "write" => Scope::Write,
                    "comment" => Scope::Comment,
                    other => return Err(format!("{at}: unknown scope {other}")),
                };
                if !scopes.contains(&scope) {
                    scopes.push(scope);
                }
            }
            if scopes.is_empty() {
                return Err(format!("{at}: scopes must name at least one scope"));
            }
            if clients
                .iter()
                .any(|(h, c)| h == hash || c.client_id.as_deref() == Some(id))
            {
                return Err(format!("{at}: its id or its token_sha256 is already used"));
            }
            let prefixes = list("prefixes")?.into_iter().map(str::to_owned).collect();
            clients.push((
                hash.to_owned(),
                Caller {
                    client_id: Some(id.to_owned()),
                    scopes,
                    prefixes,
                },
            ));
        }
        Ok(Self(clients))
    }

    /// The client whose token hash equals the hash of `token`. Each comparison reads every
    /// byte, and every client is compared, so the time does not depend on where a hash differs.
    pub(super) fn find(&self, token: &str) -> Option<&Caller> {
        let presented = sha(token.as_bytes());
        let mut found = None;
        for (hash, caller) in &self.0 {
            let difference = hash
                .bytes()
                .zip(presented.bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b));
            if difference == 0 {
                found = Some(caller);
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(body: &str) -> String {
        format!(
            "[[client]]\nid = \"a\"\ntoken_sha256 = \"{}\"\n{body}",
            sha(b"token-a")
        )
    }

    #[test]
    fn a_client_is_found_by_its_token_and_by_no_other_text() {
        let clients = Clients::parse(&file("scopes = [\"read\"]\n")).unwrap();
        let caller = clients.find("token-a").unwrap();
        assert_eq!(caller.client_id.as_deref(), Some("a"));
        assert!(clients.find("token-b").is_none());
        assert!(clients.find("").is_none());
        assert!(
            clients.find(&sha(b"token-a")).is_none(),
            "the hash is not a token"
        );
    }

    #[test]
    fn scopes_and_prefixes_limit_what_a_caller_may_do() {
        let clients = Clients::parse(&file(
            "scopes = [\"read\", \"write\"]\nprefixes = [\"VC\"]\n",
        ))
        .unwrap();
        let caller = clients.find("token-a").unwrap();
        assert!(caller.permits(Scope::Read, None));
        assert!(caller.permits(Scope::Read, Some("VE")));
        assert!(caller.permits(Scope::Write, Some("VC")));
        assert!(!caller.permits(Scope::Write, Some("VE")));
        assert!(!caller.permits(Scope::Write, None));
        assert!(!caller.permits(Scope::Comment, Some("VC")));

        let open = Clients::parse(&file("scopes = [\"write\"]\n")).unwrap();
        let caller = open.find("token-a").unwrap();
        assert!(caller.permits(Scope::Write, Some("VE")));
        assert!(caller.permits(Scope::Write, None));
        assert!(!caller.permits(Scope::Read, None));

        let local = Caller::local();
        assert!(local.permits(Scope::Write, None) && local.permits(Scope::Comment, Some("X")));
    }

    #[test]
    fn a_malformed_clients_file_is_refused_with_the_entry_named() {
        let second = format!(
            "{}\n[[client]]\nid = \"a\"\ntoken_sha256 = \"{}\"\nscopes = [\"read\"]\n",
            file("scopes = [\"read\"]\n"),
            sha(b"token-b")
        );
        for (text, reason) in [
            (String::new(), "at least one [[client]]"),
            ("[[client]]\nid = \"a\"\n".into(), "client 1: token_sha256"),
            (
                "[[client]]\nid = \"a\"\ntoken_sha256 = \"ABC\"\nscopes = [\"read\"]\n".into(),
                "64 lowercase hexadecimal",
            ),
            (file(""), "client 1: scopes must name"),
            (file("scopes = [\"admin\"]\n"), "unknown scope admin"),
            (file("scopes = \"read\"\n"), "scopes must be an array"),
            (second, "client 2: its id or its token_sha256"),
        ] {
            let error = Clients::parse(&text).err().unwrap_or_default();
            assert!(error.contains(reason), "{text:?}: {error}");
        }
    }
}
