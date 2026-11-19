//! The repositories of a hosted server (ADR 0007). Each registered repository is a bare clone
//! under the data directory. A read fetches the clone when its last fetch is older than the
//! registry's TTL, then reads the story files of one commit from a directory that holds only
//! `.story-time`, the backlog, and `model`. One lock for each repository serializes its git
//! commands. A repository that the server cannot reach does not stop the other repositories.

use super::{
    forge::{self, GitHub},
    forge_auth::{self, App, Gitea},
    registry::{branch_ok, Forge, Registry, RegistryRepo},
    retry,
};
use crate::Failure;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, RwLock,
    },
    time::{Duration, Instant},
};

pub(super) const USAGE: &str = "[--registry FILE [--data DIR] [--github-app-id-file FILE --github-app-key-file FILE] [--github-api-url URL] [--gitea-token-file FILE]]";
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
    /// The bot token that reads a Gitea repository. Without it a Gitea repository is read
    /// without a credential.
    gitea: Option<Gitea>,
    forge: Option<GitHub>,
    secrets: Secrets,
    slots: Vec<Mutex<Slot>>,
    /// One lock for each repository, held for the whole of a write: fetch, commit, push, and
    /// pull request. It is apart from the slot, so that reads go on during a write.
    writes: Vec<Mutex<()>>,
    /// The open pull request of each story branch, as the forge last reported it.
    proposals: Mutex<HashMap<(usize, String), Proposal>>,
    /// The forge token that each repository last used, which leaves `Secrets` when replaced.
    forge_tokens: Vec<Mutex<Option<String>>>,
    /// The login and the numeric user id of the App's bot. They are read once.
    bot: Mutex<Option<(String, u64)>>,
    /// What `story_repos` reports. It is apart from the slot, so that the report does not wait
    /// for a fetch that holds the slot.
    status: Vec<Mutex<Status>>,
    next: AtomicU64,
}

struct Proposal {
    head: String,
    at: Instant,
    url: Option<String>,
}

/// What one `git` child returned.
struct Ran {
    ok: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// A write of one repository in progress. It holds the repository's write lock.
pub(super) struct Writing<'a> {
    workspace: &'a Workspace,
    index: usize,
    pub(super) default_branch: String,
    _lock: MutexGuard<'a, ()>,
}

