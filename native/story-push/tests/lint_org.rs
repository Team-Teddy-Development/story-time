//! Links between repositories of one org (ST-131): a story names a story of another repository
//! in `depends_on` or `relates_to`, and the lint checks the link when it can see both
//! repositories. `story mcp` with several `--root`s sees them all; `story lint` sees one.

use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn settings(prefix: &str) -> String {
    format!(
        r#"{{"tracker": "files", "sync": "off", "backlogDir": "backlog", "prefix": "{prefix}", "team": {{"wipLimit": 2}}}}"#
    )
}

/// A story that the lint finds ready when its links are sound.
fn story(id: &str, status: &str, links: &str, notes: &str) -> String {
    format!(
        "---\nid: {id}\ntitle: Story {id}\nstatus: {status}\ntype: story\nvalue: 3\ntime_criticality: 2\nrisk_reduction: 1\nsize: 2\n{links}created: 2026-11-15\nupdated: 2026-11-15\n---\n\n## Story\nAs a billing admin, I want invoices exported, so that I can reconcile them.\n\n## Acceptance Criteria\n- [ ] Given an invoice, when I export, then a CSV row is written.\n\n## Notes\n{notes}\n"
    )
}

struct Org {
    base: PathBuf,
}
impl Org {
    /// `web` owns VE, `core` owns VC, and `other` also declares VE.
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "story-lint-org-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&base).unwrap();
        let org = Self {
            base: base.canonicalize().unwrap(),
        };
        for (root, prefix) in [("web", "VE"), ("core", "VC"), ("other", "VE")] {
            org.write(root, ".story-time/settings.json", &settings(prefix));
        }
        org.story("core", "VC-109", "done", "depends_on: []\n", "None.");
        org.story("core", "VC-110", "draft", "depends_on: []\n", "None.");
        org.write(
            "core",
            "backlog/VC-120.md",
            "---\nid: VC-120\ntitle: Which roster API\nstatus: in-progress\ntype: spike\ntimebox: 2d\ndepends_on: []\ncreated: 2026-11-15\nupdated: 2026-11-15\n---\n\n## Question\nWhich roster API does the web client need?\n\n## Exit Criteria\n- [ ] The API is named.\n",
        );
        org.story("core", "VC-130", "draft", "depends_on: [VE-046]\n", "None.");
        org.story(
            "web",
            "VE-046",
            "draft",
            "depends_on: [VC-109]\nrelates_to: [VC-110]\n",
            "Paired with VC-109 and VC-110.",
        );
        org.story(
            "web",
            "VE-047",
            "draft",
            "depends_on: [VC-999]\nrelates_to: [VC-998]\n",
            "None.",
        );
        org.story(
            "web",
            "VE-048",
            "draft",
            "depends_on: []\n",
            "Paired varsity-web story for VC-109, and again VC-109. See VE-046 and XX-1.",
        );
        org.story("web", "VE-049", "draft", "depends_on: [VC-120]\n", "None.");
        org.story("web", "VE-050", "draft", "depends_on: [VC-109]\n", "None.");
        org.story("web", "VE-051", "draft", "depends_on: [VC-110]\n", "None.");
        org.story("web", "VE-052", "draft", "depends_on: [VE-404]\n", "None.");
        org.story("other", "VE-900", "draft", "depends_on: []\n", "None.");
        org
    }
    fn root(&self, name: &str) -> PathBuf {
        self.base.join(name)
    }
    fn write(&self, root: &str, relative: &str, text: &str) {
        let path = self.root(root).join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn story(&self, root: &str, id: &str, status: &str, links: &str, notes: &str) {
        self.write(
            root,
            &format!("backlog/{id}.md"),
            &story(id, status, links, notes),
        );
    }
    fn read(&self, root: &str, id: &str) -> String {
        fs::read_to_string(self.root(root).join(format!("backlog/{id}.md"))).unwrap()
    }
    fn command(&self, root: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_story"));
        command
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("HOME", &self.base)
            .current_dir(self.root(root));
        command
    }
    /// `story <args>` with `root` as the current directory.
    fn cli(&self, root: &str, args: &[&str]) -> Output {
        self.command(root).args(args).output().unwrap()
    }
    /// One `tools/call` for each entry through one `story mcp` over `roots`; the structured
    /// content of each result, in order.
    fn mcp(&self, roots: &[&str], write: bool, calls: &[(&str, Value)]) -> Vec<Value> {
        let mut command = self.command(roots[0]);
        command.arg("mcp");
        for root in roots {
            command.arg("--root").arg(self.root(root));
        }
        if write {
            command.arg("--write");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for (n, (name, arguments)) in calls.iter().enumerate() {
            let message = json!({"jsonrpc": "2.0", "id": n, "method": "tools/call", "params": {
                "name": name, "arguments": arguments,
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {}
                }}});
            writeln!(stdin, "{message}").unwrap();
        }
        drop(stdin);
        let output = child.wait_with_output().unwrap();
        let replies: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(replies.len(), calls.len(), "{replies:?}");
        replies
            .into_iter()
            .map(|reply| {
                assert!(reply.get("error").is_none(), "{reply}");
                reply["result"]["structuredContent"].clone()
            })
            .collect()
    }
    /// The findings of one story as `(rule, severity, message)`, linted through `story mcp`.
    fn findings(&self, roots: &[&str], repository: &str, id: &str) -> Vec<(String, String, String)> {
        let content = self
            .mcp(
                roots,
                false,
                &[("story_lint", json!({"repository": repository, "ids": [id]}))],
            )
            .remove(0);
        let findings = content["results"][0]["findings"]
            .as_array()
            .unwrap_or_else(|| panic!("{content}"));
        findings.iter().map(finding).collect()
    }
}
impl Drop for Org {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn finding(value: &Value) -> (String, String, String) {
    let text = |key: &str| value[key].as_str().unwrap().to_owned();
    (text("rule"), text("severity"), text("message"))
}

fn of_rule<'a>(
    findings: &'a [(String, String, String)],
    rule: &str,
) -> Vec<&'a (String, String, String)> {
    findings.iter().filter(|f| f.0 == rule).collect()
}

