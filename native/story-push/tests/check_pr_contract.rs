//! ST-152: `story check-pr` against a temporary backlog and a fake `gh`.
mod common;

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Repo(PathBuf);
impl Repo {
    fn new(settings: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-check-pr-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::write(path.join(".story-time/settings.json"), settings).unwrap();
        common::install_stub_gh(&path.join("bin"));
        Self(path.canonicalize().unwrap())
    }
    fn github() -> Self {
        Self::new(r#"{"tracker":"github","sync":"auto","github":{"repo":"acme/shop"}}"#)
    }
    fn card(&self, dir: &str, id: &str, status: &str, remote: Option<u64>, criteria: &str) {
        fs::create_dir_all(self.0.join(dir)).unwrap();
        let remote = remote.map(|n| format!("remote: {n}\n")).unwrap_or_default();
        fs::write(
            self.0.join(format!("{dir}/{id}.md")),
            format!("---\nid: {id}\ntitle: Story {id}\nstatus: {status}\ntype: story\n{remote}---\n\n## Story\nAs a user, I want it.\n\n{criteria}\n"),
        )
        .unwrap();
    }
    fn issue(&self, n: u64, labels: &str) {
        common::respond(
            &self.0,
            &format!("api repos/acme/shop/issues/{n} "),
            0,
            labels,
            "",
        );
    }
    fn check(&self, body: &str, mode: &str, incident: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_story"));
        command
            .arg("check-pr")
            .env_clear()
            .env("PATH", common::path_with_git(&self.0.join("bin")))
            .env("HOME", &self.0)
            .env("PR_BODY", body)
            .env("IS_INCIDENT", incident.to_string())
            .env("REPO", "acme/shop")
            .current_dir(&self.0);
        if !mode.is_empty() {
            command.env("STORY_ACCEPTANCE", mode);
        }
        command.output().unwrap()
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const TICKED: &str =
    "## Acceptance Criteria\n- [x] Given a, when b, then c\n- [x] Given d, when e, then f";
const OPEN: &str =
    "## Acceptance Criteria\n- [x] Given a, when b, then c\n- [ ] Given d, when e, then f";

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn ac1_a_done_card_with_every_box_ticked_passes() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "done", Some(5), TICKED);
    r.issue(5, "story\n");
    let o = r.check("Stories: ST-001 (#5)", "fail", false);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    assert_eq!(out(&o), "PASS: every declared story reference exists.\n");
}

#[test]
fn ac2_a_card_that_is_not_done_or_has_an_unticked_box_fails_in_fail_mode() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "in-progress", Some(5), TICKED);
    r.card("backlog", "ST-002", "done", Some(6), OPEN);
    for (body, text) in [
        (
            "Stories: ST-001",
            "ST-001 is in-progress with 0 unticked criteria",
        ),
        ("Stories: ST-002", "ST-002 is done with 1 unticked criteria"),
    ] {
        let o = r.check(body, "fail", false);
        assert_eq!(o.status.code(), Some(1), "{body}");
        assert!(err(&o).starts_with(&format!("FAIL: {text}")), "{}", err(&o));
        assert!(err(&o).contains(&format!("::error::{text}")), "{}", err(&o));
    }
}

#[test]
fn ac3_a_partial_reference_passes_including_the_cuj_2_string() {
    let r = Repo::github();
    r.card("backlog", "ST-904", "in-progress", Some(905), OPEN);
    r.issue(905, "story\n");
    let o = r.check("Stories: ST-904 (#905) (partial)", "fail", false);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
}

#[test]
fn ac4_an_incident_with_no_reference_passes_with_the_reminder() {
    let r = Repo::github();
    let o = r.check("", "fail", true);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    assert_eq!(
        out(&o),
        "PASS: incident exception; file the bug story within one business day.\n"
    );
    r.card("backlog", "ST-001", "in-progress", None, OPEN);
    let o = r.check("Stories: ST-001", "fail", true);
    assert_eq!(
        o.status.code(),
        Some(0),
        "incident exempts rules 2-4: {}",
        err(&o)
    );
}

#[test]
fn ac5_closing_keywords_need_a_covering_reference_unless_not_a_story_issue() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "done", Some(5), TICKED);
    r.issue(5, "story\n");
    r.issue(7, "story\n");
    r.issue(8, "documentation\n");
    let o = r.check("Stories: ST-001 (#5)\nCloses #7", "fail", false);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o)
        .contains("FAIL: #7 is not covered by a passing reference; remove the closing keyword"));
    for body in [
        "Stories: ST-001 (#5)\nCloses #5",
        "Stories: ST-001 (#5)\nFixes: acme/shop#5",
        "Stories: ST-001 (#5)\nResolves https://github.com/acme/shop/issues/5",
        "Stories: ST-001 (#5)\nFixes #8",
        "Stories: ST-001 (#5)\nCloses other/repo#7",
    ] {
        let o = r.check(body, "fail", false);
        assert_eq!(o.status.code(), Some(0), "{body}: {}", err(&o));
    }
    let o = r.check("Stories: ST-001 (#5)\nCloses: acme/shop#7", "fail", false);
    assert_eq!(
        o.status.code(),
        Some(1),
        "qualified target in this repository"
    );
}

