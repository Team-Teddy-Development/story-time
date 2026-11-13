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
struct Project {
    base: PathBuf,
    root: PathBuf,
}
impl Project {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "ST018-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("project");
        fs::create_dir_all(&root).unwrap();
        let p = Self { base, root };
        p.put(".story-time/settings.json",json!({"tracker":"github","sync":"auto","github":{"repo":"acme/shop"},"prefix":"PAY","backlogDir":"work","team":{"sprintDays":10,"wipLimit":1},"custom":{"preserve":true}}).to_string());
        p
    }
    fn put(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn story(&self, id: &str, status: &str, size: &str, extra: &str) {
        self.put(&format!("work/{id}.md"),format!("---\r\nid: {id}\r\ntitle: café 東京\r\nstatus: {status}\r\nsize: {size}\r\n{extra}custom: [a, '東京']\r\n---\r\n\r\n## Story\r\nPreserve these original bytes.\r\n"));
    }
    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_story"))
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", "/nonexistent")
            .output()
            .unwrap()
    }
    fn request(&self) -> Value {
        let before = snapshot(&self.root);
        let out = self.command(&["plan", "--snapshot"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(out.stderr.is_empty());
        assert_eq!(before, snapshot(&self.root));
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn run(&self, request: &Value, output: Option<&str>) -> (Output, Value) {
        let request_path = self.base.join("request.json");
        fs::write(&request_path, request.to_string()).unwrap();
        self.run_path(&request_path, output)
    }
    fn run_path(&self, request_path: &Path, output: Option<&str>) -> (Output, Value) {
        let before = snapshot(&self.root);
        let mut args = vec!["plan", "--from", request_path.to_str().unwrap()];
        if let Some(output) = output {
            args.extend(["--output", output]);
        }
        let out = self.command(&args);
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report: Value = serde_json::from_slice(&out.stdout).expect("native JSON output");
        if output.is_none() || !out.status.success() {
            assert_eq!(
                before,
                snapshot(&self.root),
                "project state changed on read-only or failing path: {report}"
            );
        }
        let after = snapshot(&self.root);
        for (path, bytes) in &before {
            if path.starts_with("work") || path.starts_with(".story-time") {
                assert_eq!(
                    after.get(path),
                    Some(bytes),
                    "source changed: {}",
                    path.display()
                );
            }
        }
        (out, report)
    }
    fn planned(&self) -> Value {
        let mut r = self.request();
        r["plan_id"] = json!("checkout-review");
        r["goal"] = json!("Make checkout failures visible");
        r["calendar"] =
            json!({"start":"2026-11-16","end":"2026-11-27","time_zone":"America/New_York"});
        r["capacity"] = json!({"budget":8,"reserve":1,"unit":"points","source":"Team supplied point budget, including reserve"});
        r
    }
    fn conflict(&self, r: &Value, output: Option<&str>, cause: &str) {
        let (out, v) = self.run(r, output);
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["status"], "conflict");
        assert!(
            v["diagnostic"].as_str().unwrap().contains(cause),
            "expected {cause}: {v}"
        );
        assert!(v["recovery"].is_string());
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                visit(root, &path, result);
            }
            let bytes = if meta.is_symlink() {
                fs::read_link(&path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec()
            } else if meta.is_dir() {
                b"directory".to_vec()
            } else {
                fs::read(&path).unwrap()
            };
            result.insert(path.strip_prefix(root).unwrap().into(), bytes);
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
fn sha(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn row<'a>(r: &'a Value, id: &str) -> &'a Value {
    r["dispositions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap()
}

#[test]
fn native_plan_dispatch_needs_no_node() {
    let out = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["plan", "--help"])
        .env_clear()
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "native plan dispatch failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("story plan --from"));
}
#[test]
fn mixed_inventory_capacity_completed_dependencies_and_epics_are_separate() {
    let p = Project::new();
    p.story("PAY-000", "done", "2", "");
    p.story(
        "PAY-001",
        "ready",
        "3",
        "depends_on: [PAY-000]\r\nepic: EP-1\r\n",
    );
    p.story("PAY-002", "ready", "3", "");
    p.story("PAY-003", "ready", "8", "");
    p.story("PAY-004", "ready", "1", "depends_on: [PAY-003]\r\n");
    p.story("PAY-005", "draft", "5", "");
    p.story("PAY-006", "archived", "1", "");
    p.story("PAY-007", "split", "1", "");
    p.story("PAY-008", "in-progress", "1", "");
    let mut r = p.planned();
    r["epic_evidence"] = json!({"PAY-001":{"disposition":"mapped","epic":"EP-1","evidence":"EP-1 outcome in reviewed story-map"},"PAY-002":{"disposition":"justified-standalone","epic":null,"evidence":"Independent maintenance outcome"}});
    let (out, a) = p.run(&r, None);
    assert!(out.status.success(), "{a}");
    assert_eq!(a["selected"], json!(["PAY-001", "PAY-002"]));
    assert_eq!(a["capacity"]["selected_points"], 6.0);
    assert_eq!(a["capacity"]["remaining"], 1.0);
    assert_eq!(a["dependency_order"], a["selected"]);
    assert_eq!(row(&a, "PAY-001")["dependencies"][0]["done"], true);
    assert_eq!(row(&a, "PAY-003")["selection"], "deferred");
    assert_eq!(row(&a, "PAY-004")["selection"], "blocked");
    assert!(row(&a, "PAY-004")["reasons"][0]
        .as_str()
        .unwrap()
        .contains("PAY-003"));
    assert_eq!(row(&a, "PAY-005")["selection"], "needs-refinement");
    assert_eq!(row(&a, "PAY-008")["selection"], "needs-refinement");
    assert_eq!(
        row(&a, "PAY-002")["epic"]["disposition"],
        "justified-standalone"
    );
    assert_eq!(row(&a, "PAY-003")["epic"]["disposition"], "unresolved-epic");
    let ids: Vec<_> = a["dispositions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["PAY-001", "PAY-002", "PAY-003", "PAY-004", "PAY-005", "PAY-008"]
    );
    assert_eq!(a["assessment"], "provisional");
    assert!(a["missing_inputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["field"] == "publication.tracker_sprint_fields"));
    assert_eq!(a["wip_policy"]["limit_per_person"], 1);
    assert_eq!(a["publication"]["writes"], json!([]));
    assert_eq!(a["publication"]["sprint_fields_supported"], false);
    assert_eq!(
        a["source"]["config"]["sha256"],
        sha(&fs::read(p.root.join(".story-time/settings.json")).unwrap())
    );
}
#[test]
fn missing_inputs_are_provisional_forecasts_never_commit_and_zero_is_valid() {
    let p = Project::new();
    p.story("PAY-001", "ready", "3", "");
    p.story("PAY-002", "ready", "null", "");
    let mut r = p.request();
    r["forecasts"] = json!([{"label":"Later opportunity","ids":["PAY-001"]}]);
    let (out, a) = p.run(&r, None);
    assert!(out.status.success(), "{a}");
    assert_eq!(a["assessment"], "provisional");
    assert_eq!(a["selected"], json!([]));
    assert!(a["capacity"].is_null());
    for field in [
        "goal",
        "capacity",
        "calendar.start",
        "calendar.end",
        "calendar.time_zone",
        "size.PAY-002",
    ] {
        assert!(
            a["missing_inputs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["field"] == field),
            "{field}: {a}"
        );
    }
    assert_eq!(a["forecasts"][0]["commitment"], false);
    assert!(a["forecasts"][0]["dates"].is_null());
    r = p.planned();
    r["capacity"]["budget"] = json!(0);
    r["capacity"]["reserve"] = json!(0);
    let (out, a) = p.run(&r, None);
    assert!(out.status.success());
    assert_eq!(a["selected"], json!([]));
    assert_eq!(a["capacity"]["remaining"], 0.0);
}
#[test]
fn candidate_order_is_reviewed_not_re_ranked_and_dependencies_cannot_be_planned_together() {
    let p = Project::new();
    p.story("PAY-001", "ready", "5", "");
    p.story("PAY-002", "ready", "3", "");
    p.story("PAY-003", "ready", "1", "depends_on: [PAY-002]\r\n");
    let mut r = p.planned();
    r["candidate_order"] = json!(["PAY-002", "PAY-001", "PAY-003"]);
    let (_, a) = p.run(&r, None);
    assert_eq!(a["selected"], json!(["PAY-002"]));
    assert_eq!(row(&a, "PAY-003")["selection"], "blocked");
    r["candidate_order"] = json!(["PAY-001"]);
    let (_, a) = p.run(&r, None);
    assert_eq!(a["selected"], json!(["PAY-001"]));
    assert_eq!(row(&a, "PAY-002")["selection"], "deferred");
}
#[test]
fn empty_and_legacy_sources_and_missing_configuration_remain_read_only() {
    for legacy in [false, true] {
        let p = Project::new();
        if legacy {
            fs::rename(
                p.root.join(".story-time/settings.json"),
                p.root.join(".story-time.json"),
            )
            .unwrap();
        }
        let r = p.request();
        let (out, a) = p.run(&r, None);
        assert!(out.status.success());
        assert_eq!(a["dispositions"], json!([]));
        assert_eq!(a["assessment"], "provisional");
        assert!(a["goal"].is_null());
    }
    let p = Project::new();
    fs::remove_file(p.root.join(".story-time/settings.json")).unwrap();
    let r = p.request();
    let (_, a) = p.run(&r, None);
    assert!(a["source"]["config"].is_null());
    assert_eq!(a["assessment"], "provisional");
}
#[test]
fn malformed_requests_versions_capacity_candidates_and_epic_conflicts_preserve_bytes() {
    let p = Project::new();
    p.story("PAY-001", "ready", "3", "");
    p.put("docs/planning/keep.json", "original JSON");
    p.put("docs/planning/keep.md", "original Markdown");
    let base = p.planned();
    for (field, value, cause) in [
        ("schema_version", json!(2), "unsupported_schema"),
        ("schema_version", json!("1"), "malformed_request"),
        (
            "candidate_order",
            json!(["PAY-001", "PAY-001"]),
            "duplicate_candidate",
        ),
        ("candidate_order", json!(["PAY-999"]), "unknown_candidate"),
        ("plan_id", json!(" "), "plan_id"),
        ("readiness", json!(true), "unknown field"),
    ] {
        let mut r = base.clone();
        r[field] = value;
        p.conflict(&r, Some("docs/planning/keep.json"), cause);
    }
    for (budget, reserve, unit) in [
        (-1, 0, "points"),
        (8, 9, "points"),
        (8, -1, "points"),
        (8, 1, "days"),
    ] {
        let mut r = base.clone();
        r["capacity"] = json!({"budget":budget,"reserve":reserve,"unit":unit,"source":"reviewed"});
        p.conflict(&r, Some("docs/planning/keep.json"), "invalid_capacity");
    }
    let path = p.base.join("bad.json");
    fs::write(&path, b"{bad").unwrap();
    let (out, a) = p.run_path(&path, Some("docs/planning/keep.json"));
    assert_eq!(out.status.code(), Some(2));
    assert!(a["diagnostic"]
        .as_str()
        .unwrap()
        .contains("malformed_request"));
    let mut r = base;
    r["epic_evidence"] =
        json!({"PAY-001":{"disposition":"mapped","epic":null,"evidence":"reviewed"}});
    p.conflict(&r, None, "missing_epic");
}
#[test]
fn stale_config_identity_content_and_membership_are_rejected() {
    for change in [
        "config", "identity", "content", "added", "removed", "terminal",
    ] {
        let p = Project::new();
        p.story("PAY-001", "ready", "3", "");
        let mut r = p.planned();
        match change {
            "config" => p.put(
                ".story-time/settings.json",
                json!({"prefix":"PAY","backlogDir":"work","sync":"off"}).to_string(),
            ),
            "identity" => r["source"]["repository"]["root"] = json!("/different"),
            "content" => p.story("PAY-001", "draft", "3", ""),
            "added" => p.story("PAY-002", "ready", "1", ""),
            "removed" => fs::remove_file(p.root.join("work/PAY-001.md")).unwrap(),
            _ => p.story("PAY-002", "done", "1", ""),
        }
        p.conflict(&r, Some("docs/planning/new.json"), "stale_source");
    }
}
#[test]
fn duplicate_cycle_unknown_status_size_and_nested_data_fail_closed() {
    for (extra, status, size, cause) in [
        (
            "depends_on: [PAY-001]\r\n",
            "ready",
            "3",
            "dependency_cycle",
        ),
        ("", "imaginary", "3", "unknown or missing status"),
        ("", "ready", "huge", "unknown_size"),
        ("nested: {bad: true}\r\n", "ready", "3", "malformed_story"),
        ("depends_on: wrong\r\n", "ready", "3", "depends_on"),
    ] {
        let p = Project::new();
        let r = p.request();
        p.story("PAY-001", status, size, extra);
        p.conflict(&r, None, cause);
    }
    let p = Project::new();
    p.story("PAY-001", "ready", "3", "");
    let r = p.request();
    let bytes = fs::read(p.root.join("work/PAY-001.md")).unwrap();
    p.put("work/duplicate.md", bytes);
    p.conflict(&r, None, "duplicate_id");
    let p = Project::new();
    p.story("PAY-001", "ready", "3", "depends_on: [PAY-002]\r\n");
    let r = p.request();
    p.story("PAY-002", "ready", "3", "depends_on: [PAY-001]\r\n");
    p.conflict(&r, None, "dependency_cycle");
}
#[test]
fn output_pair_requires_both_exact_preimages_and_rechecks_markdown_before_writes() {
    let p = Project::new();
    p.story("PAY-001", "ready", "3", "");
    let r = p.planned();
    let (out, a) = p.run(&r, Some("docs/planning/plan.json"));
    assert!(out.status.success(), "{a}");
    let json_bytes = fs::read(p.root.join("docs/planning/plan.json")).unwrap();
    let md_bytes = fs::read(p.root.join("docs/planning/plan.md")).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&json_bytes).unwrap(), a);
    assert!(String::from_utf8_lossy(&md_bytes).contains("PAY-001"));
    p.conflict(&r, Some("docs/planning/plan.json"), "output_conflict");
    let mut reviewed = r.clone();
    reviewed["output_preimages"] = json!({"docs/planning/plan.json":sha(&json_bytes)});
    p.conflict(&reviewed, Some("docs/planning/plan.json"), "BOTH");
    reviewed["output_preimages"]["docs/planning/plan.md"] = json!(sha(&md_bytes));
    reviewed["goal"] = json!("New reviewed goal");
    let (out, a) = p.run(&reviewed, Some("docs/planning/plan.json"));
    assert!(out.status.success(), "{a}");
    assert_eq!(a["goal"], "New reviewed goal");
    p.conflict(
        &reviewed,
        Some("docs/planning/plan.json"),
        "output_conflict",
    );
    let p = Project::new();
    let r = p.request();
    p.put("docs/planning/only.md", "Do not replace");
    p.conflict(&r, Some("docs/planning/only.json"), "output_conflict");
    assert!(!p.root.join("docs/planning/only.json").exists());
}
#[test]
fn output_paths_symlinks_locks_and_predictable_write_conflicts_leave_no_partial_plan() {
    let p = Project::new();
    let r = p.request();
    for target in [
        "elsewhere.json",
        "docs/planning/../../escape.json",
        "/tmp/escape.json",
        "docs/planning/bad.md",
    ] {
        p.conflict(&r, Some(target), "unsafe_");
    }
    p.put("docs/planning/taken.md/child", "directory conflict");
    p.conflict(&r, Some("docs/planning/taken.json"), "output_conflict");
    p.put("docs/planning/.story-plan.lock", "another writer");
    p.conflict(&r, Some("docs/planning/locked.json"), "output_lock");
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&p.base, p.root.join("docs/planning/link")).unwrap();
        p.conflict(&r, Some("docs/planning/link/escape.json"), "symlink_path");
        symlink(
            p.base.join("outside"),
            p.root.join("docs/planning/dangling.md"),
        )
        .unwrap();
        p.conflict(&r, Some("docs/planning/dangling.json"), "symlink_path");
        symlink(&p.base, p.root.join("work")).unwrap();
        p.conflict(&r, None, "symlink_path");
    }
}

#[test]
fn eligibility_and_unknown_dependencies_are_explicit_and_never_override_readiness() {
    let p = Project::new();
    p.story("PAY-001", "ready", "3", "");
    p.story("PAY-002", "ready", "3", "depends_on: [PAY-999]\r\n");
    p.story("PAY-003", "draft", "1", "");
    let mut r = p.planned();
    r["exclusions"] = json!({"PAY-001":"Missing sandbox access evidence"});
    let (out, a) = p.run(&r, None);
    assert!(out.status.success(), "{a}");
    assert_eq!(a["assessment"], "provisional");
    assert_eq!(a["selected"], json!([]));
    assert_eq!(row(&a, "PAY-001")["selection"], "blocked");
    assert!(a["missing_inputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["field"] == "eligibility.PAY-001"));
    assert!(row(&a, "PAY-002")["reasons"][0]
        .as_str()
        .unwrap()
        .contains("PAY-999"));
    assert_eq!(row(&a, "PAY-003")["selection"], "needs-refinement");
    r["exclusions"] = json!({"PAY-999":"missing access"});
    p.conflict(&r, None, "invalid_exclusion");
}

#[test]
fn calendar_forecasts_epic_maps_and_legacy_conflicts_fail_without_publication() {
    let p = Project::new();
    p.story("PAY-001", "ready", "3", "epic: EP-1\r\n");
    p.put("work/EP-1.map.md", "# Original epic outcome\n");
    let base = p.planned();
    for calendar in [
        json!({"start":"not-a-date"}),
        json!({"start":"2026-11-27","end":"2026-11-16"}),
        json!({"time_zone":" "}),
    ] {
        let mut r = base.clone();
        r["calendar"] = calendar;
        p.conflict(&r, Some("docs/planning/new.json"), "invalid_calendar");
    }
    let mut r = base.clone();
    r["forecasts"] = json!([{"label":"Later", "ids":["PAY-001"]}]);
    p.conflict(&r, None, "malformed_forecast");
    r["forecasts"] = json!([{"label":"Later", "ids":["PAY-999"]}]);
    p.conflict(&r, None, "malformed_forecast");
    r = base.clone();
    r["epic_evidence"] = json!({"PAY-001":{"disposition":"justified-standalone","epic":null,"evidence":"Contradicts source"}});
    p.conflict(&r, None, "epic_conflict");
    p.put("work/EP-1.map.md", "# Changed epic outcome\n");
    p.conflict(&base, Some("docs/planning/new.json"), "stale_source");
    let r = p.request();
    p.put(".story-time.json", "{}");
    p.conflict(&r, None, "config_conflict");
}

#[test]
fn output_never_overlaps_configured_backlog_even_with_reviewed_preimages() {
    for name in ["PAY-001", "roadmap.map"] {
        let p = Project::new();
        p.put(
            ".story-time/settings.json",
            json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"docs/planning"})
                .to_string(),
        );
        let source = format!("docs/planning/{name}.md");
        let bytes = if name.ends_with(".map") {
            "# Existing epic map\nPreserve this source.\n".to_owned()
        } else {
            "---\nid: PAY-001\nstatus: ready\nsize: 3\n---\n\n## Story\nOriginal.\n".to_owned()
        };
        p.put(&source, &bytes);
        let mut r = p.request();
        let output = format!("docs/planning/{name}.json");
        r["output_preimages"] = json!({output.clone(): null,source.clone():sha(bytes.as_bytes())});
        let before = snapshot(&p.root);
        let (out, a) = p.run(&r, Some(&output));
        assert_eq!(out.status.code(), Some(2), "{a}");
        assert!(
            a["diagnostic"]
                .as_str()
                .unwrap()
                .contains("source_output_overlap"),
            "{a}"
        );
        assert_eq!(before, snapshot(&p.root));
    }
}

#[test]
fn fractional_point_budget_uses_exact_decimal_arithmetic() {
    let p = Project::new();
    p.story("PAY-001", "ready", "0.1", "");
    p.story("PAY-002", "ready", "0.1", "");
    let mut r = p.planned();
    r["capacity"] = json!({"budget":0.3,"reserve":0.1,"unit":"points","source":"Reviewed fractional point budget"});
    let (out, a) = p.run(&r, None);
    assert!(out.status.success(), "{a}");
    assert_eq!(a["selected"], json!(["PAY-001", "PAY-002"]));
    assert_eq!(a["capacity"]["selected_points"], 0.2);
    assert_eq!(a["capacity"]["remaining"], 0.0);
    for budget in [0.1234567, 1_000_000_001.0] {
        r["capacity"]["budget"] = json!(budget);
        r["capacity"]["reserve"] = json!(0);
        p.conflict(&r, Some("docs/planning/precision.json"), "point_precision");
    }
}

#[test]
fn complete_files_only_plan_is_planned_without_tracker_publication() {
    let p = Project::new();
    p.put(".story-time/settings.json",json!({"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"work","team":{"sprintDays":10,"wipLimit":1}}).to_string());
    p.story("PAY-001", "ready", "3", "");
    let r = p.planned();
    let (out, a) = p.run(&r, None);
    assert!(out.status.success(), "{a}");
    assert_eq!(a["assessment"], "planned");
    assert_eq!(a["selected"], json!(["PAY-001"]));
    assert_eq!(a["publication"]["mode"], "local-only");
    assert_eq!(a["publication"]["writes"], json!([]));
}

#[test]
fn malformed_config_and_unknown_nested_request_fields_cannot_masquerade_as_missing_inputs() {
    for config in [
        json!([]),
        json!({"team":"ten"}),
        json!({"tracker":3}),
        json!({"tracker":"imaginary"}),
        json!({"sync":"automatic"}),
        json!({"prefix":"../PAY"}),
        json!({"team":{"sprintDays":-1}}),
    ] {
        let p = Project::new();
        let r = p.request();
        p.put(".story-time/settings.json", config.to_string());
        let (out, a) = p.run(&r, Some("docs/planning/new.json"));
        assert_eq!(out.status.code(), Some(2));
        assert!(
            a["diagnostic"].as_str().unwrap().starts_with("malformed_"),
            "{a}"
        );
    }
    let p = Project::new();
    let mut r = p.request();
    r["calendar"] = json!({"start":null,"end":null,"time_zone":null,"ready":true});
    p.conflict(&r, None, "unknown field");
}
