use crate::adoption::{preview, sha};
use serde_json::{json, Value};
use std::{
    fs::{self, DirBuilder, File, OpenOptions, TryLockError},
    io::{ErrorKind, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

/// Test-only fault injection. `before:N` or `after:N` stops apply around journaled write N,
/// leaving the journal and backup as a crash would, and exits 75.
const FAULT: &str = "STORY_TIME_ADOPT_FAULT";
const CREDENTIAL_WORDS: [&str; 7] = [
    "token",
    "secret",
    "password",
    "credential",
    "apikey",
    "api_key",
    "private_key",
];

enum Stop {
    Conflict(Vec<Value>),
    Interrupted(Value),
}
fn diag(code: &str, path: impl Into<Value>, recovery: impl Into<String>) -> Value {
    json!({"code":code,"path":path.into(),"recovery":recovery.into()})
}
fn conflict(code: &str, path: impl Into<Value>, recovery: impl Into<String>) -> Stop {
    Stop::Conflict(vec![diag(code, path, recovery)])
}
fn io<'a>(code: &'a str, path: &'a Path) -> impl FnOnce(std::io::Error) -> Stop + 'a {
    move |e| {
        conflict(
            code,
            path.display().to_string(),
            format!("Local file operation failed ({e}). Restore access, then rerun the same command; the journal and backup are kept."),
        )
    }
}

pub fn run(args: &[String]) -> (String, u8) {
    let command = args[0].as_str();
    let result = Request::parse(&args[1..]).and_then(|request| match command {
        "apply" => apply(&request),
        "verify" => verify(&request),
        _ => rollback(&request),
    });
    let (mut report, code) = match result {
        Ok(report) => (report, 0),
        Err(Stop::Conflict(diagnostics)) => (
            json!({"schema_version":1,"status":"conflict","diagnostics":diagnostics}),
            2,
        ),
        Err(Stop::Interrupted(report)) => (report, 75),
    };
    report["command"] = json!(command);
    (format!("{report}\n"), code)
}

struct Request {
    root: PathBuf,
    plan: Value,
    state: PathBuf,
}
impl Request {
    fn parse(args: &[String]) -> Result<Self, Stop> {
        let usage = || {
            conflict("usage", Value::Null, "Use story adopt apply|verify|rollback --plan <reviewed-preview.json> [--state <directory outside the repository>].")
        };
        let (mut plan, mut state) = (None, None);
        let mut rest = args.iter();
        while let Some(flag) = rest.next() {
            let value = rest.next().ok_or_else(usage)?;
            match flag.as_str() {
                "--plan" if plan.is_none() => plan = Some(PathBuf::from(value)),
                "--state" if state.is_none() => state = Some(PathBuf::from(value)),
                _ => return Err(usage()),
            }
        }
        let plan_path = plan.ok_or_else(usage)?;
        let root = std::env::current_dir()
            .and_then(fs::canonicalize)
            .map_err(|_| {
                conflict(
                    "unreadable_root",
                    Value::Null,
                    "Run adoption from a readable project root.",
                )
            })?;
        let plan = fs::read(&plan_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .filter(Value::is_object)
            .ok_or_else(|| conflict("invalid_plan", plan_path.display().to_string(), "Save the unmodified stdout of story adopt preview to a file outside the repository and pass it with --plan."))?;
        check_plan(&plan, &root)?;
        let state = match state {
            Some(state) => state,
            None => default_state(&plan)?,
        };
        let state = resolve_state(&state, &root)?;
        Ok(Self { root, plan, state })
    }
    fn journal_path(&self) -> PathBuf {
        self.state.join("journal.json")
    }
    fn backup(&self) -> PathBuf {
        self.state.join("backup")
    }
}

fn check_plan(plan: &Value, root: &Path) -> Result<(), Stop> {
    if plan["schema_version"] != 1 {
        return Err(conflict("unsupported_version", "schema_version", "Generate a version 1 plan with this story binary's adopt preview; other plan versions have no reviewed apply contract."));
    }
    if !matches!(plan["status"].as_str(), Some("preview" | "already_adopted")) {
        let mut diagnostics = vec![diag("plan_conflict", "status", "Resolve the preview diagnostics, run story adopt preview again, and review the new plan before apply.")];
        diagnostics.extend(plan["diagnostics"].as_array().cloned().unwrap_or_default());
        return Err(Stop::Conflict(diagnostics));
    }
    let writes = plan["allowed_writes"].as_array().ok_or_else(|| {
        conflict(
            "invalid_plan",
            "allowed_writes",
            "Use the unmodified preview output.",
        )
    })?;
    for write in writes {
        let path = write["path"].as_str().unwrap_or_default();
        if !safe_relative(path) {
            return Err(conflict("unsafe_path", path, "Allowed writes must be normal relative paths inside the repository; regenerate the plan with story adopt preview."));
        }
        if credential_like(path) {
            return Err(conflict("credential_in_write_set", path, "Adoption never writes credential files. Remove the path from the plan by regenerating it with story adopt preview."));
        }
        if write["operation"] != "create" || write["preimage"] != json!({"state":"absent"}) {
            return Err(conflict("unsupported_capability", path, "Adoption v1 only creates absent lifecycle files. Replacing or deleting existing files needs a future reviewed mapping contract."));
        }
        let content = write["content"].as_str().unwrap_or_default();
        if write["postimage_sha256"] != sha(content.as_bytes()) {
            return Err(conflict(
                "invalid_plan",
                path,
                "The planned content does not match its postimage digest; regenerate the plan.",
            ));
        }
    }
    let mut unsigned = plan.clone();
    unsigned.as_object_mut().unwrap().remove("plan_digest");
    if plan["plan_digest"] != sha(unsigned.to_string().as_bytes()) {
        return Err(conflict("plan_digest_mismatch", "plan_digest", "The plan was edited after preview. Review a fresh story adopt preview and pass it unmodified."));
    }
    if plan["repository"]["root"].as_str() != root.to_str() {
        return Err(conflict(
            "repository_mismatch",
            plan["repository"]["root"].clone(),
            "Run the command from the repository root the plan was previewed in.",
        ));
    }
    Ok(())
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
fn credential_like(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    name.starts_with(".env")
        || [".env", ".pem", ".key", ".netrc"]
            .iter()
            .any(|s| name.ends_with(s))
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
}
fn has_credential_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            CREDENTIAL_WORDS.iter().any(|w| key.contains(w)) || has_credential_key(value)
        }),
        Value::Array(items) => items.iter().any(has_credential_key),
        _ => false,
    }
}

