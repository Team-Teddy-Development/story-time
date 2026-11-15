//! The stories of every repository of one org, so that a story can name a story of another
//! repository by its bare id (`VC-109`). Each repository declares the prefixes it owns. An id
//! resolves through its prefix, and a prefix that two repositories declare is ambiguous.

use crate::lenient_story::Js;

pub(crate) struct OrgRepo {
    pub(crate) key: String,
    pub(crate) prefixes: Vec<String>,
    pub(crate) stories: Vec<Js>,
}

pub(crate) struct OrgIndex {
    repos: Vec<OrgRepo>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Resolved<'a> {
    Found(&'a Js),
    Missing,
    /// The keys of the repositories that could own the id.
    Ambiguous(Vec<&'a str>),
}

/// The prefix of a story id: the text before the last `-`, when digits follow it.
pub(crate) fn prefix_of(id: &str) -> Option<&str> {
    let (prefix, number) = id.rsplit_once('-')?;
    let numbered = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
    (numbered && !prefix.is_empty()).then_some(prefix)
}

fn has_id(story: &Js, id: &str) -> bool {
    matches!(story.get("id"), Js::Str(own) if own == id)
}

impl OrgIndex {
    pub(crate) fn new(repos: Vec<OrgRepo>) -> Self {
        Self { repos }
    }

    /// The keys of the repositories that declare `prefix`.
    pub(crate) fn owners(&self, prefix: &str) -> Vec<&str> {
        self.repos
            .iter()
            .filter(|repo| repo.prefixes.iter().any(|p| p == prefix))
            .map(|repo| repo.key.as_str())
            .collect()
    }

    /// The story with `id`. The repository that declares the prefix owns the id. An id whose
    /// prefix no repository declares resolves when exactly one repository holds it, which
    /// covers a repository that keeps stories from before it changed its prefix.
    pub(crate) fn resolve(&self, id: &str) -> Resolved<'_> {
        let owners = prefix_of(id).map(|p| self.owners(p)).unwrap_or_default();
        let holders: Vec<&OrgRepo> = match owners.len() {
            0 => self
                .repos
                .iter()
                .filter(|repo| repo.stories.iter().any(|s| has_id(s, id)))
                .collect(),
            1 => self
                .repos
                .iter()
                .filter(|repo| repo.key == owners[0])
                .collect(),
            _ => return Resolved::Ambiguous(owners),
        };
        match holders.as_slice() {
            [repo] => repo
                .stories
                .iter()
                .rev()
                .find(|s| has_id(s, id))
                .map_or(Resolved::Missing, Resolved::Found),
            [] => Resolved::Missing,
            many => Resolved::Ambiguous(many.iter().map(|repo| repo.key.as_str()).collect()),
        }
    }

    /// Whether a story of any repository lists `id` in `depends_on`.
    pub(crate) fn has_dependent(&self, id: &str) -> bool {
        self.repos
            .iter()
            .flat_map(|repo| &repo.stories)
            .any(|story| match story.get("depends_on") {
                Js::Arr(deps) => deps.iter().any(|d| matches!(d, Js::Str(d) if d == id)),
                _ => false,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story(id: &str, depends_on: &[&str]) -> Js {
        Js::Obj(vec![
            ("id".into(), Js::Str(id.into())),
            (
                "depends_on".into(),
                Js::Arr(depends_on.iter().map(|d| Js::Str((*d).into())).collect()),
            ),
        ])
    }

    fn repo(key: &str, prefixes: &[&str], stories: Vec<Js>) -> OrgRepo {
        OrgRepo {
            key: key.into(),
            prefixes: prefixes.iter().map(|p| (*p).into()).collect(),
            stories,
        }
    }

    fn org() -> OrgIndex {
        OrgIndex::new(vec![
            repo(
                "core",
                &["VC"],
                vec![story("VC-109", &[]), story("ST-001", &[])],
            ),
            repo("web", &["VE"], vec![story("VE-046", &["VC-109"])]),
        ])
    }

    #[test]
    fn a_prefix_is_the_text_before_the_number() {
        assert_eq!(prefix_of("VC-109"), Some("VC"));
        assert_eq!(prefix_of("MY-TEAM-7"), Some("MY-TEAM"));
        for odd in ["VC", "VC-", "-109", "VC-1a", ""] {
            assert_eq!(prefix_of(odd), None, "{odd}");
        }
    }

    #[test]
    fn an_id_resolves_through_the_repository_that_declares_its_prefix() {
        let org = org();
        assert!(matches!(org.resolve("VC-109"), Resolved::Found(s) if has_id(s, "VC-109")));
        assert_eq!(org.resolve("VC-999"), Resolved::Missing);
        assert_eq!(org.resolve("XX-1"), Resolved::Missing);
        assert_eq!(org.resolve("not an id"), Resolved::Missing);
    }

    #[test]
    fn an_id_of_an_undeclared_prefix_resolves_when_one_repository_holds_it() {
        let mut repos = org().repos;
        assert!(matches!(
            OrgIndex::new(std::mem::take(&mut repos)).resolve("ST-001"),
            Resolved::Found(_)
        ));
        let two = OrgIndex::new(vec![
            repo("core", &["VC"], vec![story("ST-001", &[])]),
            repo("web", &["VE"], vec![story("ST-001", &[])]),
        ]);
        assert_eq!(
            two.resolve("ST-001"),
            Resolved::Ambiguous(vec!["core", "web"])
        );
    }

    #[test]
    fn a_prefix_that_two_repositories_declare_is_ambiguous() {
        let org = OrgIndex::new(vec![
            repo("web", &["VE"], vec![story("VE-046", &[])]),
            repo("other", &["VE"], vec![]),
        ]);
        assert_eq!(org.owners("VE"), ["web", "other"]);
        assert_eq!(
            org.resolve("VE-046"),
            Resolved::Ambiguous(vec!["web", "other"])
        );
    }

    #[test]
    fn a_dependent_in_any_repository_is_found() {
        let org = org();
        assert!(org.has_dependent("VC-109"));
        assert!(!org.has_dependent("VE-046"));
    }
}
