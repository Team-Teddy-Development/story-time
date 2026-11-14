use ring::digest::{digest, SHA256};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ST022-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn put(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let dest = self.0.join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(dest, bytes).unwrap();
    }
    fn config(&self, legacy: bool) {
        self.put(if legacy {".story-time.json"} else {".story-time/settings.json"}, json!({"tracker":"github","sync":"ask","prefix":"PAY","backlogDir":"work","github":{"repo":"acme/shop"},"token":"DO-NOT-REPORT-CONFIG"}).to_string());
    }
    fn story(&self, name: &str, id: &str, status: &str, extra: &str) {
        self.put(name, format!("---\r\nid: {id}\r\ntitle: café 東京\r\nstatus: {status}\r\n{extra}---\r\n\r\n## Story\r\nDO-NOT-REPORT-STORY\r\n\r\n## Custom section\r\nRetain me.\r\n"));
    }
    fn preview(&self) -> (Output, Value) {
        let before = snapshot(&self.0);
        let out = Command::new(env!("CARGO_BIN_EXE_story"))
            .args(["adopt", "preview"])
            .current_dir(&self.0)
            .env_clear()
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(before, snapshot(&self.0), "preview changed project state");
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report =
            serde_json::from_slice(&out.stdout).expect("preview must emit JSON without Node");
        (out, report)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
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
            let entry = entry.unwrap();
            let path = entry.path();
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
fn conflict(p: &Project, code: &str) -> Value {
    let (out, report) = p.preview();
    assert_eq!(out.status.code(), Some(2), "{report}");
    assert_eq!(report["status"], "conflict");
    assert_eq!(report["allowed_writes"], json!([]));
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == code && d["recovery"].as_str().is_some_and(|s| !s.is_empty())),
        "{report}"
    );
    report
}
#[test]
fn current_and_legacy_exact_inventory_and_deterministic_plan() {
    for legacy in [false, true] {
        let p = Project::new();
        p.config(legacy);
        for (name, id, status) in [
            ("work/z.md", "PAY-003", "archived"),
            ("work/é.md", "PAY-002", "done"),
            ("work/a.md", "PAY-001", "split"),
        ] {
            p.story(
                name,
                id,
                status,
                "custom: [alpha, '東京']\r\nremote: 42\r\n",
            );
        }
        // Distinct remote identities are required even for terminal records.
        p.story(
            "work/z.md",
            "PAY-003",
            "archived",
            "custom: true\r\nremote: 43\r\n",
        );
        p.story(
            "work/é.md",
            "PAY-002",
            "done",
            "custom: café\r\nremote: 44\r\n",
        );
        let (out, r) = p.preview();
        assert!(out.status.success(), "{r}");
        assert_eq!(out.stdout, p.preview().0.stdout);
        assert_eq!(r["source_version"], 0);
        assert_eq!(r["target_metadata_version"], 1);
        let config = if legacy {
            ".story-time.json"
        } else {
            ".story-time/settings.json"
        };
        assert_eq!(r["config"]["path"], config);
        assert_eq!(
            r["config"]["sha256"],
            sha(&fs::read(p.0.join(config)).unwrap())
        );
        assert_eq!(r["config"]["sync"], "ask");
        let files = r["backlog"]["files"].as_array().unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(
            files
                .iter()
                .map(|v| v["path"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["work/a.md", "work/z.md", "work/é.md"]
        );
        for f in files {
            assert_eq!(
                f["sha256"],
                sha(&fs::read(p.0.join(f["path"].as_str().unwrap())).unwrap())
            );
        }
        for key in ["goal", "capacity", "epic_links", "history"] {
            assert!(r["unknowns"][key]["value"].is_null());
            assert!(r["unknowns"][key]["reason"].is_string());
        }
        let writes = r["allowed_writes"].as_array().unwrap();
        assert_eq!(writes.len(), 2);
        for (i, w) in writes.iter().enumerate() {
            assert_eq!(w["order"], i + 1);
            assert_eq!(w["preimage"], json!({"state":"absent"}));
            assert_eq!(
                w["postimage_sha256"],
                sha(w["content"].as_str().unwrap().as_bytes())
            );
        }
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(!text.contains("DO-NOT-REPORT"));
        assert_eq!(r["preservation_rules"]["story_bytes"], "unchanged");
    }
}
#[test]
fn missing_configuration_does_not_invent_identity() {
    let p = Project::new();
    p.story("backlog/ANY.md", "OTHER-001", "done", "");
    let r = conflict(&p, "missing_config");
    assert!(r["config"].is_null());
    assert!(r["backlog"]["prefix"].is_null());
    assert_eq!(r["backlog"]["files"].as_array().unwrap().len(), 1);
}
#[test]
fn malformed_nested_duplicates_and_remote_conflicts_are_visible() {
    for (body, code) in [
        ("bad", "unsupported_story"),
        (
            "---\nid: PAY-001\ncustom: {nested: true}\n---\n",
            "unsupported_story",
        ),
        (
            "---\nid: PAY-001\ncustom:\n  child: yes\n---\n",
            "unsupported_story",
        ),
    ] {
        let p = Project::new();
        p.config(false);
        p.put("work/a.md", body);
        let r = conflict(&p, code);
        assert_eq!(r["backlog"]["files"].as_array().unwrap().len(), 1);
        assert!(r["backlog"]["files"][0]["sha256"].is_string());
    }
    let p = Project::new();
    p.config(false);
    p.story("work/a.md", "PAY-001", "done", "");
    p.story("work/b.md", "PAY-001", "ready", "");
    conflict(&p, "duplicate_id");
    let p = Project::new();
    p.config(false);
    p.story("work/a.md", "PAY-001", "done", "remote: 7\r\n");
    p.story("work/b.md", "PAY-002", "ready", "remote: 7\r\n");
    conflict(&p, "remote_conflict");
    let p = Project::new();
    p.config(false);
    p.story("work/nested/a.md", "PAY-001", "done", "");
    conflict(&p, "nested_source");
}
#[test]
fn config_conflicts_unknown_versions_tracker_only_and_path_boundaries() {
    let p = Project::new();
    p.config(false);
    p.config(true);
    conflict(&p, "config_conflict");
    let p = Project::new();
    p.config(false);
    p.put(".story-time/lifecycle.json", r#"{"schema_version":99}"#);
    conflict(&p, "unknown_version");
    let p = Project::new();
    p.config(false);
    conflict(&p, "tracker_only");
    for path in [
        "../outside",
        "/tmp/outside",
        "work/../work",
        ".story-time",
        "work/./nested",
    ] {
        let p = Project::new();
        p.put(
            ".story-time/settings.json",
            json!({"tracker":"files","prefix":"PAY","backlogDir":path}).to_string(),
        );
        conflict(&p, "unsafe_path");
    }
    let p = Project::new();
    p.put(".story-time/settings.json", b"{broken SECRET");
    let r = conflict(&p, "invalid_config");
    assert!(!r.to_string().contains("SECRET"));
}
#[cfg(unix)]
#[test]
fn symlinks_at_every_boundary_are_rejected_without_reading_targets() {
    use std::os::unix::fs::symlink;
    for target in [
        ".story-time",
        ".story-time/settings.json",
        "work",
        "work/a.md",
        ".story-time/lifecycle.json",
        ".story-time/adoption-receipt.json",
    ] {
        let p = Project::new();
        let outside = Project::new();
        outside.put("secret", "DO-NOT-READ");
        if target != ".story-time" {
            p.config(false);
        }
        let path = p.0.join(target);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }
        symlink(outside.0.join("secret"), path).unwrap();
        let r = conflict(&p, "unsafe_path");
        assert!(!r.to_string().contains("DO-NOT-READ"));
    }
}
#[test]
fn adopted_requires_receipt_and_exact_preservation_membership() {
    let p = Project::new();
    p.config(false);
    p.story("work/a.md", "PAY-001", "done", "");
    let (_, r) = p.preview();
    for w in r["allowed_writes"].as_array().unwrap() {
        p.put(w["path"].as_str().unwrap(), w["content"].as_str().unwrap());
    }
    let (out, r) = p.preview();
    assert!(out.status.success(), "{r}");
    assert_eq!(r["status"], "already_adopted");
    assert_eq!(r["allowed_writes"], json!([]));
    p.story("work/b.md", "PAY-002", "done", "");
    conflict(&p, "preservation_diverged");
    fs::remove_file(p.0.join("work/b.md")).unwrap();
    p.story("work/a.md", "PAY-001", "ready", "");
    conflict(&p, "preservation_diverged");
    let p = Project::new();
    p.config(false);
    p.story("work/a.md", "PAY-001", "done", "");
    p.put(".story-time/lifecycle.json", r#"{"schema_version":1}"#);
    conflict(&p, "preservation_diverged");
}
#[test]
fn source_link_conflicts_and_receipt_versions_are_diagnosed() {
    for link in [
        "https://github.com/acme/shop/issues/7",
        "https://github.com/acme/shop/issues/7/",
    ] {
        let p = Project::new();
        p.config(false);
        p.story(
            "work/a.md",
            "PAY-001",
            "done",
            &format!("remote: 7\r\nremote_url: '{link}'\r\n"),
        );
        let (out, report) = p.preview();
        assert!(out.status.success(), "{report}");
    }
    for link in [
        "https://github.com/acme/other/issues/7",
        "https://github.com/acme/shop/issues/8",
        "https://github.com/acme/shop/issues/8/",
        "https://github.com/acme/shop/issues/8/comments",
        "https://github.com/acme/shop/issues/7/comments",
        "https://token@github.com/acme/shop/issues/7",
    ] {
        let p = Project::new();
        p.config(false);
        p.story(
            "work/a.md",
            "PAY-001",
            "done",
            &format!("remote: 7\r\nremote_url: '{link}'\r\n"),
        );
        conflict(&p, "remote_conflict");
    }
    let p = Project::new();
    p.config(false);
    p.story("work/a.md", "PAY-001", "done", "");
    p.put(
        ".story-time/adoption-receipt.json",
        r#"{"schema_version":99}"#,
    );
    conflict(&p, "unknown_version");
}
#[test]
fn installed_preimages_detect_config_change_deletion_and_receipt_tampering() {
    for change in ["config", "delete", "receipt"] {
        let p = Project::new();
        p.config(false);
        p.story("work/a.md", "PAY-001", "done", "");
        let (_, r) = p.preview();
        for w in r["allowed_writes"].as_array().unwrap() {
            p.put(w["path"].as_str().unwrap(), w["content"].as_str().unwrap());
        }
        match change {
            "config" => p.put(
                ".story-time/settings.json",
                json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"work"})
                    .to_string(),
            ),
            "delete" => {
                fs::remove_file(p.0.join("work/a.md")).unwrap();
            }
            _ => p.put(
                ".story-time/adoption-receipt.json",
                r#"{"schema_version":1}"#,
            ),
        }
        conflict(&p, "preservation_diverged");
    }
}
#[test]
fn empty_files_backlog_unknown_formats_and_json_flags_have_explicit_outcomes() {
    let p = Project::new();
    p.put(
        ".story-time/settings.json",
        json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"work"}).to_string(),
    );
    let (out, r) = p.preview();
    assert!(out.status.success(), "{r}");
    assert_eq!(r["backlog"]["files"], json!([]));
    p.put("work/records.json", "{\"stories\":[]}");
    let r = conflict(&p, "unsupported_story");
    assert_eq!(r["backlog"]["files"][0]["path"], "work/records.json");
}
#[test]
fn malformed_config_identity_fields_are_not_silently_defaulted() {
    for (key, bad) in [
        ("tracker", json!([])),
        ("sync", json!(true)),
        ("backlogDir", json!(3)),
        ("prefix", Value::Null),
    ] {
        let p = Project::new();
        let mut config = json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"work"});
        config[key] = bad;
        p.put(".story-time/settings.json", config.to_string());
        p.story("work/a.md", "PAY-001", "done", "");
        conflict(&p, "invalid_config");
    }
}
#[test]
fn unsupported_adoption_commands_do_not_fall_back_to_node() {
    let p = Project::new();
    let out = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["adopt", "import"])
        .env_clear()
        .env("PATH", "/nonexistent")
        .current_dir(&p.0)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(r["allowed_writes"], json!([]));
    assert!(out.stderr.is_empty());
}