fn default_state(plan: &Value) -> Result<PathBuf, Stop> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .ok_or_else(|| conflict("missing_state_dir", Value::Null, "Pass --state <absolute directory outside the repository> for the backup and journal."))?;
    Ok(base
        .join("story-time/adoption")
        .join(plan["migration_id"].as_str().unwrap_or("unknown")))
}

fn resolve_state(state: &Path, root: &Path) -> Result<PathBuf, Stop> {
    let unsafe_state = || {
        conflict("unsafe_state_path", state.display().to_string(), "Choose an absolute --state directory outside the repository so the backup and journal stay outside every mutation target.")
    };
    if !state.is_absolute()
        || state
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(unsafe_state());
    }
    let mut existing = state.to_path_buf();
    let mut missing = Vec::new();
    while fs::symlink_metadata(&existing).is_err() {
        missing.push(existing.file_name().ok_or_else(unsafe_state)?.to_owned());
        existing.pop();
    }
    let mut resolved = fs::canonicalize(&existing).map_err(|_| unsafe_state())?;
    resolved.extend(missing.iter().rev());
    if resolved.starts_with(root) {
        return Err(unsafe_state());
    }
    Ok(resolved)
}

// The OS releases this exclusive lock when the holder exits, so a crash never leaves a stale lock.
fn lock(root: &Path) -> Result<File, Stop> {
    let file = File::open(root).map_err(io("lock_unavailable", root))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(conflict("lock_held", root.display().to_string(), "Another adoption process holds the repository lock. Wait for it to exit, then rerun the command; the journal resumes or reverses its work.")),
        Err(TryLockError::Error(e)) => Err(io("lock_unavailable", root)(e)),
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), Stop> {
    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name().unwrap().to_string_lossy()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(io("state_write_failed", &tmp))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .and_then(|_| file.write_all(bytes))
        .and_then(|_| file.sync_all())
        .and_then(|_| fs::rename(&tmp, path))
        .map_err(io("state_write_failed", path))
}
fn read_json(path: &Path) -> Result<Option<Value>, Stop> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| {
            conflict(
                "invalid_state",
                path.display().to_string(),
                "The adoption state file is not valid JSON. Restore it or choose the state directory that holds this plan's journal.",
            )
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io("invalid_state", path)(e)),
    }
}