fn errors(findings: &[(String, String, String)]) -> Vec<&(String, String, String)> {
    findings.iter().filter(|f| f.1 == "error").collect()
}

/// `story lint <id> --json` in `root`: the exit code and the findings.
fn cli_lint(org: &Org, root: &str, id: &str) -> (Option<i32>, Vec<(String, String, String)>) {
    let output = org.cli(root, &["lint", id, "--json"]);
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}", String::from_utf8_lossy(&output.stderr));
    });
    let findings = parsed[0]["findings"].as_array().unwrap();
    (
        output.status.code(),
        findings.iter().map(finding).collect(),
    )
}

#[test]
fn a_link_to_a_story_in_another_root_resolves() {
    let org = Org::new();
    let linked = org.findings(&["web", "core"], "web", "VE-046");
    assert!(errors(&linked).is_empty(), "{linked:?}");
    assert!(of_rule(&linked, "independent.deps").is_empty(), "{linked:?}");
    assert!(of_rule(&linked, "links.ambiguous").is_empty(), "{linked:?}");

    // A dependency in another root that is not done keeps the warning a local one gets.
    let waiting = org.findings(&["web", "core"], "web", "VE-051");
    assert_eq!(
        of_rule(&waiting, "independent.deps"),
        [&(
            "independent.deps".to_owned(),
            "warning".to_owned(),
            "Depends on VC-110 (draft). Not independent until it is done.".to_owned()
        )]
    );
    // A related story is not a blocker, so its status raises nothing.
    assert!(of_rule(&linked, "independent.deps").is_empty());
}

#[test]
fn a_link_to_a_story_that_no_root_has_is_an_error() {
    let org = Org::new();
    let findings = org.findings(&["web", "core"], "web", "VE-047");
    let messages: Vec<&str> = of_rule(&findings, "independent.deps")
        .iter()
        .map(|f| {
            assert_eq!(f.1, "error");
            f.2.as_str()
        })
        .collect();
    assert_eq!(
        messages,
        [
            "Depends on VC-999, which does not exist.",
            "Relates to VC-998, which does not exist."
        ]
    );
    let local = org.findings(&["web", "core"], "web", "VE-052");
    assert_eq!(
        of_rule(&local, "independent.deps")[0].2,
        "Depends on VE-404, which does not exist."
    );
}

#[test]
fn a_prefix_that_two_roots_declare_is_ambiguous() {
    let org = Org::new();
    let findings = org.findings(&["web", "core", "other"], "core", "VC-130");
    let ambiguous = of_rule(&findings, "links.ambiguous");
    assert_eq!(ambiguous.len(), 1, "{findings:?}");
    assert_eq!(ambiguous[0].1, "error");
    assert!(ambiguous[0].2.contains("VE-046"), "{ambiguous:?}");
    assert!(
        ambiguous[0].2.contains("other") && ambiguous[0].2.contains("web"),
        "{ambiguous:?}"
    );
    assert!(of_rule(&findings, "independent.deps").is_empty(), "{findings:?}");

    // A story of an ambiguous prefix that names its own prefix gets the same error.
    let own = org.findings(&["web", "core", "other"], "web", "VE-052");
    assert_eq!(of_rule(&own, "links.ambiguous").len(), 1, "{own:?}");

    let two = org.findings(&["web", "core"], "core", "VC-130");
    assert!(of_rule(&two, "links.ambiguous").is_empty(), "{two:?}");
    assert!(errors(&two).is_empty(), "{two:?}");
}

