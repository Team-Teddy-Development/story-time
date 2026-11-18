mod common;

use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "story-cloud-publish-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".story-time")).unwrap();
        fs::write(
            root.join(".story-time/settings.json"),
            r#"{"tracker":"github","sync":"auto","prefix":"VE","github":{"repo":"acme/shop"}}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("backlog")).unwrap();
        fs::write(root.join("backlog/VE-001.md"), "---\nid: VE-001\ntitle: Wrap the round name\nstatus: ready\ntype: bug\n---\n\n## Story\nAs a reader, I want a readable table, so that I can find the final.\n\n## Acceptance Criteria\n- [ ] Given a phone, when I open the results, then the name fits.\n").unwrap();
        common::install_stub_gh(&root.join("bin"));
        Self(root)
    }
    fn response(&self, op: &str, payload: &str) {
        common::respond_first(&self.0, op, 0, payload, "");
    }
    fn push(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .args(["push", "VE-001", "--json"])
            .current_dir(&self.0)
            .env_clear()
            .env("HOME", &self.0)
            .env("GH_TOKEN", "proxy-injected")
            .env("PATH", common::path_with_git(&self.0.join("bin")))
            .output()
            .unwrap()
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cloud_replay_finds_the_existing_issue_after_a_lost_create_response() {
    let r = Repo::new();
    let card = r.0.join("backlog/VE-001.md");
    let raw = fs::read_to_string(&card).unwrap();
    fs::write(
        &card,
        raw.replace(
            "status: ready\n",
            "status: ready\npending_create: 2026-11-18\n",
        ),
    )
    .unwrap();
    r.response("api --method GET repos/acme/shop/issues?state=all&since=2026-11-18T00%3A00%3A00Z&per_page=100&page=1", r#"[{"number":17,"title":"VE-001: Wrap the round name","html_url":"https://github.com/acme/shop/issues/17"}]"#);
    let result = r.push();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(fs::read_to_string(card).unwrap().contains("remote: 17"));
    let calls = fs::read_to_string(r.0.join("gh-calls")).unwrap();
    assert!(!calls.contains("--method POST"), "{calls}");
}

#[test]
fn publish_new_story_once() {
    let r = Repo::new();
    r.response(
        "api --method GET repos/acme/shop/labels?per_page=100&page=1",
        r#"[{"name":"bug"}]"#,
    );
    let issue = r#"{"number":17,"title":"VE-001: Wrap the round name","html_url":"https://github.com/acme/shop/issues/17","state":"open","labels":[{"name":"bug"}]}"#;
    r.response("api --method POST repos/acme/shop/issues --input -", issue);
    r.response("api --method GET repos/acme/shop/issues/17", issue);
    r.response(
        "api --method PATCH repos/acme/shop/issues/17 --input -",
        issue,
    );
    for _ in 0..2 {
        let out = r.push();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("https://github.com/acme/shop/issues/17")
        );
    }
    let calls = fs::read_to_string(r.0.join("gh-calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|l| *l == "api --method POST repos/acme/shop/issues --input -")
            .count(),
        1
    );
    assert!(calls.lines().all(|l| l.starts_with("api ")), "{calls}");
    assert!(fs::read_to_string(r.0.join("backlog/VE-001.md"))
        .unwrap()
        .contains("remote: 17"));
}