fn load_journal(request: &Request) -> Result<Option<Value>, Stop> {
    let journal = read_json(&request.journal_path())?;
    if journal
        .as_ref()
        .is_some_and(|j| j["plan_digest"] != request.plan["plan_digest"])
    {
        return Err(conflict("state_conflict", request.journal_path().display().to_string(), "This state directory belongs to another plan. Pass the plan it was created for, or choose a new --state directory."));
    }
    Ok(journal)
}
fn save_journal(request: &Request, journal: &Value) -> Result<(), Stop> {
    write_private(&request.journal_path(), format!("{journal}\n").as_bytes())
}
fn record_failure(request: &Request, journal: &mut Value, failure: &Value) -> Result<(), Stop> {
    journal["failures"]
        .as_array_mut()
        .unwrap()
        .push(failure.clone());
    save_journal(request, journal)
}

/// Classifies a journaled step only by the bytes now at its path.
fn observe(root: &Path, step: &Value) -> &'static str {
    let relative = step["path"].as_str().unwrap_or_default();
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_symlink() => return "diverged",
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => return "pending",
            Err(_) => return "diverged",
        }
    }
    match fs::read(&path) {
        Ok(bytes) if step["postimage_sha256"] == sha(&bytes) => "complete",
        _ => "diverged",
    }
}

