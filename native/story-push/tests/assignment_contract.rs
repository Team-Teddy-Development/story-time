use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "st168-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join(".story-time")).unwrap();
        fs::create_dir_all(p.join("backlog")).unwrap();
        fs::write(
            p.join(".story-time/settings.json"),
            json!({"team":{"wipLimit":1}}).to_string(),
        )
        .unwrap();
        let r = Self(p);
        r.git(&["init", "-q"]);
        r.story("ST-001", "ready", 1, "[]");
        r.story("ST-002", "ready", 9, "[]");
        r
    }
    fn git(&self, args: &[&str]) {
        let o = Command::new("git")
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
        assert!(o.status.success(), "git failed");
    }
    fn story(&self, id: &str, status: &str, value: u32, deps: &str) {
        fs::write(self.0.join(format!("backlog/{id}.md")), format!("---\nid: {id}\ntitle: Assignment\nstatus: {status}\nvalue: {value}\ntime_criticality: 1\nrisk_reduction: 1\nsize: 1\ndepends_on: {deps}\n---\n\n## Story\nAs a local operator, I want owned work, so that work is not duplicated.\n\n## Acceptance Criteria\n- [ ] Given free capacity, when claimed, then ownership is recorded.\n")).unwrap();
    }
    fn call(&self, args: &[&str]) -> Output {
        call(&self.0, args)
    }
    fn next(&self, owner: &str) -> Value {
        success(self.call(&[
            "next",
            "--owner",
            owner,
            "--pid",
            &std::process::id().to_string(),
        ]))
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn call(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_story"))
        .arg("assignment")
        .args(args)
        .current_dir(path)
        .env("STORY_OTLP_ENDPOINT", "")
        .output()
        .unwrap()
}
fn success(o: Output) -> Value {
    assert!(
        o.status.success(),
        "assignment must succeed (exit {:?}): {}",
        o.status.code(),
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).expect("structured assignment output")
}
#[test]
fn claim_ranked_eligible_and_resume_idempotently() {
    let r = Repo::new();
    let first = r.next("run-a");
    assert_eq!(first["reason"], "claimed");
    assert_eq!(first["claim"]["story_id"], "ST-002");
    let again = r.next("run-a");
    assert_eq!(again["reason"], "resumed");
    assert_eq!(again["claim"]["token"], first["claim"]["token"]);
    assert_eq!(r.next("run-b")["reason"], "capacity_full");
}

struct Owner(std::process::Child);
impl Owner {
    fn new() -> Self {
        Self(Command::new("sleep").arg("120").spawn().unwrap())
    }
    fn pid(&self) -> String {
        self.0.id().to_string()
    }
    fn terminate(&mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn claim(r: &Repo, owner: &str, pid: &str) -> Value {
    success(r.call(&["next", "--owner", owner, "--pid", pid]))
}
fn ledger_path(r: &Repo) -> PathBuf {
    r.0.join(".git/story-assignment/ledger.json")
}
fn ledger(r: &Repo) -> Value {
    serde_json::from_slice(&fs::read(ledger_path(r)).unwrap()).unwrap()
}
fn write_ledger(r: &Repo, v: &Value) {
    fs::write(ledger_path(r), v.to_string()).unwrap();
}
fn stale(r: &Repo, op: &str, token: &str) {
    let bytes = fs::read(ledger_path(r)).unwrap();
    let out = r.call(&[op, "--token", token]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["reason"], "stale_token");
    assert_eq!(fs::read(ledger_path(r)).unwrap(), bytes);
}

#[test]
fn dead_owner_recovery_fences_tokens_and_preserves_work() {
    let r = Repo::new();
    let mut old = Owner::new();
    let first = claim(&r, "old-run", &old.pid());
    let token = first["claim"]["token"].as_str().unwrap();
    fs::write(r.0.join("partial-work.rs"), "uncommitted work").unwrap();
    let story_before = fs::read(r.0.join("backlog/ST-002.md")).unwrap();
    old.terminate();
    let reconciled = success(r.call(&["reconcile"]));
    assert_eq!(reconciled["changes"][0]["reason"], "verified_dead");
    assert_eq!(reconciled["after"]["claims"][0]["state"], "recoverable");
    assert!(reconciled["after"]["claims"][0].get("token").is_none());
    stale(&r, "heartbeat", token);
    stale(&r, "release", token);
    let recovered = r.next("new-run");
    assert_eq!(recovered["reason"], "recovered");
    assert_eq!(recovered["claim"]["story_id"], "ST-002");
    assert_ne!(recovered["claim"]["token"], first["claim"]["token"]);
    assert_eq!(recovered["claim"]["worktree"], first["claim"]["worktree"]);
    assert_eq!(recovered["claim"]["branch"], first["claim"]["branch"]);
    stale(&r, "release", token);
    assert_eq!(
        fs::read_to_string(r.0.join("partial-work.rs")).unwrap(),
        "uncommitted work"
    );
    assert_eq!(
        fs::read(r.0.join("backlog/ST-002.md")).unwrap(),
        story_before
    );
}

#[test]
fn alive_and_unknown_owners_are_not_recovered_by_age() {
    let r = Repo::new();
    let owner = Owner::new();
    let first = claim(&r, "live-run", &owner.pid());
    let mut l = ledger(&r);
    l["claims"][0]["heartbeat"] = json!("2000-01-01T00:00:00Z");
    write_ledger(&r, &l);
    let status = success(r.call(&["reconcile"]));
    assert_eq!(status["changes"], json!([]));
    assert_eq!(status["owners"][0]["liveness"], "alive");
    assert_eq!(r.next("other")["reason"], "capacity_full");
    assert_eq!(r.next("live-run")["reason"], "owner_conflict");
    l["claims"][0]["process"]["machine_id"] = json!("not-this-machine");
    write_ledger(&r, &l);
    let status = success(r.call(&["reconcile"]));
    assert_eq!(status["owners"][0]["liveness"], "unknown");
    assert_eq!(
        status["after"]["claims"][0]["token"],
        first["claim"]["token"]
    );
    assert_eq!(r.next("other")["reason"], "capacity_full");
}

#[test]
fn unknown_legacy_work_blocks_capacity_without_reset() {
    let r = Repo::new();
    r.story("ST-003", "in-progress", 3, "[]");
    let before = fs::read(r.0.join("backlog/ST-003.md")).unwrap();
    let out = r.next("run");
    assert_eq!(out["reason"], "legacy_ownership_unknown");
    assert_eq!(out["legacy_in_progress"], json!(["ST-003"]));
    assert_eq!(out["after"]["claims"], json!([]));
    assert_eq!(before, fs::read(r.0.join("backlog/ST-003.md")).unwrap());
}

#[test]
fn heartbeat_release_and_stale_token_fencing() {
    let r = Repo::new();
    let first = r.next("run");
    let token = first["claim"]["token"].as_str().unwrap();
    let beat = success(r.call(&["heartbeat", "--token", token]));
    assert_eq!(beat["reason"], "heartbeat");
    assert_eq!(beat["claim"]["token"], token);
    let release = success(r.call(&["release", "--token", token]));
    assert_eq!(release["reason"], "released");
    assert_eq!(release["after"]["claims"], json!([]));
    let second = r.next("another");
    assert_eq!(second["reason"], "claimed");
    assert_ne!(second["claim"]["token"], first["claim"]["token"]);
    stale(&r, "heartbeat", token);
    stale(&r, "release", token);
}

#[test]
fn dependency_and_readiness_are_rechecked_before_selection_and_recovery() {
    let r = Repo::new();
    r.story("ST-002", "ready", 9, "[ST-404]");
    assert_eq!(r.next("run")["claim"]["story_id"], "ST-001");
    let token = ledger(&r)["claims"][0]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    success(r.call(&["release", "--token", &token]));
    r.story("ST-001", "ready", 1, "[ST-404]");
    r.story("ST-002", "ready", 9, "[]");
    let p = r.0.join("backlog/ST-002.md");
    fs::write(
        &p,
        fs::read_to_string(&p)
            .unwrap()
            .replace("## Acceptance Criteria", "## Removed criteria"),
    )
    .unwrap();
    assert_eq!(r.next("none")["reason"], "no_eligible_story");
    r.story("ST-002", "ready", 9, "[]");
    let mut old = Owner::new();
    claim(&r, "old", &old.pid());
    old.terminate();
    r.story("ST-002", "ready", 9, "[ST-404]");
    let out = r.next("recover");
    assert_eq!(out["reason"], "recovery_ineligible");
    assert_eq!(out["after"]["claims"][0]["state"], "recoverable");
}

fn linked(r: &Repo) -> PathBuf {
    r.git(&["add", "."]);
    r.git(&[
        "-c",
        "user.name=Contract",
        "-c",
        "user.email=contract@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    let path = r.0.join("linked");
    r.git(&["worktree", "add", "-qb", "linked", path.to_str().unwrap()]);
    path
}

#[test]
fn concurrent_linked_worktrees_have_one_wip_winner() {
    let r = Repo::new();
    let linked = linked(&r);
    let pid = std::process::id().to_string();
    let mut children = Vec::new();
    for (i, path) in [&r.0, &linked, &r.0, &linked, &r.0, &linked]
        .iter()
        .enumerate()
    {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_story"))
                .args([
                    "assignment",
                    "next",
                    "--owner",
                    &format!("worker-{i}"),
                    "--pid",
                    &pid,
                ])
                .current_dir(path)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    let replies: Vec<_> = children
        .into_iter()
        .map(|child| success(child.wait_with_output().unwrap()))
        .collect();
    assert_eq!(
        replies.iter().filter(|v| v["reason"] == "claimed").count(),
        1
    );
    assert_eq!(
        replies
            .iter()
            .filter(|v| v["reason"] == "capacity_full")
            .count(),
        5
    );
    assert_eq!(ledger(&r)["claims"].as_array().unwrap().len(), 1);
    let other = success(call(&linked, &["status"]));
    assert_eq!(other["after"], ledger(&r));
}

#[test]
fn recovery_requires_recorded_worktree_instead_of_starting_fresh() {
    let r = Repo::new();
    let linked = linked(&r);
    let mut owner = Owner::new();
    let first = claim(&r, "old", &owner.pid());
    owner.terminate();
    let out = success(call(
        &linked,
        &[
            "next",
            "--owner",
            "new",
            "--pid",
            &std::process::id().to_string(),
        ],
    ));
    assert_eq!(out["reason"], "recovery_required");
    assert_eq!(out["claim"]["worktree"], first["claim"]["worktree"]);
    assert_eq!(out["after"]["claims"].as_array().unwrap().len(), 1);
    assert_eq!(r.next("new")["reason"], "recovered");
}

#[test]
fn zombie_owner_and_reused_pid_are_verified_dead() {
    let r = Repo::new();
    let mut owner = Owner::new();
    claim(&r, "old", &owner.pid());
    owner.0.kill().unwrap();
    for _ in 0..200 {
        let stat = fs::read_to_string(format!("/proc/{}/stat", owner.pid())).unwrap();
        if stat.rsplit_once(") ").unwrap().1.starts_with('Z') {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let recovered = r.next("new");
    assert_eq!(recovered["reason"], "recovered");
    owner.0.wait().unwrap();
    let mut l = ledger(&r);
    let start = l["claims"][0]["process"]["starttime"].as_u64().unwrap();
    l["claims"][0]["process"]["starttime"] = json!(start + 1);
    write_ledger(&r, &l);
    let next = r.next("pid-reuse");
    assert_eq!(next["reason"], "recovered");
    assert_eq!(next["changes"][0]["reason"], "verified_dead");
}

#[test]
fn linked_legacy_work_counts_toward_configured_capacity() {
    let r = Repo::new();
    let linked = linked(&r);
    r.story("ST-003", "in-progress", 1, "[]");
    let out = success(call(
        &linked,
        &[
            "next",
            "--owner",
            "worker",
            "--pid",
            &std::process::id().to_string(),
        ],
    ));
    assert_eq!(out["reason"], "legacy_ownership_unknown");
    assert_eq!(out["legacy_in_progress"], json!(["ST-003"]));
    fs::write(
        r.0.join(".story-time/settings.json"),
        json!({"team":{"wipLimit":2}}).to_string(),
    )
    .unwrap();
    let out = r.next("worker");
    assert_eq!(out["reason"], "claimed");
    assert_eq!(r.next("second")["reason"], "legacy_ownership_unknown");
}

#[test]
fn resume_rechecks_story_status_and_recovery_checks_branch() {
    let r = Repo::new();
    let linked = linked(&r);
    let first = r.next("same");
    r.story("ST-002", "done", 9, "[]");
    assert_eq!(r.next("same")["reason"], "recovery_ineligible");
    let out = success(call(
        &linked,
        &[
            "next",
            "--owner",
            "same",
            "--pid",
            &std::process::id().to_string(),
        ],
    ));
    assert_eq!(out["reason"], "recovery_required");
    assert_eq!(out["claim"]["token"], first["claim"]["token"]);
    r.git(&["checkout", "-qb", "different"]);
    assert_eq!(r.next("same")["reason"], "recovery_required");
}

#[test]
fn assignment_metadata_does_not_wait_for_story_bodies() {
    use std::io::Write;
    use std::time::{Duration, Instant};

    for op in ["status", "reconcile", "heartbeat", "release", "next"] {
        let r = Repo::new();
        let token = r.next("existing")["claim"]["token"]
            .as_str()
            .unwrap()
            .to_owned();
        let dir = if op == "next" {
            linked(&r)
        } else {
            r.0.clone()
        };
        let fifo = dir.join("backlog/ST-003.md");
        assert!(Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        let mut body = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&fifo)
            .unwrap();
        body.write_all(b"---\r\nid: ST-003\r\nstatus: in-progress\r\n---")
            .unwrap();
        let mut args = vec![op.to_owned()];
        if matches!(op, "heartbeat" | "release") {
            args.extend(["--token".into(), token]);
        } else if op == "next" {
            args.extend([
                "--owner".into(),
                "other".into(),
                "--pid".into(),
                std::process::id().to_string(),
            ]);
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_story"))
            .arg("assignment")
            .args(&args)
            .current_dir(&r.0)
            .env("STORY_OTLP_ENDPOINT", "")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let finished = loop {
            if child.try_wait().unwrap().is_some() {
                break true;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                break false;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        drop(body);
        let output = child.wait_with_output().unwrap();
        assert!(finished, "assignment {op} waited for a story body");
        let out = success(output);
        assert_eq!(out["legacy_in_progress"], json!(["ST-003"]));
    }
}

#[test]
fn legacy_metadata_is_fresh_and_malformed_headers_fail_closed() {
    let r = Repo::new();
    let linked = linked(&r);
    let path = linked.join("backlog/ST-003.md");
    for (status, expected) in [
        ("ready", json!([])),
        ("in-progress", json!(["ST-003"])),
        ("done", json!([])),
    ] {
        fs::write(
            &path,
            format!("---\nid: ST-003\nstatus: {status}\n---suffix without newline"),
        )
        .unwrap();
        assert_eq!(success(r.call(&["status"]))["legacy_in_progress"], expected);
    }
    for (header, message) in [
        (
            "---\nid: ST-003\nmalformed line\n---\n",
            "Unsupported frontmatter line",
        ),
        (
            "---\nid: ST-003\nstatus: in-progress\n",
            "missing YAML frontmatter",
        ),
        ("not frontmatter\n---\n", "missing YAML frontmatter"),
        (
            "---\nstatus: in-progress\n---\n",
            "Story id must be a string",
        ),
        (
            "---\nid: 3\nstatus: in-progress\n---\n",
            "Story id must be a string",
        ),
    ] {
        fs::write(&path, header).unwrap();
        let out = r.call(&["status"]);
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains(message));
        assert!(!ledger_path(&r).exists());
    }
}

#[test]
fn malformed_ledger_fails_closed_and_preserves_bytes_and_artifacts() {
    let r = Repo::new();
    r.next("run");
    fs::write(r.0.join("work.txt"), "partial").unwrap();
    let original = ledger(&r);
    let mut duplicate = original.clone();
    let c = duplicate["claims"][0].clone();
    duplicate["claims"].as_array_mut().unwrap().push(c);
    for bytes in [
        b"{bad json".to_vec(),
        duplicate.to_string().into_bytes(),
        b"{\"version\":2,\"claims\":[]}".to_vec(),
    ] {
        fs::write(ledger_path(&r), &bytes).unwrap();
        for args in [
            &["status"][..],
            &["reconcile"],
            &[
                "next",
                "--owner",
                "new",
                "--pid",
                &std::process::id().to_string(),
            ],
        ] {
            let out = r.call(args);
            assert_eq!(out.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&out.stderr).contains("Invalid assignment ledger"));
            assert_eq!(fs::read(ledger_path(&r)).unwrap(), bytes);
        }
    }
    assert_eq!(fs::read_to_string(r.0.join("work.txt")).unwrap(), "partial");
}