pub(super) enum Pushed {
    Done,
    /// The branch at the remote was not where the server last saw it.
    Rejected,
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
            | "--github-api-url"
            | "--gitea-token-file" => {
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
    let forge = match &app {
        Some(_) => Some(GitHub::new(api).map_err(|e| refuse(format!("{e}.")))?),
        None => None,
    };
    let gitea = match flags.get("gitea-token-file") {
        None => None,
        Some(file) => Some(
            Gitea::new(&read("gitea-token-file", file)?)
                .map_err(|e| refuse(format!("--gitea-token-file {file}: {e}.")))?,
        ),
    };
    let data = flags.get("data").map_or(DEFAULT_DATA, |s| s);
    let workspace = Workspace::open(
        registry,
        Path::new(data),
        (app, gitea),
        forge,
        secrets.clone(),
    )
    .map_err(|e| refuse(format!("--data {data}: {e}.")))?;
    Ok((rest, Some(workspace)))
}

/// The git command in `args`, for an error text.
fn operation<'a>(args: &[&'a str]) -> &'a str {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .copied()
        .unwrap_or("git")
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl Workspace {
    /// The workspace under `data`. Commit directories that an earlier process left are removed.
    fn open(
        registry: Registry,
        data: &Path,
        (app, gitea): (Option<App>, Option<Gitea>),
        forge: Option<GitHub>,
        secrets: Secrets,
    ) -> Result<Self, String> {
        let made = |path: PathBuf| {
            fs::create_dir_all(&path)
                .map(|()| path)
                .map_err(|e| format!("could not create the data directory ({})", e.kind()))
        };
        if let Some(gitea) = &gitea {
            secrets
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .extend(gitea.secrets());
        }
        made(data.to_owned())?;
        let data = data
            .canonicalize()
            .map_err(|e| format!("could not resolve the data directory ({})", e.kind()))?;
        // What an earlier process left when it stopped inside a read or a write.
        for name in ["trees", "tmp"] {
            let _ = fs::remove_dir_all(data.join(name));
        }
        for name in ["repos", "trees", "home", "tmp"] {
            made(data.join(name))?;
        }
        let workspace = Self {
            slots: registry.repos.iter().map(|_| Mutex::default()).collect(),
            status: registry.repos.iter().map(|_| Mutex::default()).collect(),
            writes: registry.repos.iter().map(|_| Mutex::default()).collect(),
            forge_tokens: registry.repos.iter().map(|_| Mutex::default()).collect(),
            proposals: Mutex::default(),
            bot: Mutex::default(),
            registry,
            data,
            app,
            gitea,
            forge,
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
        // A name that did not resolve reached no server, so the command can run again.
        let ran = retry::again(
            retry::GIT_PAUSE,
            |failed: &Ran| retry::name_did_not_resolve(&String::from_utf8_lossy(&failed.stderr)),
            || match self.run(repo, args, environment, input, timeout) {
                Ok(ran) if ran.ok => Ok(Ok(ran)),
                Ok(ran) => Err(ran),
                Err(not_run) => Ok(Err(not_run)),
            },
        );
        match ran {
            Ok(Ok(ran)) => Ok(String::from_utf8_lossy(&ran.stdout).into_owned()),
            Ok(Err(not_run)) => Err(not_run),
            Err(ran) => Err(self.failed(args, &ran)),
        }
    }

    fn failed(&self, args: &[&str], ran: &Ran) -> String {
        format!(
            "git {} failed: {}",
            operation(args),
            self.plain(&String::from_utf8_lossy(&ran.stderr))
        )
    }

    /// Runs one `git` child to its end. `Err` is a child that did not start or did not end.
    fn run(
        &self,
        repo: &RegistryRepo,
        args: &[&str],
        environment: &[(String, String)],
        input: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<Ran, String> {
        let operation = operation(args);
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
            Some(status) => Ok(Ran {
                ok: status.success(),
                stdout,
                stderr,
            }),
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
        let access = match (repo.forge, &self.app, &self.gitea) {
            (Forge::GitHub, Some(app), _) => app.git_environment(&repo.clone)?,
            (Forge::Gitea, _, Some(gitea)) => {
                gitea.git_environment(&repo.clone, repo.allow_http)?
            }
            _ => Default::default(),
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

    /// Fetches every repository at start, so that the first read does not wait for a clone,
    /// and again each TTL, so that a failed fetch does not wait for a read to be tried again.
    pub(super) fn keep_fetched(&self) -> ! {
        let ttl = Duration::from_secs(self.registry.fetch_ttl_seconds);
        loop {
            for index in 0..self.slots.len() {
                let mut slot = self.slots[index].lock().unwrap_or_else(|e| e.into_inner());
                self.refresh(index, &mut slot);
            }
            std::thread::sleep(ttl);
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

    /// Whether this server can change stories: it has a GitHub App to push and to open pull
    /// requests with.
    pub(super) fn writes(&self) -> bool {
        self.app.is_some() && self.forge.is_some()
    }

    fn index(&self, key: &str) -> Option<usize> {
        self.registry.repos.iter().position(|repo| repo.key == key)
    }

    pub(super) fn repository(&self, key: &str) -> Option<&RegistryRepo> {
        self.index(key).map(|index| &self.registry.repos[index])
    }

    pub(super) fn comments_enabled(&self) -> bool {
        self.writes()
            || (self.gitea.is_some()
                && self
                    .registry
                    .repos
                    .iter()
                    .any(|repo| repo.forge == Forge::Gitea))
    }

    pub(super) fn gitea(&self) -> Option<&Gitea> {
        self.gitea.as_ref()
    }

    pub(super) fn lock_write(&self, key: &str) -> Result<MutexGuard<'_, ()>, String> {
        let index = self.index(key).ok_or("the repository is not registered")?;
        Ok(self.writes[index].lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Starts a write of `key`: takes its write lock and fetches it, whatever the TTL. A write
    /// needs the current heads, so a failed fetch refuses the write.
    pub(super) fn write(&self, key: &str) -> Result<Writing<'_>, Refusal> {
        let index = self.index(key).ok_or(Refusal::UnknownRepository)?;
        let lock = self.writes[index].lock().unwrap_or_else(|e| e.into_inner());
        let writing = Writing {
            workspace: self,
            index,
            default_branch: String::new(),
            _lock: lock,
        };
        let default_branch = writing.fetch()?;
        Ok(Writing {
            default_branch,
            ..writing
        })
    }

    /// The forge token of a repository. It joins `Secrets`, and the token it replaces leaves.
    fn forge_access(&self, index: usize) -> Result<forge::Access, String> {
        let app = self.app.as_ref().ok_or("the server has no GitHub App")?;
        let (owner, name) = forge_auth::repository(&self.registry.repos[index].clone)?;
        let token = app.token(&owner, &name)?;
        let mut last = self.forge_tokens[index]
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if last.as_deref() != Some(&token) {
            let mut secrets = self.secrets.write().unwrap_or_else(|e| e.into_inner());
            secrets.retain(|secret| Some(secret) != last.as_ref());
            secrets.push(token.clone());
            *last = Some(token.clone());
        }
        Ok(forge::Access { owner, name, token })
    }

    /// Calls the forge for the repository `key`, with its token.
    pub(super) fn forge<T>(
        &self,
        key: &str,
        call: impl FnOnce(&GitHub, &forge::Access) -> Result<T, String>,
    ) -> Result<T, String> {
        let index = self.index(key).ok_or("the repository is not registered")?;
        let forge = self.forge.as_ref().ok_or("the server has no GitHub App")?;
        call(forge, &self.forge_access(index)?)
    }

    /// The login and the numeric user id of the App's bot, asked of the forge once.
    pub(super) fn bot(&self, key: &str) -> Result<(String, u64), String> {
        let mut bot = self.bot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(known) = bot.as_ref() {
            return Ok(known.clone());
        }
        let app = self.app.as_ref().ok_or("the server has no GitHub App")?;
        let login = app.bot();
        let id = self.forge(key, |forge, access| forge.user_id(access, &login))?;
        *bot = Some((login.clone(), id));
        Ok((login, id))
    }

    /// The open pull request of `branch` in `key`: the head of the branch and the URL. `None`
    /// when the branch does not exist or has no open pull request. The forge is asked again
    /// when the head moved or the answer is older than the TTL; when the forge does not
    /// answer, the last answer for this head stands.
    pub(super) fn proposal(&self, key: &str, branch: &str) -> Option<(String, String)> {
        let index = self.index(key)?;
        // The GitHub App has no installation on a Gitea repository.
        if self.registry.repos[index].forge == Forge::Gitea {
            return None;
        }
        let head = self.head(&self.registry.repos[index], branch)?;
        let ttl = Duration::from_secs(self.registry.fetch_ttl_seconds);
        let at = (index, branch.to_owned());
        let mut proposals = self.proposals.lock().unwrap_or_else(|e| e.into_inner());
        let known = proposals.get(&at).filter(|known| known.head == head);
        let (had, current) = (
            known.is_some(),
            known.is_some_and(|known| known.at.elapsed() < ttl),
        );
        if !current {
            let asked = self.forge_access(index).and_then(|access| {
                let forge = self.forge.as_ref().ok_or("no forge")?;
                forge.open_pull(&access, branch)
            });
            match asked {
                Ok(url) => {
                    let found = Proposal {
                        head: head.clone(),
                        at: Instant::now(),
                        url,
                    };
                    proposals.insert(at.clone(), found);
                }
                Err(_) if had => {}
                Err(_) => return None,
            }
        }
        let url = proposals.get(&at)?.url.clone()?;
        Some((head, url))
    }

    /// The bytes of the file `path` in commit `sha` of `key`.
    pub(super) fn blob(&self, key: &str, sha: &str, path: &str) -> Option<Vec<u8>> {
        let repo = &self.registry.repos[self.index(key)?];
        let object = format!("{sha}:{path}");
        let ran = self
            .run(
                repo,
                &["cat-file", "blob", &object],
                &[],
                None,
                LOCAL_TIMEOUT,
            )
            .ok()?;
        ran.ok.then_some(ran.stdout)
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
                    "org": repo.org,
                    "prefixes": repo.prefixes,
                    "forge": repo.forge.name(),
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

impl Writing<'_> {
    fn repo(&self) -> &RegistryRepo {
        &self.workspace.registry.repos[self.index]
    }

    /// Fetches the repository now, and returns its default branch.
    pub(super) fn fetch(&self) -> Result<String, Refusal> {
        let workspace = self.workspace;
        let mut slot = workspace.slots[self.index]
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        slot.tried = None;
        workspace.refresh(self.index, &mut slot);
        match (&slot.last_error, &slot.default_branch) {
            (None, Some(branch)) if slot.cloned => Ok(branch.clone()),
            (reason, _) => {
                Err(Refusal::Unavailable(reason.clone().unwrap_or_else(|| {
                    "the repository is not cloned yet".into()
                })))
            }
        }
    }

    pub(super) fn head(&self, branch: &str) -> Option<String> {
        self.workspace.head(self.repo(), branch)
    }

    /// The story files of commit `sha` in a directory of this write alone.
    pub(super) fn scratch(&self, sha: &str) -> Result<Arc<Tree>, String> {
        self.workspace.materialize(self.repo(), sha)
    }

    /// The id of each story file in `backlog` on every branch. A new story takes an id above
    /// them all, so that two open pull requests do not carry one id.
    pub(super) fn ids(&self, backlog: &str) -> Result<Vec<String>, String> {
        let git = |args: &[&str]| self.workspace.git(self.repo(), args, &[], None);
        let refs = git(&["for-each-ref", "--format=%(refname)", "refs/heads"])?;
        let folder = format!("{backlog}/");
        let mut ids = Vec::new();
        for reference in refs.lines().filter(|line| !line.is_empty()) {
            let names = git(&["ls-tree", "--name-only", reference, "--", &folder])?;
            for name in names.lines() {
                let file = name.rsplit('/').next().unwrap_or(name);
                if let Some(id) = crate::story_file_id(file) {
                    ids.push(id.to_owned());
                }
            }
        }
        Ok(ids)
    }

    /// Makes the commit that puts `bytes` at `path` on top of `base`, and returns its id. The
    /// commit is in the clone only; `push` sends it.
    pub(super) fn commit(
        &self,
        base: &str,
        path: &str,
        bytes: &[u8],
        message: &str,
        author: &str,
    ) -> Result<String, String> {
        let workspace = self.workspace;
        let n = workspace.next.fetch_add(1, Ordering::Relaxed);
        let index = workspace
            .data
            .join("tmp")
            .join(format!("index.{}.{n}", self.repo().key));
        let email = format!("{author}@users.noreply.github.com");
        let mut environment = vec![(
            "GIT_INDEX_FILE".to_owned(),
            index.to_string_lossy().into_owned(),
        )];
        for role in ["AUTHOR", "COMMITTER"] {
            environment.push((format!("GIT_{role}_NAME"), author.to_owned()));
            environment.push((format!("GIT_{role}_EMAIL"), email.clone()));
        }
        let git = |args: &[&str], input: Option<&[u8]>| {
            workspace
                .git(self.repo(), args, &environment, input)
                .map(|out| out.trim().to_owned())
        };
        let made = (|| {
            git(&["read-tree", base], None)?;
            let blob = git(&["hash-object", "-w", "--stdin"], Some(bytes))?;
            let entry = format!("100644,{blob},{path}");
            git(&["update-index", "--add", "--cacheinfo", &entry], None)?;
            let tree = git(&["write-tree"], None)?;
            git(
                &["commit-tree", &tree, "-p", base],
                Some(message.as_bytes()),
            )
        })();
        let _ = fs::remove_file(&index);
        made
    }

    fn push_with_lease(
        &self,
        branch: &str,
        source: &str,
        expected: &str,
    ) -> Result<Pushed, String> {
        let workspace = self.workspace;
        let access = match &workspace.app {
            Some(app) => app.git_environment(&self.repo().clone)?,
            None => Default::default(),
        };
        let reference = format!("refs/heads/{branch}");
        let lease = format!("--force-with-lease={reference}:{expected}");
        let refspec = format!("{source}:{reference}");
        let args = ["push", "--porcelain", &lease, "origin", &refspec];
        let ran = workspace.run(
            self.repo(),
            &args,
            &access.environment,
            None,
            NETWORK_TIMEOUT,
        )?;
        let stale = String::from_utf8_lossy(&ran.stdout)
            .lines()
            .any(|line| line.starts_with('!') && line.contains("stale info"));
        match (ran.ok, stale) {
            (true, _) => Ok(Pushed::Done),
            (false, true) => Ok(Pushed::Rejected),
            (false, false) => Err(workspace.failed(&args, &ran)),
        }
    }

    /// Pushes `commit` to `branch` at the remote, when the branch is still at `expected` there:
    /// `None` is a branch that does not exist. Then the clone has the branch at `commit`.
    pub(super) fn push(
        &self,
        branch: &str,
        commit: &str,
        expected: Option<&str>,
    ) -> Result<Pushed, String> {
        let pushed = self.push_with_lease(branch, commit, expected.unwrap_or(""))?;
        if matches!(pushed, Pushed::Done) {
            let reference = format!("refs/heads/{branch}");
            self.workspace
                .git(self.repo(), &["update-ref", &reference, commit], &[], None)?;
        }
        Ok(pushed)
    }

    /// Removes `branch` at the remote and in the clone, when it is still at `expected`.
    pub(super) fn delete(&self, branch: &str, expected: &str) -> Result<(), String> {
        self.push_with_lease(branch, "", expected)?;
        let reference = format!("refs/heads/{branch}");
        self.workspace
            .git(self.repo(), &["update-ref", "-d", &reference], &[], None)
            .map(|_| ())
    }

    /// The name of the bot that is the author of this server's commits.
    pub(super) fn author(&self) -> String {
        match &self.workspace.app {
            Some(app) => app.bot(),
            None => "story-time[bot]".into(),
        }
    }

    /// The URL of the open pull request of `branch`, asked of the forge now.
    pub(super) fn open_pull(&self, branch: &str) -> Result<Option<String>, String> {
        let access = self.workspace.forge_access(self.index)?;
        let forge = self.workspace.forge.as_ref().ok_or("no forge")?;
        forge.open_pull(&access, branch)
    }

    /// Opens the pull request of `branch` to the default branch, and returns its URL.
    pub(super) fn create_pull(
        &self,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<String, String> {
        let access = self.workspace.forge_access(self.index)?;
        let forge = self.workspace.forge.as_ref().ok_or("no forge")?;
        forge.create_pull(&access, branch, &self.default_branch, title, body)
    }

    /// Records the pull request of `branch` at `head`, so that the next read names it.
    pub(super) fn remember(&self, branch: &str, head: &str, url: &str) {
        let found = Proposal {
            head: head.to_owned(),
            at: Instant::now(),
            url: Some(url.to_owned()),
        };
        self.workspace
            .proposals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((self.index, branch.to_owned()), found);
    }
}