fn backup_inputs(request: &Request) -> Vec<String> {
    let targets: Vec<_> = request.plan["allowed_writes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["path"].clone())
        .collect();
    request.plan["preimages"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(path, image)| image["state"] == "present" && !targets.contains(&json!(path)))
        .map(|(path, _)| path.clone())
        .collect()
}

fn create_backup(request: &Request) -> Result<(), Stop> {
    let inputs = backup_inputs(request);
    let mut copies = Vec::new();
    for path in &inputs {
        let bytes = fs::read(request.root.join(path)).map_err(io("stale_plan", Path::new(path)))?;
        let sensitive = credential_like(path)
            || (request.plan["config"]["path"] == json!(path)
                && serde_json::from_slice::<Value>(&bytes).is_ok_and(|v| has_credential_key(&v)));
        if sensitive {
            return Err(conflict("credential_in_backup_set", path.as_str(), "Move credentials out of reviewed inputs (use GITEA_TOKEN or the git-ignored .story-time/.gitea.env), run story adopt preview again, and review the new plan. Adoption never copies secrets into backups."));
        }
        if request.plan["preimages"][path]["sha256"] != sha(&bytes) {
            return Err(conflict(
                "stale_plan",
                path.as_str(),
                "Source bytes changed after preview. Review a fresh story adopt preview.",
            ));
        }
        copies.push((path, bytes));
    }
    let backup = request.backup();
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(backup.join("files"))
        .map_err(io("state_write_failed", &backup))?;
    let mut files = Vec::new();
    for (index, (path, bytes)) in copies.iter().enumerate() {
        let copy = format!("files/{:04}", index + 1);
        write_private(&backup.join(&copy), bytes)?;
        files.push(json!({"path":path,"sha256":sha(bytes),"bytes":bytes.len(),"copy":copy}));
    }
    let manifest = json!({"schema_version":1,"migration_id":request.plan["migration_id"],"plan_digest":request.plan["plan_digest"],"repository_root":request.root,"files":files});
    write_private(
        &backup.join("manifest.json"),
        format!("{manifest}\n").as_bytes(),
    )
}

/// Re-reads every backup copy and checks its digest and mode against the reviewed preimages.
fn verify_backup(request: &Request) -> Result<String, Stop> {
    let backup = request.backup();
    let manifest_path = backup.join("manifest.json");
    let invalid = |path: &str| {
        conflict("backup_invalid", path, format!("Backup {} no longer matches the reviewed preimages. Do not apply or roll back from it; restore the copy or rerun preview with a new --state directory.", backup.display()))
    };
    let bytes = fs::read(&manifest_path).map_err(|_| invalid("manifest.json"))?;
    let manifest: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("manifest.json"))?;
    let mut paths = Vec::new();
    for file in manifest["files"].as_array().into_iter().flatten() {
        let path = file["path"].as_str().unwrap_or_default();
        let copy = backup.join(file["copy"].as_str().unwrap_or_default());
        let readable = fs::read(&copy).ok().filter(|b| {
            let digest = sha(b);
            file["sha256"] == digest && request.plan["preimages"][path]["sha256"] == digest
        });
        let private = fs::metadata(&copy).is_ok_and(|m| m.permissions().mode() & 0o777 == 0o600);
        if readable.is_none() || !private {
            return Err(invalid(path));
        }
        paths.push(path.to_owned());
    }
    paths.sort();
    let mut expected = backup_inputs(request);
    expected.sort();
    if manifest["plan_digest"] != request.plan["plan_digest"] || paths != expected {
        return Err(invalid("manifest.json"));
    }
    Ok(sha(&bytes))
}

fn adopted(request: &Request, current: &Value) -> bool {
    current["status"] == "already_adopted"
        && current["migration_id"] == request.plan["migration_id"]
}
fn fault(step: usize) -> Option<(String, &'static str)> {
    let fault = std::env::var(FAULT).ok()?;
    ["before", "after"]
        .into_iter()
        .find(|phase| fault == format!("{phase}:{step}"))
        .map(|phase| (fault.clone(), phase))
}

/// Classifies every journaled step by current bytes and records a divergence as a failure.
fn classify(request: &Request, journal: &mut Value) -> Result<Vec<Value>, Stop> {
    let steps = journal["steps"].as_array().unwrap().clone();
    let mut resume = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let observed = observe(&request.root, step);
        resume.push(json!({"order":step["order"],"path":step["path"],"observed":observed}));
        if observed == "diverged" {
            let path = step["path"].as_str().unwrap_or_default();
            let failure = diag("step_diverged", path, format!("Current bytes at {path} match neither the absent preimage nor postimage {}. Reconcile that file by hand, then rerun apply; source originals stay in backup {}.", step["postimage_sha256"], request.backup().display()));
            journal["steps"][index]["error"] = json!("step_diverged");
            record_failure(
                request,
                journal,
                &json!({"command":"apply","code":"step_diverged","path":path}),
            )?;
            return Err(Stop::Conflict(vec![failure]));
        }
    }
    Ok(resume)
}

