use ring::digest::{digest, SHA256};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

const CONFIG: &str = ".story-time/settings.json";
const LEGACY: &str = ".story-time.json";
const METADATA: &str = ".story-time/lifecycle.json";
const RECEIPT: &str = ".story-time/adoption-receipt.json";

pub(crate) fn sha(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Inventory {
    root: PathBuf,
    diagnostics: Vec<Value>,
    preimages: BTreeMap<String, Value>,
    link_scope: Option<reqwest::Url>,
}
impl Inventory {
    fn conflict(&mut self, code: &str, path: &str, recovery: &str) {
        self.diagnostics
            .push(json!({"code":code,"path":path,"recovery":recovery}));
    }

    // Validate every existing component, including parents of absent targets.
    fn path(&mut self, relative: &str) -> Option<PathBuf> {
        let path = Path::new(relative);
        if relative.is_empty()
            || relative.contains('\\')
            || relative
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            self.conflict("unsafe_path", relative, "Select a relative path with normal components inside the repository; retain the original source.");
            return None;
        }
        let mut checked = self.root.clone();
        let count = path.components().count();
        for (index, component) in path.components().enumerate() {
            checked.push(component);
            match fs::symlink_metadata(&checked) {
                Ok(meta) if meta.is_symlink() || (index + 1 < count && !meta.is_dir()) => {
                    self.conflict("unsafe_path", relative, "Remove the symlink or non-directory traversal through a separately reviewed mapping; preview does not follow it.");
                    return None;
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    self.conflict(
                        "unreadable_source",
                        relative,
                        "Restore local read access and retry preview without changing the source.",
                    );
                    return None;
                }
            }
        }
        Some(checked)
    }
    fn read(&mut self, relative: &str) -> Option<Vec<u8>> {
        let path = self.path(relative)?;
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => match fs::read(path) {
                Ok(bytes) => {
                    self.preimages.insert(
                        relative.to_owned(),
                        json!({"state":"present","sha256":sha(&bytes),"bytes":bytes.len()}),
                    );
                    Some(bytes)
                }
                Err(_) => {
                    self.conflict(
                        "unreadable_source",
                        relative,
                        "Restore read access and rerun preview.",
                    );
                    None
                }
            },
            Ok(_) => {
                self.conflict(
                    "unsupported_source",
                    relative,
                    "Provide an immutable regular file through a reviewed export/import mapping.",
                );
                None
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.preimages
                    .insert(relative.to_owned(), json!({"state":"absent"}));
                None
            }
            Err(_) => {
                self.conflict(
                    "unreadable_source",
                    relative,
                    "Restore read access and rerun preview.",
                );
                None
            }
        }
    }
    fn json(&mut self, path: &str, code: &str) -> Option<(Vec<u8>, Value)> {
        let bytes = self.read(path)?;
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) if value.is_object() => Some((bytes, value)),
            _ => {
                self.conflict(code, path, "Provide a valid JSON object through a reviewed mapping; original bytes remain authoritative.");
                None
            }
        }
    }
    fn files(&mut self, relative: &str) -> Vec<Value> {
        let Some(path) = self.path(relative) else {
            return Vec::new();
        };
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(_) => {
                self.conflict(
                    "unsupported_source",
                    relative,
                    "Provide a readable flat directory of Markdown stories.",
                );
                return Vec::new();
            }
        };
        let mut names = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => match entry.file_name().into_string() {
                    Ok(name) => names.push(name),
                    Err(_) => self.conflict("unsupported_filename", relative, "Map non-UTF-8 filenames explicitly before adoption; no source is omitted silently."),
                },
                Err(_) => self.conflict("unreadable_source", relative, "Restore directory read access and repeat the complete inventory."),
            }
        }
        names.sort();
        let mut files = Vec::new();
        let mut ids = BTreeMap::new();
        let mut remotes = BTreeMap::new();
        let mut urls = BTreeMap::new();
        for name in names {
            let relative = format!("{relative}/{name}");
            let Some(path) = self.path(&relative) else {
                files.push(json!({"path":relative,"kind":"unsafe"}));
                continue;
            };
            if path.is_dir() {
                files.push(json!({"path":relative,"kind":"directory"}));
                self.conflict("nested_source", &relative, "Export nested records to an immutable flat format and review an identity-preserving import mapping.");
                continue;
            }
            let Some(bytes) = self.read(&relative) else {
                files.push(json!({"path":relative,"kind":"unreadable"}));
                continue;
            };
            let mut row = json!({"path":relative,"kind":"file","sha256":sha(&bytes),"bytes":bytes.len(),"id":null,"status":null,"remote":null,"remote_url":null});
            let parsed = if Path::new(&name).extension().is_some_and(|e| e == "md") {
                std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|text| crate::lenient_story::parse_noting_nested(text).ok())
                    .filter(|(_, nested)| nested.is_empty())
                    .map(|(story, _)| story.to_value())
            } else {
                None
            };
            if let Some(story) = parsed {
                if let Some(id) = story["id"].as_str().filter(|id| safe_identity(id)) {
                    row["id"] = json!(id);
                    if ids.insert(id.to_owned(), relative.clone()).is_some() {
                        self.conflict("duplicate_id", &relative, "Resolve duplicate local IDs with a reviewed identity mapping; neither record wins automatically.");
                    }
                } else {
                    self.conflict("unsupported_story", &relative, "Supply a nonempty scalar story ID through a reviewed mapping; retain original bytes.");
                }
                if let Some(status) = story["status"].as_str().filter(|s| safe_identity(s)) {
                    row["status"] = json!(status);
                }
                if let Some(remote) = story.get("remote").filter(|v| !v.is_null()) {
                    let identity = match remote {
                        Value::String(s) if safe_identity(s) => Some(s.clone()),
                        Value::Number(n) if n.is_u64() => Some(n.to_string()),
                        _ => None,
                    };
                    if let Some(identity) = identity {
                        row["remote"] = remote.clone();
                        let canonical = identity
                            .parse::<u64>()
                            .map(|n| n.to_string())
                            .unwrap_or(identity);
                        if remotes.insert(canonical, relative.clone()).is_some() {
                            self.conflict("remote_conflict", &relative, "Reconcile the duplicate remote identity using an immutable tracker export and reviewed mapping; do not create another issue.");
                        }
                    } else {
                        self.conflict("remote_conflict", &relative, "Review the unsupported remote identity using a tracker export; original bytes remain unchanged.");
                    }
                }
                if let Some(raw) = story.get("remote_url").filter(|v| !v.is_null()) {
                    let url = raw
                        .as_str()
                        .and_then(|s| reqwest::Url::parse(s).ok())
                        .filter(|url| {
                            matches!(url.scheme(), "http" | "https")
                                && url.username().is_empty()
                                && url.password().is_none()
                                && url.query().is_none()
                                && url.fragment().is_none()
                        });
                    if let Some(url) = url {
                        if self.link_scope.as_ref().is_some_and(|scope| {
                            url.origin() != scope.origin()
                                || !url.path().strip_prefix(scope.path()).is_some_and(|suffix| {
                                    suffix.trim_end_matches('/').parse::<u64>().is_ok()
                                        && !suffix.trim_end_matches('/').contains('/')
                                })
                        }) {
                            self.conflict("remote_conflict", &relative, "Reconcile the remote link with the configured tracker repository using an immutable export and reviewed mapping.");
                        }
                        let value = url.to_string();
                        row["remote_url"] = json!(value);
                        if urls.insert(value, relative.clone()).is_some() {
                            self.conflict("remote_conflict", &relative, "Reconcile duplicate remote URLs using an immutable tracker export and reviewed identity mapping.");
                        }
                        if let Some(remote) = row["remote"]
                            .as_u64()
                            .or_else(|| row["remote"].as_str().and_then(|s| s.parse().ok()))
                        {
                            let linked_id = url
                                .path_segments()
                                .and_then(|mut segments| segments.rfind(|s| !s.is_empty()))
                                .and_then(|last| last.parse::<u64>().ok());
                            if linked_id != Some(remote) {
                                self.conflict("remote_conflict", &relative, "Reconcile the remote ID and URL using authoritative export evidence; neither side wins automatically.");
                            }
                        }
                    } else {
                        self.conflict("remote_conflict", &relative, "Provide a credential-free remote link and reviewed identity mapping; the original link is not printed.");
                    }
                }
            } else {
                self.conflict("unsupported_story", &relative, "Export malformed or unsupported records through an explicit flat-format mapping; preserve custom data and original bytes.");
            }
            files.push(row);
        }
        files
    }
}
fn safe_identity(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
}
fn unknowns() -> Value {
    json!({
        "goal":{"value":null,"reason":"No explicit adoption planning goal was supplied."},
        "capacity":{"value":null,"reason":"No capacity budget and availability source was supplied; velocity is not capacity."},
        "epic_links":{"value":null,"reason":"Existing story links remain in original bytes; no new epic mapping is inferred."},
        "history":{"value":null,"reason":"No authoritative historical event source exists; observation does not establish past events."}
    })
}

