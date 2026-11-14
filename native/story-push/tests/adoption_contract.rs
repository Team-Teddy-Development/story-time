use ring::digest::{digest, SHA256};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ST023-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    fs::canonicalize(path).unwrap()
}

struct Project {
    root: PathBuf,
    outside: PathBuf,
}
impl Project {
    fn new() -> Self {
        Self {
            root: temp("repo"),
            outside: temp("outside"),
        }
    }
    fn state(&self) -> PathBuf {
        self.outside.join("state")
    }
    fn put(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let dest = self.root.join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(dest, bytes).unwrap();
    }
    fn read(&self, path: &str) -> Vec<u8> {
        fs::read(self.root.join(path)).unwrap()
    }
    fn config(&self) {
        self.put(".story-time/settings.json", json!({"tracker":"github","sync":"ask","prefix":"PAY","backlogDir":"work","github":{"repo":"acme/shop"},"definitionOfDone":["custom done"],"customSetting":{"keep":"東京"}}).to_string());
    }
    fn legacy_config(&self) {
        self.put(
            ".story-time.json",
            json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"work"}).to_string(),
        );
    }
    fn story(&self, name: &str, id: &str, status: &str, extra: &str) {
        self.put(name, format!("---\r\nid: {id}\r\ntitle: café 東京\r\nstatus: {status}\r\n{extra}---\r\n\r\n## Story\r\nRetain prose.\r\n\r\n## Custom section\r\nRetain me.\r\n"));
    }
    fn standard(&self) {
        self.config();
        self.story(
            "work/a.md",
            "PAY-001",
            "ready",
            "size: 3\r\ndepends_on: [PAY-002]\r\ncustom: [alpha, '東京']\r\nremote: 7\r\nremote_url: 'https://github.com/acme/shop/issues/7'\r\n",
        );
        self.story("work/b.md", "PAY-002", "done", "size: 5\r\nremote: 8\r\n");
        self.story("work/c.md", "PAY-003", "archived", "");
        self.put("README.md", "unrelated\n");
    }
    fn story_cmd(&self, args: &[&str], envs: &[(&str, &str)]) -> (Output, Value) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_story"));
        command
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", "/nonexistent");
        for (key, value) in envs {
            command.env(key, value);
        }
        let out = command.output().unwrap();
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            panic!(
                "adopt must emit JSON: {}",
                String::from_utf8_lossy(&out.stdout)
            )
        });
        (out, report)
    }
    fn plan(&self) -> PathBuf {
        let (out, _) = self.story_cmd(&["adopt", "preview"], &[]);
        let path = self.outside.join(format!(
            "plan-{}.json",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, out.stdout).unwrap();
        path
    }
    fn adopt(&self, command: &str, plan: &Path) -> (Output, Value) {
        self.adopt_env(command, plan, &[])
    }
    fn adopt_env(&self, command: &str, plan: &Path, envs: &[(&str, &str)]) -> (Output, Value) {
        let state = self.state();
        self.story_cmd(
            &[
                "adopt",
                command,
                "--plan",
                plan.to_str().unwrap(),
                "--state",
                state.to_str().unwrap(),
            ],
            envs,
        )
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
        fs::remove_dir_all(&self.outside).unwrap();
    }
}

