//! The repositories of a hosted server (ADR 0007). Each registered repository is a bare clone
//! under the data directory. A read fetches the clone when its last fetch is older than the
//! registry's TTL, then reads the story files of one commit from a directory that holds only
//! `.story-time`, the backlog, and `model`. One lock for each repository serializes its git
//! commands. A repository that the server cannot reach does not stop the other repositories.

use super::{
    forge_auth::App,
    registry::{branch_ok, Registry, RegistryRepo},
};
use crate::Failure;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

pub(super) const USAGE: &str = "[--registry FILE [--data DIR] [--github-app-id-file FILE --github-app-key-file FILE] [--github-api-url URL]]";
const DEFAULT_DATA: &str = "/data";
const DEFAULT_API: &str = "https://api.github.com";
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);
/// Commit directories kept for each repository after their read ends.
const TREES_KEPT: usize = 4;
const ERROR_LIMIT: usize = 300;

/// The secret strings that no tool result may show. Installation tokens join it as they are
/// granted, so the redactor reads it for each result.
pub(super) type Secrets = Arc<RwLock<Vec<String>>>;

pub(super) struct Workspace {
    registry: Registry,
    data: PathBuf,
    app: Option<App>,
    secrets: Secrets,
    slots: Vec<Mutex<Slot>>,
    /// What `story_repos` reports. It is apart from the slot, so that the report does not wait
    /// for a fetch that holds the slot.
    status: Vec<Mutex<Status>>,
    next: AtomicU64,
}

#[derive(Default)]
struct Status {
    state: &'static str,
    sha: Option<String>,
    fetched_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Default)]
struct Slot {
    /// A fetch has succeeded, in this process or an earlier one, so the clone can be read.
    cloned: bool,
    default_branch: Option<String>,
    /// When the last fetch was tried. A failed fetch is not tried again before the TTL passes.
    tried: Option<Instant>,
    fetched_at: Option<String>,
    last_error: Option<String>,
    trees: Vec<Arc<Tree>>,
    /// The secrets of the last token this repository used, which leave `Secrets` with it.
    secrets: Vec<String>,
}

