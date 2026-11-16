//! The registry of a hosted server (ADR 0007): the repositories of each org, each with its clone
//! URL and the story prefixes it declares. A repository names its org, or takes the top-level one.
//! Links between stories resolve inside one org, so two orgs can declare the same prefix. The file is TOML, so that a reviewer sees every
//! prefix in one place. Loading refuses an entry the server cannot serve.

use crate::toml_subset;
use serde_json::Value;

pub(super) struct Registry {
    pub(super) fetch_ttl_seconds: u64,
    pub(super) repos: Vec<RegistryRepo>,
}

pub(super) struct RegistryRepo {
    pub(super) key: String,
    /// The org whose stories this repository's links resolve in.
    pub(super) org: String,
    pub(super) clone: String,
    pub(super) default_branch: Option<String>,
    pub(super) prefixes: Vec<String>,
}

const DEFAULT_TTL: u64 = 30;
/// The org of a repository when the registry names none.
const DEFAULT_ORG: &str = "default";
const KEY_FORM: &str = "^[a-z0-9][a-z0-9._-]{0,63}$";

fn key_ok(key: &str) -> bool {
    let mut bytes = key.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && key.len() <= 64
        && bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

fn prefix_ok(prefix: &str) -> bool {
    prefix.len() <= 32
        && prefix.starts_with(|c: char| c.is_ascii_uppercase())
        && prefix
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

impl Registry {
    /// The registry in `text`. An error names the entry that the server refuses.
    pub(super) fn parse(text: &str) -> Result<Self, String> {
        let document = toml_subset::parse(text)?;
        let fetch_ttl_seconds = match document.get("fetch_ttl_seconds") {
            None => DEFAULT_TTL,
            Some(value) => value
                .as_u64()
                .filter(|seconds| (1..=86_400).contains(seconds))
                .ok_or("fetch_ttl_seconds must be an integer from 1 to 86400")?,
        };
        let org_in = |table: &Value, at: &str| match table.get("org") {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .filter(|org| key_ok(org))
                .map(|org| Some(org.to_owned()))
                .ok_or_else(|| format!("{at}: org must match {KEY_FORM}")),
        };
        let top_org = org_in(&document, "the registry")?.unwrap_or_else(|| DEFAULT_ORG.to_owned());
        let entries = document
            .get("repo")
            .and_then(Value::as_array)
            .filter(|entries| !entries.is_empty())
            .ok_or("it needs at least one [[repo]] table")?;
        let mut repos: Vec<RegistryRepo> = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let key = entry
                .get("key")
                .and_then(Value::as_str)
                .filter(|key| key_ok(key))
                .ok_or_else(|| format!("repo {}: key must match {KEY_FORM}", index + 1))?;
            let at = format!("repo {key}");
            let org = org_in(entry, &at)?.unwrap_or_else(|| top_org.clone());
            let text = |name: &str| {
                entry
                    .get(name)
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("{at}: {name} must be a non-empty string"))
            };
            match text("forge")? {
                "github" => {}
                "gitea" => {
                    return Err(format!(
                        "{at}: forge gitea is not supported; this server serves github only"
                    ))
                }
                other => return Err(format!("{at}: unknown forge {other}")),
            }
            let clone = text("clone")?;
            if !["https://", "http://", "file://"]
                .iter()
                .any(|scheme| clone.starts_with(scheme))
                || clone.contains(char::is_whitespace)
                || clone.contains('@')
            {
                return Err(format!(
                    "{at}: clone must be an https, http, or file URL without credentials"
                ));
            }
            let default_branch = match entry.get("default_branch") {
                None => None,
                Some(_) => Some(text("default_branch")?)
                    .filter(|branch| branch_ok(branch))
                    .map(str::to_owned)
                    .map(Some)
                    .ok_or_else(|| format!("{at}: default_branch is not a branch name"))?,
            };
            let prefixes: Vec<String> = entry
                .get("prefixes")
                .and_then(Value::as_array)
                .and_then(|items| {
                    items
                        .iter()
                        .map(|item| item.as_str().filter(|p| prefix_ok(p)).map(str::to_owned))
                        .collect::<Option<Vec<_>>>()
                })
                .filter(|prefixes| !prefixes.is_empty())
                .ok_or_else(|| {
                    format!("{at}: prefixes must hold at least one story prefix such as \"VC\"")
                })?;
            if repos.iter().any(|repo| repo.key == key) {
                return Err(format!("{at}: the key is used by two entries"));
            }
            for prefix in &prefixes {
                let holds =
                    |repo: &&RegistryRepo| repo.org == org && repo.prefixes.contains(prefix);
                if let Some(other) = repos.iter().find(holds) {
                    return Err(format!(
                        "{at}: prefix {prefix} is already declared by repo {} of org {org}; one repository of an org owns a prefix",
                        other.key
                    ));
                }
            }
            repos.push(RegistryRepo {
                key: key.to_owned(),
                org,
                clone: clone.to_owned(),
                default_branch,
                prefixes,
            });
        }
        Ok(Self {
            fetch_ttl_seconds,
            repos,
        })
    }
}