fn apply(request: &Request) -> Result<Value, Stop> {
    let _lock = lock(&request.root)?;
    let current = preview(&request.root);
    let mut journal = load_journal(request)?;
    let incomplete = journal.as_ref().is_some_and(|j| {
        j["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["state"] != "complete")
    });
    if adopted(request, &current) && !incomplete {
        return Ok(
            json!({"schema_version":1,"status":"already_adopted","migration_id":request.plan["migration_id"],"state":request.state}),
        );
    }
    if let Some(journal) = journal.as_mut() {
        classify(request, journal)?;
    }
    // Diagnostics about this plan's own write targets are explained by byte classification.
    let targets: Vec<_> = request.plan["allowed_writes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["path"].clone())
        .collect();
    let fresh = request.plan["status"] == "preview"
        && current["migration_id"] == request.plan["migration_id"]
        && if journal.is_some() {
            current["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .all(|d| targets.contains(&d["path"]))
        } else {
            current["status"] == "preview" && current["plan_digest"] == request.plan["plan_digest"]
        };
    if !fresh {
        let mut diagnostics = vec![diag("stale_plan", Value::Null, "The repository no longer matches the reviewed plan. Run story adopt preview again, review it, and apply the new plan. Nothing was written.")];
        diagnostics.extend(
            current["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        return Err(Stop::Conflict(diagnostics));
    }
    if !request.backup().join("manifest.json").exists() {
        create_backup(request)?;
    }
    let backup_sha = verify_backup(request)?;
    let journal = journal.get_or_insert_with(|| {
        let steps: Vec<_> = request.plan["allowed_writes"].as_array().unwrap().iter().map(|w| json!({"order":w["order"],"path":w["path"],"operation":w["operation"],"preimage":w["preimage"],"postimage_sha256":w["postimage_sha256"],"state":"pending","error":null})).collect();
        json!({"schema_version":1,"migration_id":request.plan["migration_id"],"plan_digest":request.plan["plan_digest"],"repository_root":request.root,"backup_manifest_sha256":backup_sha,"created_dirs":[],"steps":steps,"failures":[]})
    });
    save_journal(request, journal)?;
    let resume = classify(request, journal)?;
    let count = resume.len();
    for index in 0..count {
        if resume[index]["observed"] == "complete" {
            journal["steps"][index]["state"] = json!("complete");
            continue;
        }
        let step = journal["steps"][index].clone();
        let order = index + 1;
        let interrupted = |fault: String| {
            Stop::Interrupted(
                json!({"schema_version":1,"status":"interrupted","fault":fault,"step":order,"recovery":"Rerun story adopt apply with the same plan and --state to resume from the journal."}),
            )
        };
        if let Some((fault, "before")) = fault(order) {
            save_journal(request, journal)?;
            return Err(interrupted(fault));
        }
        let relative = step["path"].as_str().unwrap();
        let target = request.root.join(relative);
        let mut dir = request.root.clone();
        for component in Path::new(relative).parent().unwrap().components() {
            dir.push(component);
            if !dir.exists() {
                // Write ahead: rollback removes only directories the journal proves this run created.
                journal["created_dirs"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(dir.strip_prefix(&request.root).unwrap()));
                save_journal(request, journal)?;
                fs::create_dir(&dir).map_err(io("write_failed", &dir))?;
            }
        }
        let content = request.plan["allowed_writes"][index]["content"]
            .as_str()
            .unwrap();
        let tmp = target.with_file_name(format!(
            ".adoption-{}.tmp",
            &request.plan["plan_digest"].as_str().unwrap()[..16]
        ));
        let _ = fs::remove_file(&tmp);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(io("write_failed", &tmp))?;
        file.write_all(content.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(io("write_failed", &tmp))?;
        // A hard link publishes the complete file atomically and refuses to replace a concurrent creation.
        let linked = fs::hard_link(&tmp, &target);
        let _ = fs::remove_file(&tmp);
        if let Err(e) = linked {
            record_failure(
                request,
                journal,
                &json!({"command":"apply","code":"step_diverged","path":relative}),
            )?;
            return Err(conflict("step_diverged", relative, format!("Another writer created {relative} during apply ({e}). Reconcile it, then rerun apply with the same plan.")));
        }
        if let Some((fault, "after")) = fault(order) {
            return Err(interrupted(fault));
        }
        journal["steps"][index]["state"] = json!("complete");
        save_journal(request, journal)?;
    }
    save_journal(request, journal)?;
    let current = preview(&request.root);
    if !adopted(request, &current) {
        let mut diagnostics = vec![diag("verification_failed", Value::Null, "Apply wrote every step but the repository does not verify as adopted. Run story adopt rollback with the same plan, then preview again.")];
        diagnostics.extend(
            current["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        return Err(Stop::Conflict(diagnostics));
    }
    Ok(
        json!({"schema_version":1,"status":"applied","migration_id":request.plan["migration_id"],"state":request.state,"backup_manifest_sha256":backup_sha,"resume":resume,"steps":journal["steps"]}),
    )
}

fn verify(request: &Request) -> Result<Value, Stop> {
    let _lock = lock(&request.root)?;
    let current = preview(&request.root);
    let journal = load_journal(request)?;
    let steps: Vec<_> = journal
        .iter()
        .flat_map(|j| j["steps"].as_array().unwrap().iter())
        .map(|s| json!({"order":s["order"],"path":s["path"],"observed":observe(&request.root, s)}))
        .collect();
    if !adopted(request, &current) {
        let mut diagnostics = vec![
            json!({"code":"not_adopted","path":null,"recovery":"Run story adopt apply with the same plan and --state to apply or resume, or story adopt rollback to reverse journaled writes.","current_status":current["status"],"steps":steps}),
        ];
        diagnostics.extend(
            current["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        return Err(Stop::Conflict(diagnostics));
    }
    let backup = if request.backup().join("manifest.json").exists() {
        json!({"state":"verified","manifest_sha256":verify_backup(request)?})
    } else {
        json!({"state":"absent"})
    };
    let read = |path: &str| fs::read(request.root.join(path)).unwrap_or_default();
    let metadata: Value =
        serde_json::from_slice(&read(".story-time/lifecycle.json")).unwrap_or(Value::Null);
    Ok(
        json!({"schema_version":1,"status":"verified","migration_id":request.plan["migration_id"],"receipt_sha256":sha(&read(".story-time/adoption-receipt.json")),"unknowns":metadata["unknowns"],"backup":backup,"steps":steps}),
    )
}

fn rollback(request: &Request) -> Result<Value, Stop> {
    let _lock = lock(&request.root)?;
    let mut journal = load_journal(request)?.ok_or_else(|| conflict("no_journal", request.journal_path().display().to_string(), "Rollback reverses only journaled writes. Pass the --state directory used by apply; without a journal nothing is changed."))?;
    let steps = journal["steps"].as_array().unwrap().clone();
    let observed: Vec<_> = steps.iter().map(|s| observe(&request.root, s)).collect();
    if let Some(index) = (0..steps.len()).rev().find(|&i| observed[i] == "diverged") {
        let path = steps[index]["path"].as_str().unwrap_or_default();
        record_failure(
            request,
            &mut journal,
            &json!({"command":"rollback","code":"rollback_diverged","path":path}),
        )?;
        return Err(conflict("rollback_diverged", path, format!("{path} changed after adoption and no longer matches postimage {}. Restore those bytes or remove the file by hand after review, then rerun rollback; nothing was removed. Source originals stay in backup {}.", steps[index]["postimage_sha256"], request.backup().display())));
    }
    let mut removed = Vec::new();
    for index in (0..steps.len()).rev() {
        if observed[index] == "complete" {
            let path = request.root.join(steps[index]["path"].as_str().unwrap());
            fs::remove_file(&path).map_err(io("rollback_failed", &path))?;
            removed.push(steps[index]["path"].clone());
        }
        if steps[index]["state"] == "complete" {
            journal["steps"][index]["state"] = json!("rolled_back");
            save_journal(request, &journal)?;
        }
    }
    let mut retained = Vec::new();
    let dirs = journal["created_dirs"].as_array().unwrap().clone();
    for dir in dirs.iter().rev() {
        match fs::remove_dir(request.root.join(dir.as_str().unwrap_or_default())) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => retained.push(dir.clone()),
        }
    }
    journal["created_dirs"] = json!(retained);
    save_journal(request, &journal)?;
    Ok(
        json!({"schema_version":1,"status":"rolled_back","migration_id":request.plan["migration_id"],"state":request.state,"removed":removed,"retained_dirs":retained}),
    )
}