/// The story files of one commit. The directory is removed when the last reader drops it.
pub(super) struct Tree {
    sha: String,
    pub(super) path: PathBuf,
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// One repository at one commit, ready to read.
#[derive(Clone)]
pub(super) struct Checkout {
    pub(super) sha: String,
    pub(super) fetched_at: String,
    pub(super) tree: Arc<Tree>,
}

pub(super) enum Refusal {
    UnknownRepository,
    /// The server has no clone to read. The text says why, without a secret.
    Unavailable(String),
    RefNotFound,
    Git(String),
}

/// Splits the registry flags off `args`. `--registry` selects the hosted repositories; without
/// it, the other flags of this group are refused.
pub(super) fn launch(
    args: &[String],
    secrets: &Secrets,
) -> Result<(Vec<String>, Option<Workspace>), Failure> {
    let refuse = |message: String| Failure {
        code: 2,
        message: format!("story mcp: {message}"),
    };
    let mut rest = Vec::new();
    let mut flags = std::collections::HashMap::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--registry"
            | "--data"
            | "--github-app-id-file"
            | "--github-app-key-file"
            | "--github-api-url" => {
                let value = args
                    .next()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| refuse(format!("{arg} needs a value.")))?;
                flags.insert(&arg[2..], value.clone());
            }
            _ => rest.push(arg.clone()),
        }
    }
    let Some(file) = flags.get("registry") else {
        return match flags.keys().next() {
            Some(flag) => Err(refuse(format!("--{flag} needs --registry."))),
            None => Ok((rest, None)),
        };
    };
    if rest.iter().any(|arg| arg == "--root" || arg == "--write") {
        return Err(refuse(
            "--registry serves the registered repositories; it does not take --root or --write."
                .into(),
        ));
    }
    let read = |flag: &str, path: &String| {
        fs::read_to_string(path).map_err(|e| refuse(format!("--{flag} {path}: {}", e.kind())))
    };
    let registry = Registry::parse(&read("registry", file)?)
        .map_err(|e| refuse(format!("--registry {file}: {e}.")))?;
    let api = flags.get("github-api-url").map_or(DEFAULT_API, |s| s);
    let app = match (
        flags.get("github-app-id-file"),
        flags.get("github-app-key-file"),
    ) {
        (None, None) => None,
        (Some(id), Some(key)) => Some(
            App::new(
                &read("github-app-id-file", id)?,
                &read("github-app-key-file", key)?,
                api,
            )
            .map_err(|e| refuse(format!("--github-app-key-file {key}: {e}.")))?,
        ),
        _ => {
            return Err(refuse(
                "--github-app-id-file and --github-app-key-file go together.".into(),
            ))
        }
    };
    let data = flags.get("data").map_or(DEFAULT_DATA, |s| s);
    let workspace = Workspace::open(registry, Path::new(data), app, secrets.clone())
        .map_err(|e| refuse(format!("--data {data}: {e}.")))?;
    Ok((rest, Some(workspace)))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl Workspace {
    /// The workspace under `data`. Commit directories that an earlier process left are removed.
    fn open(
        registry: Registry,
        data: &Path,
        app: Option<App>,
        secrets: Secrets,
    ) -> Result<Self, String> {
        let made = |path: PathBuf| {
            fs::create_dir_all(&path)
                .map(|()| path)
                .map_err(|e| format!("could not create the data directory ({})", e.kind()))
        };
        made(data.to_owned())?;
        let data = data
            .canonicalize()
            .map_err(|e| format!("could not resolve the data directory ({})", e.kind()))?;
        let _ = fs::remove_dir_all(data.join("trees"));
        for name in ["repos", "trees", "home", "tmp"] {
            made(data.join(name))?;
        }
        let workspace = Self {
            slots: registry.repos.iter().map(|_| Mutex::default()).collect(),
            status: registry.repos.iter().map(|_| Mutex::default()).collect(),
            registry,
            data,
            app,
            secrets,
            next: AtomicU64::new(0),
        };
        for (index, repo) in workspace.registry.repos.iter().enumerate() {
            // A clone that an earlier process fetched can be read before the first fetch here.
            let marker = fs::read_to_string(workspace.bare(repo).join("story-time-fetched-at"));
            if let Ok(fetched_at) = marker {
                let mut slot = workspace.slots[index].lock().unwrap();
                slot.default_branch = repo.default_branch.clone().or_else(|| {
                    workspace
                        .git(repo, &["symbolic-ref", "--short", "HEAD"], &[], None)
                        .ok()
                        .map(|branch| branch.trim().to_owned())
                });
                slot.cloned = slot.default_branch.is_some();
                slot.fetched_at = Some(fetched_at.trim().to_owned());
                workspace.publish(index, &slot);
            }
        }
        Ok(workspace)
    }

    pub(super) fn repos(&self) -> &[RegistryRepo] {
        &self.registry.repos
    }

    fn bare(&self, repo: &RegistryRepo) -> PathBuf {
        self.data.join("repos").join(format!("{}.git", repo.key))
    }

    /// Runs `git` in the clone of `repo`, with an environment that holds nothing of the
    /// server's own: no token variable, no user config, no system config.
    fn git(
        &self,
        repo: &RegistryRepo,
        args: &[&str],
        environment: &[(String, String)],
        input: Option<&[u8]>,
    ) -> Result<String, String> {
        self.git_within(repo, args, environment, input, LOCAL_TIMEOUT)
    }

    fn git_within(
        &self,
        repo: &RegistryRepo,
        args: &[&str],
        environment: &[(String, String)],
        input: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<String, String> {
        let operation = args
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .copied()
            .unwrap_or("git");
        let mut child = Command::new("git")
            .arg("--git-dir")
            .arg(self.bare(repo))
            .args(["-c", "core.symlinks=false", "-c", "gc.auto=0"])
            .args([
                "-c",
                "maintenance.auto=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.data.join("home"))
            .env("TMPDIR", self.data.join("tmp"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("LC_ALL", "C")
            .envs(environment.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .current_dir(&self.data)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("git {operation} did not start ({})", e.kind()))?;
        let reader = |mut pipe: Box<dyn Read + Send>| {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = pipe.read_to_end(&mut bytes);
                bytes
            })
        };
        let stdout = reader(Box::new(child.stdout.take().unwrap()));
        let stderr = reader(Box::new(child.stderr.take().unwrap()));
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            let _ = stdin.write_all(input);
        }
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if started.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        match status {
            Some(status) if status.success() => Ok(String::from_utf8_lossy(&stdout).into_owned()),
            Some(_) => Err(format!(
                "git {operation} failed: {}",
                self.plain(&String::from_utf8_lossy(&stderr))
            )),
            None => Err(format!(
                "git {operation} did not end in {} seconds",
                timeout.as_secs()
            )),
        }
    }

    /// One line of git's error text for an operator: no secret, no data directory, bounded.
    fn plain(&self, text: &str) -> String {
        let mut line = text
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("no error text")
            .trim()
            .replace(&*self.data.to_string_lossy(), "<data>");
        for secret in self
            .secrets
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            line = line.replace(secret.as_str(), "[redacted]");
        }
        line.chars().take(ERROR_LIMIT).collect()
    }

    /// Fetches `repo`, after making its bare clone when there is none.
    fn fetch(&self, repo: &RegistryRepo, slot: &mut Slot) -> Result<(), String> {
        let bare = self.bare(repo);
        let access = match &self.app {
            Some(app) => app.git_environment(&repo.clone)?,
            None => Default::default(),
        };
        if access.secrets != slot.secrets {
            let mut secrets = self.secrets.write().unwrap_or_else(|e| e.into_inner());
            secrets.retain(|secret| !slot.secrets.contains(secret));
            secrets.extend(access.secrets.iter().cloned());
            slot.secrets = access.secrets.clone();
        }
        if !bare.join("HEAD").is_file() {
            let _ = fs::remove_dir_all(&bare);
            self.git(
                repo,
                &["init", "-q", "--bare", "--initial-branch=main"],
                &[],
                None,
            )?;
            self.git(repo, &["remote", "add", "origin", &repo.clone], &[], None)?;
            let refspec = "+refs/heads/*:refs/heads/*";
            self.git(repo, &["config", "remote.origin.fetch", refspec], &[], None)?;
        }
        let branch = match &repo.default_branch {
            Some(branch) => branch.clone(),
            None => {
                let head = self.git_within(
                    repo,
                    &["ls-remote", "--symref", "origin", "HEAD"],
                    &access.environment,
                    None,
                    NETWORK_TIMEOUT,
                )?;
                head.lines()
                    .find_map(|line| {
                        line.strip_prefix("ref: refs/heads/")?
                            .strip_suffix("\tHEAD")
                    })
                    .filter(|branch| branch_ok(branch))
                    .ok_or("the remote names no default branch")?
                    .to_owned()
            }
        };
        self.git_within(
            repo,
            &["fetch", "--quiet", "--prune", "--no-tags", "origin"],
            &access.environment,
            None,
            NETWORK_TIMEOUT,
        )?;
        let head = format!("refs/heads/{branch}");
        self.git(repo, &["symbolic-ref", "HEAD", &head], &[], None)?;
        let at = now();
        fs::write(bare.join("story-time-fetched-at"), &at)
            .map_err(|e| format!("could not record the fetch ({})", e.kind()))?;
        slot.default_branch = Some(branch);
        slot.fetched_at = Some(at);
        slot.cloned = true;
        Ok(())
    }

    /// Fetches when the last try is older than the TTL. A failure is recorded and, when a clone
    /// exists, the read goes on with the commit of the last fetch.
    fn refresh(&self, index: usize, slot: &mut Slot) {
        let repo = &self.registry.repos[index];
        let ttl = Duration::from_secs(self.registry.fetch_ttl_seconds);
        if slot.tried.is_some_and(|tried| tried.elapsed() < ttl) {
            return;
        }
        slot.last_error = self.fetch(repo, slot).err();
        slot.tried = Some(Instant::now());
        self.publish(index, slot);
        if slot.last_error.is_some() {
            let line = json!({"at": now(), "event": "fetch_failed", "repository": repo.key});
            eprintln!("{line}");
        }
    }

    /// Fetches every repository once, so that the first read does not wait for a clone.
    pub(super) fn warm(&self) {
        for index in 0..self.slots.len() {
            let mut slot = self.slots[index].lock().unwrap_or_else(|e| e.into_inner());
            self.refresh(index, &mut slot);
        }
    }

    fn head(&self, repo: &RegistryRepo, branch: &str) -> Option<String> {
        let commit = format!("refs/heads/{branch}^{{commit}}");
        let sha = self
            .git(
                repo,
                &["rev-parse", "--verify", "--quiet", &commit],
                &[],
                None,
            )
            .ok()?;
        let sha = sha.trim();
        (sha.len() >= 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then(|| sha.to_owned())
    }

    /// Copies the state of a slot to the report of `story_repos`.
    fn publish(&self, index: usize, slot: &Slot) {
        let repo = &self.registry.repos[index];
        let branch = slot.default_branch.as_deref().filter(|_| slot.cloned);
        let status = Status {
            state: match (slot.cloned, slot.tried) {
                (true, _) => "ready",
                (false, None) => "cloning",
                (false, Some(_)) => "unavailable",
            },
            sha: branch.and_then(|branch| self.head(repo, branch)),
            fetched_at: slot.fetched_at.clone().filter(|_| slot.cloned),
            last_error: slot.last_error.clone(),
        };
        *self.status[index].lock().unwrap_or_else(|e| e.into_inner()) = status;
    }

    /// The story files of `key` at the head of `branch`, or of the default branch.
    pub(super) fn checkout(&self, key: &str, branch: Option<&str>) -> Result<Checkout, Refusal> {
        let index = self
            .registry
            .repos
            .iter()
            .position(|repo| repo.key == key)
            .ok_or(Refusal::UnknownRepository)?;
        let repo = &self.registry.repos[index];
        let mut slot = self.slots[index].lock().unwrap_or_else(|e| e.into_inner());
        self.refresh(index, &mut slot);
        let (true, Some(fetched_at), Some(default)) = (
            slot.cloned,
            slot.fetched_at.clone(),
            slot.default_branch.clone(),
        ) else {
            let reason = slot.last_error.clone();
            return Err(Refusal::Unavailable(
                reason.unwrap_or_else(|| "the repository is not cloned yet".into()),
            ));
        };
        let branch = branch.unwrap_or(&default);
        if !branch_ok(branch) {
            return Err(Refusal::RefNotFound);
        }
        let sha = self.head(repo, branch).ok_or(Refusal::RefNotFound)?;
        let tree = match slot.trees.iter().position(|tree| tree.sha == sha) {
            Some(found) => slot.trees.remove(found),
            None => self.materialize(repo, &sha).map_err(Refusal::Git)?,
        };
        slot.trees.insert(0, tree.clone());
        slot.trees.truncate(TREES_KEPT);
        Ok(Checkout {
            sha,
            fetched_at,
            tree,
        })
    }

    /// Writes the story files of commit `sha` into a new directory: `.story-time`, then the
    /// backlog directory that its settings name, and `model`. Nothing else leaves the clone.
    fn materialize(&self, repo: &RegistryRepo, sha: &str) -> Result<Arc<Tree>, String> {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        let path = self
            .data
            .join("trees")
            .join(&repo.key)
            .join(format!("{sha}.{n}"));
        fs::create_dir_all(&path)
            .map_err(|e| format!("could not make a directory ({})", e.kind()))?;
        let tree = Arc::new(Tree {
            sha: sha.to_owned(),
            path,
        });
        let index = self
            .data
            .join("tmp")
            .join(format!("index.{}.{n}", repo.key));
        let environment = [(
            "GIT_INDEX_FILE".to_owned(),
            index.to_string_lossy().into_owned(),
        )];
        let work_tree = format!("--work-tree={}", tree.path.display());
        let write = |paths: &[&str]| -> Result<(), String> {
            let mut list = vec!["ls-files", "-z", "--"];
            let literal: Vec<String> = paths.iter().map(|p| format!(":(literal){p}")).collect();
            list.extend(literal.iter().map(String::as_str));
            let files = self.git(repo, &list, &environment, None)?;
            if files.is_empty() {
                return Ok(());
            }
            self.git(
                repo,
                &[&work_tree, "checkout-index", "-z", "--stdin"],
                &environment,
                Some(files.as_bytes()),
            )
            .map(|_| ())
        };
        let outcome = self
            .git(repo, &["read-tree", sha], &environment, None)
            .and_then(|_| write(&[".story-time", ".story-time.json", "model"]))
            .and_then(|()| {
                let backlog = backlog_dir(&tree.path);
                write(&[&backlog])
            });
        let _ = fs::remove_file(&index);
        outcome.map(|()| tree)
    }

    /// The state of each repository for `story_repos`. It does not fetch, and it does not wait
    /// for a fetch that runs.
    pub(super) fn states(&self) -> Vec<Value> {
        self.registry
            .repos
            .iter()
            .zip(&self.status)
            .map(|(repo, status)| {
                let status = status.lock().unwrap_or_else(|e| e.into_inner());
                json!({
                    "key": repo.key,
                    "prefixes": repo.prefixes,
                    "sha": status.sha,
                    "fetched_at": status.fetched_at,
                    "state": if status.state.is_empty() { "cloning" } else { status.state },
                    "last_error": status.last_error,
                })
            })
            .collect()
    }
}

/// The `backlogDir` of the settings in a commit directory, when it is a plain relative path.
fn backlog_dir(tree: &Path) -> String {
    [crate::config::FILE, crate::config::LEGACY_FILE]
        .iter()
        .find_map(|file| fs::read_to_string(tree.join(file)).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|settings| settings["backlogDir"].as_str().map(str::to_owned))
        .map(|dir| dir.trim_matches('/').to_owned())
        .filter(|dir| {
            !dir.is_empty()
                && dir.split('/').all(|step| {
                    !step.is_empty() && step != "." && step != ".." && !step.starts_with(':')
                })
        })
        .unwrap_or_else(|| "backlog".into())
}