#[test]
fn a_mention_in_the_text_without_a_link_is_a_warning() {
    let org = Org::new();
    let findings = org.findings(&["web", "core"], "web", "VE-048");
    let mentions = of_rule(&findings, "links.prose_mention");
    assert_eq!(mentions.len(), 1, "one warning for each id: {findings:?}");
    assert_eq!(mentions[0].1, "warning");
    assert!(mentions[0].2.contains("VC-109"), "{mentions:?}");
    assert!(errors(&findings).is_empty(), "{findings:?}");

    // A linked id, an id of the story's own repository, and an undeclared prefix raise nothing.
    let linked = org.findings(&["web", "core"], "web", "VE-046");
    assert!(of_rule(&linked, "links.prose_mention").is_empty(), "{linked:?}");

    // One root cannot tell which prefixes other repositories declare.
    let alone = org.findings(&["web"], "web", "VE-048");
    assert!(of_rule(&alone, "links.prose_mention").is_empty(), "{alone:?}");
}

#[test]
fn story_lint_in_one_repository_warns_about_a_link_it_cannot_check() {
    let org = Org::new();
    let (code, findings) = cli_lint(&org, "web", "VE-046");
    assert_eq!(code, Some(0), "{findings:?}");
    assert!(errors(&findings).is_empty(), "{findings:?}");
    let unresolved: Vec<&str> = of_rule(&findings, "independent.foreign_unresolved")
        .iter()
        .map(|f| {
            assert_eq!(f.1, "warning");
            f.2.split(' ').next().unwrap()
        })
        .collect();
    assert_eq!(unresolved, ["VC-109", "VC-110"]);

    // A missing id of the repository's own prefix is still an error.
    let (code, findings) = cli_lint(&org, "web", "VE-052");
    assert_eq!(code, Some(1));
    assert_eq!(
        of_rule(&findings, "independent.deps")[0].2,
        "Depends on VE-404, which does not exist."
    );
    assert!(of_rule(&findings, "independent.foreign_unresolved").is_empty());

    // With both roots the link is checked, so nothing is left unresolved.
    let checked = org.findings(&["web", "core"], "web", "VE-046");
    assert!(of_rule(&checked, "independent.foreign_unresolved").is_empty());
}

#[test]
fn a_spike_that_a_story_in_another_root_waits_on_unblocks_a_story() {
    let org = Org::new();
    let (code, alone) = cli_lint(&org, "core", "VC-120");
    assert_eq!(code, Some(1));
    assert_eq!(of_rule(&alone, "spike.unblocks").len(), 1, "{alone:?}");

    let together = org.findings(&["core", "web"], "core", "VC-120");
    assert!(of_rule(&together, "spike.unblocks").is_empty(), "{together:?}");
    assert!(errors(&together).is_empty(), "{together:?}");
}

#[test]
fn story_update_moves_a_story_to_ready_when_its_dependency_in_another_root_is_done() {
    let org = Org::new();
    let shown = org.mcp(
        &["web", "core"],
        false,
        &[
            ("story_show", json!({"repository": "web", "id": "VE-050"})),
            ("story_show", json!({"repository": "web", "id": "VE-047"})),
        ],
    );
    let update = |index: usize, id: &str| {
        (
            "story_update",
            json!({"repository": "web", "id": id, "set": {"status": "ready"},
                "expected_sha256": shown[index]["sha256"]}),
        )
    };
    let results = org.mcp(
        &["web", "core"],
        true,
        &[update(0, "VE-050"), update(1, "VE-047")],
    );
    assert!(results[0].get("error").is_none(), "{}", results[0]);
    assert!(org.read("web", "VE-050").contains("\nstatus: ready\n"));

    assert_eq!(results[1]["error"]["code"], "lifecycle_refused", "{}", results[1]);
    let refused = results[1]["error"]["details"]["findings"].to_string();
    assert!(refused.contains("VC-999"), "{refused}");
    assert!(org.read("web", "VE-047").contains("\nstatus: draft\n"));
}

#[test]
fn relates_to_is_a_list_field_saved_directly_after_depends_on() {
    let org = Org::new();
    let set = org.cli("web", &["set", "VE-046", "size=3"]);
    assert!(set.status.success(), "{}", String::from_utf8_lossy(&set.stderr));
    assert!(
        org.read("web", "VE-046")
            .contains("\nsize: 3\ndepends_on: [VC-109]\nrelates_to: [VC-110]\n"),
        "{}",
        org.read("web", "VE-046")
    );

    let set = org.cli("web", &["set", "VE-050", "relates_to=VC-110, VC-109"]);
    assert!(set.status.success(), "{}", String::from_utf8_lossy(&set.stderr));
    assert!(
        org.read("web", "VE-050")
            .contains("\ndepends_on: [VC-109]\nrelates_to: [VC-110, VC-109]\n"),
        "{}",
        org.read("web", "VE-050")
    );

    let cleared = org.cli("web", &["set", "VE-050", "relates_to="]);
    assert!(cleared.status.success());
    assert!(org.read("web", "VE-050").contains("\nrelates_to: []\n"));
}