pub fn run(args: &[String]) -> (String, u8) {
    match args.first().map(String::as_str) {
        Some("apply" | "verify" | "rollback") => return crate::adoption_apply::run(args),
        _ if args == ["preview"] => {}
        _ => {
            let report = json!({"schema_version":1,"status":"conflict","allowed_writes":[],"diagnostics":[{"code":"unsupported_command","path":null,"recovery":"Use story adopt preview, then story adopt apply|verify|rollback --plan <reviewed-preview.json> [--state <dir>]."}]});
            return (format!("{report}\n"), 2);
        }
    }
    let root = match std::env::current_dir().and_then(fs::canonicalize) {
        Ok(root) => root,
        Err(_) => {
            return (
                format!(
                    "{}\n",
                    json!({"schema_version":1,"status":"conflict","allowed_writes":[],"diagnostics":[{"code":"unreadable_root","recovery":"Run preview from a readable project root."}]})
                ),
                2,
            )
        }
    };
    let report = preview(&root);
    let code = if report["status"] == "conflict" { 2 } else { 0 };
    (format!("{report}\n"), code)
}

/// Builds the version1 preview report for a canonical repository root.
pub(crate) fn preview(root: &Path) -> Value {
    let root = root.to_path_buf();
    let mut inventory = Inventory {
        root: root.clone(),
        diagnostics: Vec::new(),
        preimages: BTreeMap::new(),
        link_scope: None,
    };
    let current = inventory.json(CONFIG, "invalid_config");
    let legacy = inventory.json(LEGACY, "invalid_config");
    // Presence, not successful parsing, decides whether two configuration owners conflict.
    let current_present = inventory.path(CONFIG).is_some_and(|p| p.exists());
    let legacy_present = inventory.path(LEGACY).is_some_and(|p| p.exists());
    if current_present && legacy_present {
        inventory.conflict("config_conflict", CONFIG, "Choose one authoritative current or legacy configuration through a reviewed mapping; preview preserves both.");
    }
    let config_input = current
        .map(|(bytes, v)| (CONFIG, bytes, v))
        .or_else(|| legacy.map(|(bytes, v)| (LEGACY, bytes, v)));
    let mut config = Value::Null;
    let mut backlog_path = "backlog".to_owned();
    let mut prefix = Value::Null;
    let mut tracker = "".to_owned();
    if let Some((path, bytes, raw)) = config_input {
        let backlog = raw.get("backlogDir").unwrap_or(&Value::Null);
        match backlog {
            Value::String(s) => backlog_path = s.clone(),
            Value::Null if raw.get("backlogDir").is_none() => {},
            _ => inventory.conflict("invalid_config", path, "Provide a string backlogDir in the authoritative configuration through a reviewed mapping."),
        }
        prefix = raw.get("prefix").cloned().unwrap_or(json!("ST"));
        if !prefix.as_str().is_some_and(safe_identity) {
            prefix = Value::Null;
            inventory.conflict(
                "invalid_config",
                path,
                "Provide an explicit valid prefix through a reviewed mapping.",
            );
        }
        tracker = match raw.get("tracker") {
            None => "files".to_owned(),
            Some(Value::String(value)) => value.clone(),
            _ => String::new(),
        };
        if !["files", "github", "gitea", "ado", "jira", "linear"].contains(&tracker.as_str()) {
            tracker.clear();
            inventory.conflict(
                "invalid_config",
                path,
                "Review the unsupported tracker and supply its export/import contract.",
            );
        }
        inventory.link_scope = match tracker.as_str() {
            "github" => raw["github"]["repo"].as_str().and_then(|repo| {
                reqwest::Url::parse(&format!("https://github.com/{repo}/issues/")).ok()
            }),
            "gitea" => (|| {
                let mut base = reqwest::Url::parse(raw["gitea"]["baseUrl"].as_str()?).ok()?;
                let owner = raw["gitea"]["owner"].as_str()?;
                let repo = raw["gitea"]["repo"].as_str()?;
                base.set_path(&format!(
                    "{}/{owner}/{repo}/issues/",
                    base.path().trim_end_matches('/')
                ));
                Some(base)
            })(),
            _ => None,
        };
        let sync = match raw.get("sync") {
            None => Some("off"),
            Some(value) => value.as_str(),
        };
        let sync = if sync.is_some_and(|value| ["off", "ask", "auto", "manual"].contains(&value)) {
            json!(sync)
        } else {
            inventory.conflict(
                "invalid_config",
                path,
                "Review the unsupported sync policy without silently changing it.",
            );
            Value::Null
        };
        config = json!({"path":path,"sha256":sha(&bytes),"bytes":bytes.len(),"tracker":if tracker.is_empty(){Value::Null}else{json!(tracker)},"sync":sync});
    } else if !current_present && !legacy_present {
        inventory.conflict("missing_config", CONFIG, "Review tracker, prefix and backlog location inputs first; initialize only missing settings through native init after review. No identity defaults are adopted.");
    }
    if backlog_path == ".story-time"
        || backlog_path.starts_with(".story-time/")
        || backlog_path == ".git"
        || backlog_path.starts_with(".git/")
    {
        inventory.conflict("unsafe_path", &backlog_path, "Select a backlog outside lifecycle, credential and Git metadata paths through a reviewed mapping.");
        backlog_path.clear();
    }
    let files = if backlog_path.is_empty() {
        Vec::new()
    } else {
        inventory.files(&backlog_path)
    };
    if files.is_empty() && !tracker.is_empty() && tracker != "files" {
        inventory.conflict("tracker_only", &backlog_path, "Obtain an immutable tracker export through a supported read/import capability, retaining IDs, links and custom data. Existing issue-write adapters cannot import; review the mapping before adoption.");
    }
    let backlog = json!({"path":if backlog_path.is_empty(){Value::Null}else{json!(backlog_path)},"prefix":prefix,"files":files});
    let preservation = json!({"repository_root":root,"config":config,"backlog":backlog});
    let migration_id = format!("adoption-v1-{}", sha(preservation.to_string().as_bytes()));
    let metadata_input = inventory.json(METADATA, "invalid_metadata");
    let receipt_input = inventory.json(RECEIPT, "invalid_metadata");
    if receipt_input
        .as_ref()
        .is_some_and(|(_, receipt)| receipt["schema_version"] != 1)
    {
        inventory.conflict("unknown_version", RECEIPT, "Use a reviewed receipt-version contract before adoption; preserve the original receipt.");
    }
    let mut source_version = json!(0);
    let mut adopted = false;
    if let Some((_, metadata)) = &metadata_input {
        source_version = metadata
            .get("schema_version")
            .cloned()
            .unwrap_or(Value::Null);
        match source_version.as_u64() {
            Some(1) => {
                adopted = metadata["preservation"] == preservation && metadata["migration_id"] == migration_id && metadata["planning_schema_version"] == 1 && metadata["unknowns"] == unknowns() && receipt_input.as_ref().is_some_and(|(_, r)| r["schema_version"] == 1 && r["migration_id"] == migration_id && r["preservation"] == preservation && r["metadata_sha256"] == sha(metadata_input.as_ref().unwrap().0.as_slice()));
                if !adopted { inventory.conflict("preservation_diverged", METADATA, "Reconcile metadata and receipt against the exact current configuration, full backlog membership and byte preimages. A version marker alone is not adoption proof."); }
            }
            _ => inventory.conflict("unknown_version", METADATA, "Use a tool with a reviewed contract for this metadata version; preserve all original files."),
        }
    }
    if receipt_input.is_some() && metadata_input.is_none() {
        inventory.conflict(
            "source_conflict",
            RECEIPT,
            "Reconcile the orphan receipt with the authoritative metadata before adoption.",
        );
    }
    let mut writes = Vec::new();
    if inventory.diagnostics.is_empty() && !adopted {
        let metadata = json!({"schema_version":1,"planning_schema_version":1,"migration_id":migration_id,"preservation":preservation,"unknowns":unknowns()});
        let content = format!("{metadata}\n");
        let receipt = json!({"schema_version":1,"migration_id":migration_id,"preservation":preservation,"metadata_sha256":sha(content.as_bytes())});
        for (index, (path, content)) in [(METADATA, content), (RECEIPT, format!("{receipt}\n"))]
            .into_iter()
            .enumerate()
        {
            writes.push(json!({"order":index+1,"path":path,"operation":"create","preimage":{"state":"absent"},"postimage_sha256":sha(content.as_bytes()),"content":content}));
        }
    }
    let conflict = !inventory.diagnostics.is_empty();
    let mut report = json!({"schema_version":1,"status":if conflict{"conflict"}else if adopted{"already_adopted"}else{"preview"},"source_version":source_version,"target_metadata_version":1,"repository":{"root":root},"config":config,"backlog":backlog,"migration_id":migration_id,"unknowns":unknowns(),"preservation_rules":{"story_bytes":"unchanged","config_bytes":"unchanged","custom_fields_and_sections":"retained in original bytes, never rendered","terminal_records":"included","tracker_state":"no reads or writes","sync_policy":"unchanged"},"preimages":inventory.preimages,"allowed_writes":writes,"diagnostics":inventory.diagnostics});
    let plan_digest = sha(report.to_string().as_bytes());
    report["plan_digest"] = json!(plan_digest);
    report
}
