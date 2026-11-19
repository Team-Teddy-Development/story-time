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
    /// The orgs of the registry that the client may use; empty covers every org.
    orgs: Vec<String>,
    /// The client may give its token in the URL path. A URL is written to more places than a
    /// header is, so only a client that the file marks can do this.
    url_token: bool,
}

/// Where a request carried its token.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Carrier {
    Header,
    Path,
}

impl Caller {
    pub(super) fn local() -> Self {
        Self {
            client_id: None,
            scopes: vec![Scope::Read, Scope::Write, Scope::Comment],
            prefixes: Vec::new(),
            orgs: Vec::new(),
            url_token: false,
        }
    }

    /// Whether the caller may use the repositories of `org`, with each of its scopes.
    pub(super) fn uses(&self, org: &str) -> bool {
        self.orgs.is_empty() || self.orgs.iter().any(|allowed| allowed == org)
    }

    pub(super) fn has(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }

    /// Whether the caller may use `scope` on a story with `prefix`. Reads cover each org that
    /// the caller may use (`uses`). A caller limited to prefixes is refused when the target has no known prefix.
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
            let orgs = list("orgs")?.into_iter().map(str::to_owned).collect();
            let url_token = match entry.get("url_token") {
                None => false,
                Some(Value::Bool(marked)) => *marked,
                Some(_) => return Err(format!("{at}: url_token must be true or false")),
            };
            clients.push((
                hash.to_owned(),
                Caller {
                    client_id: Some(id.to_owned()),
                    scopes,
                    prefixes,
                    orgs,
                    url_token,
                },
            ));
        }
        Ok(Self(clients))
    }

    /// The first client that names an org outside `held`, with that org.
    pub(super) fn unknown_org(&self, held: &[&str]) -> Option<(&str, &str)> {
        self.0.iter().find_map(|(_, caller)| {
            let org = caller
                .orgs
                .iter()
                .find(|org| !held.contains(&org.as_str()))?;
            Some((
                caller.client_id.as_deref().unwrap_or_default(),
                org.as_str(),
            ))
        })
    }

    /// The client whose token hash equals the hash of `token`, when that client may give its
    /// token through `carrier`. Each comparison reads every byte, and every client is compared,
    /// so the time does not depend on where a hash differs.
    pub(super) fn find(&self, token: &str, carrier: Carrier) -> Option<&Caller> {
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
        found.filter(|caller| carrier == Carrier::Header || caller.url_token)
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
        let caller = clients.find("token-a", Carrier::Header).unwrap();
        assert_eq!(caller.client_id.as_deref(), Some("a"));
        assert!(clients.find("token-b", Carrier::Header).is_none());
        assert!(clients.find("", Carrier::Header).is_none());
        assert!(
            clients.find(&sha(b"token-a"), Carrier::Header).is_none(),
            "the hash is not a token"
        );
    }

    #[test]
    fn scopes_and_prefixes_limit_what_a_caller_may_do() {
        let clients = Clients::parse(&file(
            "scopes = [\"read\", \"write\"]\nprefixes = [\"VC\"]\n",
        ))
        .unwrap();
        let caller = clients.find("token-a", Carrier::Header).unwrap();
        assert!(caller.permits(Scope::Read, None));
        assert!(caller.permits(Scope::Read, Some("VE")));
        assert!(caller.permits(Scope::Write, Some("VC")));
        assert!(!caller.permits(Scope::Write, Some("VE")));
        assert!(!caller.permits(Scope::Write, None));
        assert!(!caller.permits(Scope::Comment, Some("VC")));

        let open = Clients::parse(&file("scopes = [\"write\"]\n")).unwrap();
        let caller = open.find("token-a", Carrier::Header).unwrap();
        assert!(caller.permits(Scope::Write, Some("VE")));
        assert!(caller.permits(Scope::Write, None));
        assert!(!caller.permits(Scope::Read, None));

        let local = Caller::local();
        assert!(local.permits(Scope::Write, None) && local.permits(Scope::Comment, Some("X")));
    }

    #[test]
    fn orgs_limit_the_repositories_of_a_caller() {
        let limited = Clients::parse(&file(
            "scopes = [\"read\"]\norgs = [\"varsity\", \"teddy\"]\n",
        ))
        .unwrap();
        let caller = limited.find("token-a", Carrier::Header).unwrap();
        assert!(caller.uses("varsity") && caller.uses("teddy"));
        assert!(!caller.uses("other") && !caller.uses(""));
        assert_eq!(limited.unknown_org(&["varsity", "teddy"]), None);
        assert_eq!(limited.unknown_org(&["varsity"]), Some(("a", "teddy")));
        assert_eq!(limited.unknown_org(&[]), Some(("a", "varsity")));

        let open = Clients::parse(&file("scopes = [\"read\"]\n")).unwrap();
        assert!(open.find("token-a", Carrier::Header).unwrap().uses("other"));
        assert_eq!(open.unknown_org(&[]), None);
        assert!(Caller::local().uses("other"));

        let error = Clients::parse(&file("scopes = [\"read\"]\norgs = \"varsity\"\n")).err();
        assert_eq!(error.as_deref(), Some("client 1: orgs must be an array"));
    }

    #[test]
    fn only_a_marked_client_is_found_by_a_token_in_the_path() {
        let plain = Clients::parse(&file("scopes = [\"read\"]\n")).unwrap();
        assert!(plain.find("token-a", Carrier::Header).is_some());
        assert!(plain.find("token-a", Carrier::Path).is_none());
        let unmarked = Clients::parse(&file("scopes = [\"read\"]\nurl_token = false\n")).unwrap();
        assert!(unmarked.find("token-a", Carrier::Path).is_none());
        let marked = Clients::parse(&file("scopes = [\"read\"]\nurl_token = true\n")).unwrap();
        assert!(marked.find("token-a", Carrier::Path).is_some());
        assert!(marked.find("token-a", Carrier::Header).is_some());
        assert!(marked.find("token-b", Carrier::Path).is_none());
    }

    #[test]
    fn url_token_must_be_a_boolean() {
        for value in ["\"true\"", "1", "[true]"] {
            let error = Clients::parse(&file(&format!(
                "scopes = [\"read\"]\nurl_token = {value}\n"
            )))
            .err()
            .unwrap_or_else(|| panic!("{value} was accepted"));
            assert_eq!(error, "client 1: url_token must be true or false");
        }
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