fn sha(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let value = if meta.is_symlink() {
                fs::read_link(&path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec()
            } else if meta.is_dir() {
                visit(root, &path, out);
                b"directory".to_vec()
            } else {
                fs::read(&path).unwrap()
            };
            out.insert(path.strip_prefix(root).unwrap().to_owned(), value);
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
fn assert_conflict(out: &Output, report: &Value, code: &str) {
    assert_eq!(out.status.code(), Some(2), "{report}");
    assert_eq!(report["status"], "conflict", "{report}");
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == code && d["recovery"].as_str().is_some_and(|s| !s.is_empty())),
        "expected {code}: {report}"
    );
}
fn assert_backup_matches(p: &Project, before: &BTreeMap<PathBuf, Vec<u8>>) {
    let backup = p.state().join("backup");
    assert_eq!(mode(&backup), 0o700);
    let manifest: Value =
        serde_json::from_slice(&fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(mode(&backup.join("manifest.json")), 0o600);
    let files = manifest["files"].as_array().unwrap();
    let mut paths = Vec::new();
    for file in files {
        let copy = backup.join(file["copy"].as_str().unwrap());
        assert_eq!(mode(&copy), 0o600, "backup copy must be mode 600");
        let bytes = fs::read(&copy).unwrap();
        let path = PathBuf::from(file["path"].as_str().unwrap());
        assert_eq!(&bytes, before.get(&path).unwrap(), "backup bytes differ");
        assert_eq!(file["sha256"], sha(&bytes));
        paths.push(path);
    }
    for expected in [
        ".story-time/settings.json",
        "work/a.md",
        "work/b.md",
        "work/c.md",
    ] {
        assert!(paths.contains(&PathBuf::from(expected)), "{manifest}");
    }
}

#[test]
fn apply_backs_up_mode600_inputs_before_mutation_and_preserves_every_source_byte() {
    let p = Project::new();
    p.standard();
    let before = snapshot(&p.root);
    let plan = p.plan();
    let reviewed: Value = serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();

    // Stopping before the first journaled write proves backup and verification precede mutation.
    let (out, r) = p.adopt_env("apply", &plan, &[("STORY_TIME_ADOPT_FAULT", "before:1")]);
    assert_eq!(out.status.code(), Some(75), "{r}");
    assert_eq!(before, snapshot(&p.root), "no mutation before step 1");
    assert_backup_matches(&p, &before);

    let (out, r) = p.adopt("apply", &plan);
    assert!(out.status.success(), "{r}");
    assert_eq!(r["status"], "applied");
    assert_eq!(r["migration_id"], reviewed["migration_id"]);
    let after = snapshot(&p.root);
    for (path, bytes) in &before {
        assert_eq!(after.get(path), Some(bytes), "{path:?} changed");
    }
    let added: Vec<_> = after.keys().filter(|k| !before.contains_key(*k)).collect();
    assert_eq!(
        added,
        [
            Path::new(".story-time/adoption-receipt.json"),
            Path::new(".story-time/lifecycle.json")
        ]
    );
    for write in reviewed["allowed_writes"].as_array().unwrap() {
        assert_eq!(
            p.read(write["path"].as_str().unwrap()),
            write["content"].as_str().unwrap().as_bytes()
        );
    }
    assert_backup_matches(&p, &before);
    let journal: Value =
        serde_json::from_slice(&fs::read(p.state().join("journal.json")).unwrap()).unwrap();
    assert_eq!(journal["plan_digest"], reviewed["plan_digest"]);
    for step in journal["steps"].as_array().unwrap() {
        assert_eq!(step["state"], "complete");
        assert_eq!(step["preimage"], json!({"state":"absent"}));
        assert!(step["postimage_sha256"].is_string());
    }
}

#[test]
fn credential_bearing_inputs_are_refused_before_any_backup_or_write() {
    let p = Project::new();
    p.put(".story-time/settings.json", json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"work","gitea":{"token":"DO-NOT-COPY"}}).to_string());
    p.story("work/a.md", "PAY-001", "done", "");
    let before = snapshot(&p.root);
    let plan = p.plan();
    let (out, r) = p.adopt("apply", &plan);
    assert_conflict(&out, &r, "credential_in_backup_set");
    assert!(!r.to_string().contains("DO-NOT-COPY"));
    assert_eq!(before, snapshot(&p.root));
    assert!(!p.state().join("backup").exists());
}