#[test]
fn ac6_warn_mode_is_the_default_and_exits_0_with_warnings() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "in-progress", Some(5), OPEN);
    for mode in ["", "warn"] {
        let o = r.check("Stories: ST-001", mode, false);
        assert_eq!(o.status.code(), Some(0), "{mode}");
        assert!(err(&o).starts_with("::warning::ST-001 is in-progress with 1 unticked criteria; run story-accept or mark the reference (partial)"), "{}", err(&o));
        assert_eq!(
            out(&o),
            "PASS: every declared story reference exists. Warnings: 1.\n"
        );
    }
}

#[test]
fn a_spike_is_held_to_its_exit_criteria() {
    let r = Repo::github();
    r.card(
        "backlog",
        "ST-001",
        "done",
        Some(5),
        "## Exit Criteria\n- [ ] Question answered",
    );
    let o = r.check("Stories: ST-001", "fail", false);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("ST-001 is done with 1 unticked criteria"));
}

#[test]
fn a_done_card_with_zero_criteria_fails() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "done", Some(5), "");
    let o = r.check("Stories: ST-001", "fail", false);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("FAIL: ST-001 has no acceptance criteria"));
}

#[test]
fn an_unlinked_card_is_a_finding_unless_sync_is_off() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "done", None, TICKED);
    let o = r.check("Stories: ST-001", "fail", false);
    assert!(err(&o).contains("FAIL: ST-001 has no remote; run story push before merge"));
    let off = Repo::new(r#"{"tracker":"files","sync":"off"}"#);
    off.card("backlog", "ST-001", "done", None, TICKED);
    let o = off.check("Stories: ST-001", "fail", false);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
}

#[test]
fn a_bare_issue_uses_the_card_whose_remote_matches() {
    let r = Repo::github();
    r.card("backlog", "ST-002", "done", Some(6), TICKED);
    r.issue(6, "story\n");
    let o = r.check("Stories: #6", "fail", false);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
}

#[test]
fn an_id_and_issue_that_disagree_fail_in_fail_mode_and_warn_in_warn_mode() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "done", Some(6), TICKED);
    r.issue(5, "story\n");
    let o = r.check("Stories: ST-001 (#5)", "fail", false);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("FAIL: ST-001 (#5) does not match remote 6"));
    let o = r.check("Stories: ST-001 (#5)", "warn", false);
    assert_eq!(o.status.code(), Some(0));
    assert!(err(&o).contains("::warning::ST-001 (#5) does not match remote 6"));
}

#[test]
fn the_prefix_and_backlog_dir_come_from_settings() {
    let r = Repo::new(
        r#"{"tracker":"github","sync":"auto","prefix":"XY","backlogDir":"cards","github":{"repo":"acme/shop"}}"#,
    );
    r.card("cards", "XY-001", "done", Some(5), TICKED);
    let o = r.check("Stories: XY-001", "fail", false);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
}

#[test]
fn an_invalid_mode_is_a_usage_error() {
    let r = Repo::github();
    let o = r.check("Stories: ST-001", "strict", false);
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn an_issue_lookup_failure_is_a_link_failure_not_an_absent_label() {
    let r = Repo::github();
    common::respond(&r.0, "api repos/acme/shop/issues/5 ", 1, "", "HTTP 502");
    let o = r.check("Stories: #5", "warn", false);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("FAIL: #5: issue lookup failed"));
}

#[test]
fn issue_lookups_work_when_the_tracker_is_files() {
    let r = Repo::new(r#"{"tracker":"files","sync":"off"}"#);
    r.card("backlog", "ST-002", "done", Some(6), TICKED);
    r.issue(6, "story\n");
    let o = r.check("Stories: #6", "fail", false);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
}

// Moved from test/story-policy-contract.rs, which no longer runs story-link.
#[test]
fn link_rules_fail_in_every_mode_with_a_fail_prefix() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "done", Some(5), TICKED);
    let o = r.check("Stories: ST-001", "warn", false);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("PASS:"));
    for body in ["Related ST-001", "Stories: ST-999999999999"] {
        let o = r.check(body, "warn", false);
        assert_eq!(o.status.code(), Some(1), "{body}");
        assert!(err(&o).starts_with("FAIL:"), "{}", err(&o));
    }
    assert_eq!(
        r.check("Stories: ST-999999999999", "warn", true)
            .status
            .code(),
        Some(1),
        "an incident does not hide an invalid reference"
    );
}

#[test]
fn json_output_carries_only_the_report_on_stdout() {
    let r = Repo::github();
    r.card("backlog", "ST-001", "in-progress", Some(5), OPEN);
    let mut c = Command::new(env!("CARGO_BIN_EXE_story"));
    let o = c
        .args(["check-pr", "--json"])
        .env_clear()
        .env("PATH", common::path_with_git(&r.0.join("bin")))
        .env("PR_BODY", "Stories: ST-001 (partial), ST-001")
        .env("STORY_ACCEPTANCE", "fail")
        .current_dir(&r.0)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(report["result"], "fail");
    assert_eq!(report["mode"], "fail");
    assert_eq!(report["references"][0]["partial"], true);
    assert_eq!(report["findings"][0]["rule"], 2);
}