/// Whether `name` is a branch name this server passes to git: no option, no path step out, and
/// none of the forms `git check-ref-format` refuses.
pub(super) fn branch_ok(name: &str) -> bool {
    (1..=200).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("/.")
        && !name.ends_with('/')
        && !name.ends_with('.')
        && !name.ends_with(".lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO: &str = "org = \"varsity\"\nfetch_ttl_seconds = 30\n[[repo]]\nkey = \"core\"\nclone = \"https://github.com/o/core.git\"\nforge = \"github\"\ndefault_branch = \"main\"\nprefixes = [\"VC\", \"ST\"]\n[[repo]]\nkey = \"web\"\nclone = \"https://github.com/o/web.git\"\nforge = \"github\"\nprefixes = [\"VE\"]\n";

    #[test]
    fn a_registry_lists_each_repository_with_its_prefixes() {
        let registry = Registry::parse(TWO).unwrap();
        assert_eq!(registry.fetch_ttl_seconds, 30);
        assert_eq!(registry.repos.len(), 2);
        assert_eq!(registry.repos[0].key, "core");
        assert_eq!(registry.repos[0].default_branch.as_deref(), Some("main"));
        assert_eq!(registry.repos[0].prefixes, ["VC", "ST"]);
        assert_eq!(registry.repos[1].default_branch, None);
    }

    #[test]
    fn a_repository_takes_its_own_org_or_the_top_level_org() {
        let one = Registry::parse(TWO).unwrap();
        assert_eq!(one.repos[0].org, "varsity");
        assert_eq!(one.repos[1].org, "varsity");
        let two = Registry::parse(&TWO.replace("key = \"web\"", "key = \"web\"\norg = \"teddy\""))
            .unwrap();
        assert_eq!(two.repos[0].org, "varsity");
        assert_eq!(two.repos[1].org, "teddy");
        let none = Registry::parse(&TWO.replace("org = \"varsity\"\n", "")).unwrap();
        assert_eq!(none.repos[0].org, "default");
    }

    #[test]
    fn one_prefix_can_be_in_two_orgs_but_not_twice_in_one() {
        let shared = TWO.replace("[\"VE\"]", "[\"VE\", \"ST\"]");
        let message = refused(&shared);
        assert!(
            message.contains("repo web")
                && message.contains("ST")
                && message.contains("repo core of org varsity"),
            "{message}"
        );
        let apart = shared.replace("key = \"web\"", "key = \"web\"\norg = \"teddy\"");
        let registry = Registry::parse(&apart).unwrap();
        assert_eq!(registry.repos[1].prefixes, ["VE", "ST"]);
    }

    #[test]
    fn an_org_or_key_the_server_cannot_serve_is_refused_by_name() {
        let in_two_orgs = TWO.replace("key = \"web\"", "key = \"core\"\norg = \"teddy\"");
        assert!(refused(&in_two_orgs).contains("repo core: the key is used by two entries"));
        let bad = TWO.replace("key = \"web\"", "key = \"web\"\norg = \"Team Teddy\"");
        assert!(
            refused(&bad).contains("repo web: org must match"),
            "{}",
            refused(&bad)
        );
        assert!(refused(&TWO.replace("org = \"varsity\"", "org = 7")).contains("the registry: org"));
    }

    fn refused(text: &str) -> String {
        Registry::parse(text)
            .err()
            .expect("the registry is refused")
    }

    #[test]
    fn an_entry_the_server_cannot_serve_is_refused_by_name() {
        let gitea = TWO.replace(
            "key = \"web\"\nclone = \"https://github.com/o/web.git\"\nforge = \"github\"",
            "key = \"web\"\nclone = \"https://gitea.example/o/web.git\"\nforge = \"gitea\"",
        );
        assert!(refused(&gitea).contains("repo web: forge gitea"));
        let repeated = TWO.replace("[\"VE\"]", "[\"VE\", \"ST\"]");
        let message = refused(&repeated);
        assert!(
            message.contains("repo web") && message.contains("ST") && message.contains("repo core"),
            "{message}"
        );
        assert!(refused(&TWO.replace("key = \"web\"", "key = \"core\"")).contains("two entries"));
        assert!(refused(&TWO.replace("[\"VE\"]", "[]")).contains("repo web: prefixes"));
        assert!(refused(&TWO.replace("[\"VE\"]", "[\"ve\"]")).contains("repo web: prefixes"));
        assert!(
            refused(&TWO.replace("https://github.com/o/web.git", "git@github.com:o/web.git"))
                .contains("repo web: clone")
        );
        assert!(refused(&TWO.replace(
            "https://github.com/o/web.git",
            "https://u:p@github.com/o/web"
        ))
        .contains("repo web: clone"));
        assert!(refused(&TWO.replace("\"main\"", "\"-x\"")).contains("default_branch"));
        assert!(refused(&TWO.replace("= 30", "= 0")).contains("fetch_ttl_seconds"));
        assert!(refused("org = \"none\"\n").contains("[[repo]]"));
    }

    #[test]
    fn a_branch_name_cannot_be_an_option_or_step_out_of_the_refs() {
        for good in ["main", "story-time/VC-109", "release/1.2", "a_b"] {
            assert!(branch_ok(good), "{good}");
        }
        for bad in [
            "",
            "-x",
            "../main",
            "a..b",
            "main lock",
            "a//b",
            "a/",
            "a.lock",
            "a/.b",
            "a.",
            "a~1",
            "a:b",
            "a^",
            "@{u}",
        ] {
            assert!(!branch_ok(bad), "{bad}");
        }
    }
}