#[test]
fn repeated_apply_and_verify_keep_files_and_receipt_identity_without_tracker_writes() {
    let tracker = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    tracker.set_nonblocking(true).unwrap();
    let base = format!("http://{}", tracker.local_addr().unwrap());
    let p = Project::new();
    p.put(".story-time/settings.json", json!({"tracker":"gitea","sync":"auto","prefix":"PAY","backlogDir":"work","gitea":{"baseUrl":base,"owner":"acme","repo":"shop"}}).to_string());
    p.story("work/a.md", "PAY-001", "ready", "remote: 3\r\n");
    let plan = p.plan();
    let env = [("GITEA_TOKEN", "recording-only")];
    let (out, r) = p.adopt_env("apply", &plan, &env);
    assert!(out.status.success(), "{r}");
    let (out, verified) = p.adopt_env("verify", &plan, &env);
    assert!(out.status.success(), "{verified}");
    assert_eq!(verified["status"], "verified");
    let receipt = p.read(".story-time/adoption-receipt.json");
    assert_eq!(verified["receipt_sha256"], sha(&receipt));
    let files = snapshot(&p.root);
    let state = snapshot(&p.state());
    let (out, again) = p.adopt_env("apply", &plan, &env);
    assert!(out.status.success(), "{again}");
    assert_eq!(again["status"], "already_adopted");
    let (out, reverified) = p.adopt_env("verify", &plan, &env);
    assert!(out.status.success(), "{reverified}");
    assert_eq!(reverified["receipt_sha256"], verified["receipt_sha256"]);
    assert_eq!(files, snapshot(&p.root), "repeat changed project files");
    assert_eq!(
        state,
        snapshot(&p.state()),
        "repeat changed journal or backup"
    );
    assert_eq!(
        tracker.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "adoption contacted the tracker"
    );
}

#[test]
fn interruption_before_or_after_each_journaled_write_resumes_by_byte_identity() {
    for (fault, observed) in [
        ("before:1", ["pending", "pending"]),
        ("after:1", ["complete", "pending"]),
        ("before:2", ["complete", "pending"]),
        ("after:2", ["complete", "complete"]),
    ] {
        let p = Project::new();
        p.standard();
        let before = snapshot(&p.root);
        let plan = p.plan();
        let (out, r) = p.adopt_env("apply", &plan, &[("STORY_TIME_ADOPT_FAULT", fault)]);
        assert_eq!(out.status.code(), Some(75), "{fault}: {r}");
        assert_eq!(r["status"], "interrupted");
        let (out, partial) = p.adopt("verify", &plan);
        if fault == "after:2" {
            assert!(out.status.success(), "{partial}");
        } else {
            assert_conflict(&out, &partial, "not_adopted");
        }
        p.put("README.md", "unrelated edit during outage\n");
        p.put("notes/new.txt", "unrelated new file\n");
        let (out, resumed) = p.adopt("apply", &plan);
        assert!(out.status.success(), "{fault}: {resumed}");
        assert_eq!(resumed["status"], "applied", "{fault}");
        let seen: Vec<_> = resumed["resume"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["observed"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(seen, observed, "{fault}");
        assert_eq!(p.read("README.md"), b"unrelated edit during outage\n");
        assert_eq!(p.read("notes/new.txt"), b"unrelated new file\n");
        assert_backup_matches(&p, &before);
        let (out, verified) = p.adopt("verify", &plan);
        assert!(out.status.success(), "{fault}: {verified}");
    }
    let p = Project::new();
    p.standard();
    let plan = p.plan();
    let (out, _) = p.adopt_env("apply", &plan, &[("STORY_TIME_ADOPT_FAULT", "after:1")]);
    assert_eq!(out.status.code(), Some(75));
    p.put(".story-time/lifecycle.json", "{\"edited\":true}\n");
    let before = snapshot(&p.root);
    let (out, r) = p.adopt("apply", &plan);
    assert_conflict(&out, &r, "step_diverged");
    assert_eq!(before, snapshot(&p.root));
    let journal: Value =
        serde_json::from_slice(&fs::read(p.state().join("journal.json")).unwrap()).unwrap();
    assert_eq!(journal["failures"][0]["code"], "step_diverged");
}

fn rewrite_plan(plan: &Path, edit: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&fs::read(plan).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("plan_digest");
    edit(&mut value);
    let digest = sha(value.to_string().as_bytes());
    value["plan_digest"] = json!(digest);
    fs::write(plan, format!("{value}\n")).unwrap();
}

#[test]
fn stale_unsafe_unsupported_and_tampered_plans_stop_without_changes() {
    let p = Project::new();
    p.standard();
    let plan = p.plan();
    p.story("work/b.md", "PAY-002", "done", "size: 8\r\n");
    let before = snapshot(&p.root);
    let (out, r) = p.adopt("apply", &plan);
    assert_conflict(&out, &r, "stale_plan");
    assert_eq!(before, snapshot(&p.root));
    assert!(!p.state().join("journal.json").exists());

    type Edit = fn(&mut Value);
    let cases: [(&str, Edit); 5] = [
        ("unsafe_path", |v| {
            v["allowed_writes"][0]["path"] = json!("../escape.json")
        }),
        ("credential_in_write_set", |v| {
            v["allowed_writes"][0]["path"] = json!(".story-time/.gitea.env")
        }),
        ("unsupported_capability", |v| {
            v["allowed_writes"][0]["operation"] = json!("replace")
        }),
        ("unsupported_version", |v| v["schema_version"] = json!(2)),
        ("plan_conflict", |v| v["status"] = json!("conflict")),
    ];
    for (code, edit) in cases {
        let p = Project::new();
        p.standard();
        let plan = p.plan();
        rewrite_plan(&plan, edit);
        let before = snapshot(&p.root);
        let (out, r) = p.adopt("apply", &plan);
        assert_conflict(&out, &r, code);
        assert_eq!(before, snapshot(&p.root), "{code}");
        assert!(!p.state().exists(), "{code} created state");
    }

    let p = Project::new();
    p.standard();
    let plan = p.plan();
    let mut text = fs::read_to_string(&plan).unwrap();
    text = text.replacen("\"order\":1", "\"order\":3", 1);
    fs::write(&plan, text).unwrap();
    let (out, r) = p.adopt("apply", &plan);
    assert_conflict(&out, &r, "plan_digest_mismatch");

    let p = Project::new();
    p.standard();
    let plan = p.plan();
    let inside = p.root.join("state");
    let (out, r) = p.story_cmd(
        &[
            "adopt",
            "apply",
            "--plan",
            plan.to_str().unwrap(),
            "--state",
            inside.to_str().unwrap(),
        ],
        &[],
    );
    assert_conflict(&out, &r, "unsafe_state_path");
    assert!(!inside.exists());
}

#[test]
fn concurrent_adoption_is_refused_while_the_repository_lock_is_held() {
    let p = Project::new();
    p.standard();
    let plan = p.plan();
    let holder = fs::File::open(&p.root).unwrap();
    holder.try_lock().unwrap();
    let before = snapshot(&p.root);
    for command in ["apply", "rollback"] {
        let (out, r) = p.adopt(command, &plan);
        assert_conflict(&out, &r, "lock_held");
        assert_eq!(before, snapshot(&p.root));
    }
    // Release is not asserted here: sibling tests fork concurrently, and a fork briefly
    // duplicates this descriptor. Crash release is covered by the interruption test.
    drop(holder);
}

#[test]
fn rollback_reverses_only_journaled_writes_and_keeps_unrelated_edits() {
    for legacy in [false, true] {
        let p = Project::new();
        if legacy {
            p.legacy_config();
            p.story("work/a.md", "PAY-001", "done", "");
        } else {
            p.standard();
        }
        let before = snapshot(&p.root);
        let plan = p.plan();
        let (out, r) = p.adopt("apply", &plan);
        assert!(out.status.success(), "{r}");
        p.put("README.md", "edited after adoption\n");
        let (out, r) = p.adopt("rollback", &plan);
        assert!(out.status.success(), "{r}");
        assert_eq!(r["status"], "rolled_back");
        let mut expected = before.clone();
        expected.insert(
            PathBuf::from("README.md"),
            b"edited after adoption\n".to_vec(),
        );
        assert_eq!(expected, snapshot(&p.root), "legacy={legacy}");
        let (out, r) = p.adopt("rollback", &plan);
        assert!(out.status.success(), "rollback must be repeatable: {r}");
        let (out, r) = p.adopt("apply", &plan);
        assert!(out.status.success(), "apply after rollback: {r}");
    }
}

#[test]
fn rollback_stops_on_diverged_bytes_and_records_the_failure() {
    let p = Project::new();
    p.standard();
    let plan = p.plan();
    let (out, _) = p.adopt("apply", &plan);
    assert!(out.status.success());
    p.put(
        ".story-time/adoption-receipt.json",
        "{\"hand\":\"edited\"}\n",
    );
    let before = snapshot(&p.root);
    let (out, r) = p.adopt("rollback", &plan);
    assert_conflict(&out, &r, "rollback_diverged");
    let diagnostic = &r["diagnostics"][0];
    assert_eq!(diagnostic["path"], ".story-time/adoption-receipt.json");
    assert_eq!(
        before,
        snapshot(&p.root),
        "divergent rollback mutated files"
    );
    let journal: Value =
        serde_json::from_slice(&fs::read(p.state().join("journal.json")).unwrap()).unwrap();
    assert_eq!(journal["failures"][0]["code"], "rollback_diverged");
    let (out, _) = p.adopt("rollback", &plan);
    assert_eq!(out.status.code(), Some(2));
    let journal: Value =
        serde_json::from_slice(&fs::read(p.state().join("journal.json")).unwrap()).unwrap();
    assert_eq!(journal["failures"].as_array().unwrap().len(), 2);

    let p = Project::new();
    p.standard();
    let plan = p.plan();
    let (out, r) = p.adopt("rollback", &plan);
    assert_conflict(&out, &r, "no_journal");
}

#[test]
fn legacy_stories_keep_missing_data_explicit_and_terminal_status_unchanged() {
    let p = Project::new();
    p.legacy_config();
    p.put("work/old-1.md", "---\nid: PAY-001\nstatus: done\n---\n");
    p.put(
        "work/old-2.md",
        "---\nid: PAY-002\nstatus: archived\ntitle: no criteria\n---\n\n## Notes\nfree text\n",
    );
    p.put("work/old-3.md", "---\nid: PAY-003\nstatus: draft\n---\n");
    let before = snapshot(&p.root);
    let plan = p.plan();
    let (out, r) = p.adopt("apply", &plan);
    assert!(out.status.success(), "{r}");
    let (out, verified) = p.adopt("verify", &plan);
    assert!(out.status.success(), "{verified}");
    for (path, bytes) in &before {
        assert_eq!(&snapshot(&p.root)[path], bytes);
    }
    let metadata: Value = serde_json::from_slice(&p.read(".story-time/lifecycle.json")).unwrap();
    let keys: Vec<_> = metadata.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        [
            "schema_version",
            "planning_schema_version",
            "migration_id",
            "preservation",
            "unknowns"
        ]
    );
    assert_eq!(
        metadata["preservation"]["config"]["path"],
        ".story-time.json"
    );
    for key in ["goal", "capacity", "epic_links", "history"] {
        assert!(metadata["unknowns"][key]["value"].is_null());
        assert!(metadata["unknowns"][key]["reason"]
            .as_str()
            .is_some_and(|s| !s.is_empty()));
        assert_eq!(verified["unknowns"][key], metadata["unknowns"][key]);
    }
    let statuses: Vec<_> = metadata["preservation"]["backlog"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["done", "archived", "draft"]);
    assert!(!p.root.join(".story-time/settings.json").exists());
}

#[test]
fn legacy_story_with_prose_before_its_first_section_is_preserved_byte_for_byte() {
    let p = Project::new();
    p.legacy_config();
    let story = "---\nid: PAY-001\nstatus: ready\n---\nWritten before sections existed.\n\nSecond paragraph.\n\n## Story\nRetain prose.\n";
    p.put("work/legacy.md", story);
    let (out, preview) = p.story_cmd(&["adopt", "preview"], &[]);
    assert!(out.status.success(), "{preview}");
    assert_eq!(preview["status"], "preview");
    assert!(preview["diagnostics"].as_array().unwrap().is_empty());
    let row = &preview["backlog"]["files"][0];
    assert_eq!(row["path"], "work/legacy.md");
    assert_eq!(
        (&row["id"], &row["status"]),
        (&json!("PAY-001"), &json!("ready"))
    );
    assert_eq!(row["sha256"], sha(story.as_bytes()));
    let plan = p.plan();
    let (out, r) = p.adopt("apply", &plan);
    assert!(out.status.success(), "{r}");
    assert_eq!(p.read("work/legacy.md"), story.as_bytes());
}

#[test]
fn setup_routes_use_the_native_contract_and_init_is_unchanged() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let installed = Project {
        root: temp("installed"),
        outside: temp("installed-state"),
    };
    let home = installed.root.clone();
    let install = Command::new("node")
        .arg(source.join("scripts/install.mjs"))
        .args(["--agent", "codex", "--scope", "project", "--project"])
        .arg(&home)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let skill = fs::read_to_string(home.join(".agents/skills/story-setup/SKILL.md")).unwrap();
    let routing = &skill[skill.find("## New, adopt, or upgrade").unwrap()..];
    for command in [
        "story init --from",
        "story adopt preview",
        "story adopt apply --plan",
        "story adopt verify --plan",
        "story adopt rollback --plan",
    ] {
        assert!(routing.contains(command), "setup route lacks {command}");
    }
    let run = |project: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .args(args)
            .current_dir(project)
            .env_clear()
            .env("PATH", "/nonexistent")
            .output()
            .unwrap()
    };
    let new = home.join("new");
    fs::create_dir(&new).unwrap();
    fs::write(
        new.join("answers.json"),
        json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"backlog"}).to_string(),
    )
    .unwrap();
    let out = run(&new, &["init", "--from", "answers.json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(new.join(".story-time/settings.json").is_file());
    assert!(
        !new.join(".story-time/lifecycle.json").exists(),
        "init must not adopt"
    );

    for kind in ["adopt", "upgrade"] {
        let project = home.join(kind);
        fs::create_dir_all(project.join("backlog")).unwrap();
        fs::write(
            project.join("backlog/PAY-001.md"),
            "---\nid: PAY-001\nstatus: done\n---\n",
        )
        .unwrap();
        fs::write(
            project.join(".story-time.json"),
            json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"backlog"})
                .to_string(),
        )
        .unwrap();
        if kind == "upgrade" {
            fs::create_dir(project.join(".story-time")).unwrap();
            fs::write(
                project.join(".story-time/lifecycle.json"),
                r#"{"schema_version":99}"#,
            )
            .unwrap();
        }
        let plan = home.join(format!("{kind}-plan.json"));
        fs::write(&plan, run(&project, &["adopt", "preview"]).stdout).unwrap();
        let state = home.join(format!("{kind}-state"));
        let adopt = |command: &str| {
            let out = run(
                &project,
                &[
                    "adopt",
                    command,
                    "--plan",
                    plan.to_str().unwrap(),
                    "--state",
                    state.to_str().unwrap(),
                ],
            );
            assert!(
                out.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let report: Value = serde_json::from_slice(&out.stdout).unwrap();
            (out.status.code(), report)
        };
        if kind == "adopt" {
            for (command, status) in [
                ("apply", "applied"),
                ("verify", "verified"),
                ("rollback", "rolled_back"),
            ] {
                let (code, report) = adopt(command);
                assert_eq!(
                    (code, report["status"].as_str()),
                    (Some(0), Some(status)),
                    "{report}"
                );
            }
        } else {
            let (code, report) = adopt("apply");
            assert_eq!(code, Some(2));
            assert!(
                report["diagnostics"].to_string().contains("plan_conflict"),
                "{report}"
            );
        }
    }
}

#[test]
fn no_non_rust_adoption_implementation_exists() {
    let source = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    fn visit(path: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if ![".git", "node_modules", "target", "native"].contains(&name.as_str()) {
                    visit(&path, found);
                }
            } else if ["mjs", "js", "cjs", "ts", "py", "sh"]
                .iter()
                .any(|ext| path.extension().is_some_and(|e| e == *ext))
            {
                let text = fs::read_to_string(&path).unwrap_or_default();
                if text.contains("adoption-receipt") || text.contains("lifecycle.json") {
                    found.push(path);
                }
            }
        }
    }
    let mut found = Vec::new();
    visit(&source, &mut found);
    assert!(found.is_empty(), "non-Rust adoption logic: {found:?}");
}
