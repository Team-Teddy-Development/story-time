use crate::{
    config::{self, Flags},
    lenient_story::Js,
    lint, list, rank, Failure,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
};

const HELP: &str = "Usage: story assignment next --owner RUN --pid PID\n       story assignment status|reconcile\n       story assignment heartbeat|release --token TOKEN\n\nLocal Linux ownership. All results are JSON. Claims reserve configured WIP\nwithout changing story status. Recovery must resume the recorded worktree.\nLedger: <git-common-dir>/story-assignment/ledger.json\n";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ProcessIdentity {
    machine_id: String,
    boot_id: String,
    pid: u32,
    starttime: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum Ownership {
    Active { token: String },
    Recoverable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Claim {
    story_id: String,
    owner: String,
    process: ProcessIdentity,
    heartbeat: String,
    worktree: PathBuf,
    branch: String,
    head: Option<String>,
    #[serde(flatten)]
    ownership: Ownership,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    version: u32,
    claims: Vec<Claim>,
}
impl Default for Ledger {
    fn default() -> Self {
        Self {
            version: 1,
            claims: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Liveness {
    Alive,
    Dead,
    Unknown,
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("Cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "Git {} failed; assignment requires a local Git worktree.",
            args.join(" ")
        ));
    }
    String::from_utf8(out.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|e| e.to_string())
}
fn current_branch(root: &Path) -> Result<String, String> {
    git(root, &["rev-parse", "--symbolic-full-name", "HEAD"])
        .or_else(|_| git(root, &["symbolic-ref", "HEAD"]))
}
fn read_id(path: &str) -> Result<String, String> {
    let id = fs::read_to_string(path)
        .map_err(|e| format!("Cannot read local identity {path}: {e}"))?
        .trim()
        .to_owned();
    if id.is_empty() {
        return Err(format!("Empty local identity {path}"));
    }
    Ok(id)
}
fn proc_stat(pid: u32) -> io::Result<(char, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let tail = stat
        .rsplit_once(") ")
        .ok_or_else(|| io::Error::other("Invalid process stat"))?
        .1;
    let fields: Vec<_> = tail.split_whitespace().collect();
    let state = fields
        .first()
        .and_then(|s| s.chars().next())
        .ok_or_else(|| io::Error::other("Missing process state"))?;
    let start = fields
        .get(19)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| io::Error::other("Missing process starttime"))?;
    Ok((state, start))
}
fn identity(pid: u32) -> Result<ProcessIdentity, String> {
    if pid == 0 {
        return Err("--pid must name a live local process.".into());
    }
    let boot_id = read_id("/proc/sys/kernel/random/boot_id")?;
    let machine_id = read_id("/etc/machine-id")?;
    let (state, starttime) =
        proc_stat(pid).map_err(|e| format!("Cannot verify --pid {pid}: {e}"))?;
    if matches!(state, 'Z' | 'X' | 'x') {
        return Err("--pid is terminated or a zombie.".into());
    }
    Ok(ProcessIdentity {
        machine_id,
        boot_id,
        pid,
        starttime,
    })
}
fn liveness(process: &ProcessIdentity) -> Liveness {
    let Ok(machine) = read_id("/etc/machine-id") else {
        return Liveness::Unknown;
    };
    if machine != process.machine_id {
        return Liveness::Unknown;
    }
    let Ok(boot) = read_id("/proc/sys/kernel/random/boot_id") else {
        return Liveness::Unknown;
    };
    if boot != process.boot_id {
        return Liveness::Dead;
    }
    match proc_stat(process.pid) {
        Ok((state, start)) if start != process.starttime || matches!(state, 'Z' | 'X' | 'x') => {
            Liveness::Dead
        }
        Ok(_) => Liveness::Alive,
        Err(e)
            if e.kind() == io::ErrorKind::NotFound
                && fs::read_to_string("/proc/self/stat").is_ok() =>
        {
            Liveness::Dead
        }
        Err(_) => Liveness::Unknown,
    }
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "Could not generate an assignment token.".to_owned())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn load(path: &Path) -> Result<Ledger, String> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Ledger::default()),
        Err(e) => return Err(format!("Cannot read assignment ledger: {e}")),
    };
    let ledger: Ledger = serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "Invalid assignment ledger; preserved {}: {e}",
            path.display()
        )
    })?;
    let mut ids = HashSet::new();
    let mut owners = HashSet::new();
    let mut tokens = HashSet::new();
    if ledger.version != 1
        || ledger.claims.iter().any(|c| {
            !crate::valid_id(&c.story_id)
                || c.owner.trim().is_empty()
                || !c.worktree.is_absolute()
                || c.process.pid == 0
                || c.process.starttime == 0
                || c.process.machine_id.is_empty()
                || c.process.boot_id.is_empty()
                || chrono::DateTime::parse_from_rfc3339(&c.heartbeat).is_err()
                || !ids.insert(&c.story_id)
                || match &c.ownership {
                    Ownership::Active { token } => {
                        token.len() != 64
                            || !token.bytes().all(|b| b.is_ascii_hexdigit())
                            || !tokens.insert(token)
                            || !owners.insert(&c.owner)
                    }
                    Ownership::Recoverable => false,
                }
        })
    {
        return Err(format!(
            "Invalid assignment ledger invariants; preserved {}.",
            path.display()
        ));
    }
    Ok(ledger)
}
fn persist(path: &Path, ledger: &Ledger) -> Result<(), String> {
    let tmp = path.with_extension(format!("json.{}.tmp", token()?));
    let result = (|| -> Result<(), String> {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(&serde_json::to_vec_pretty(ledger).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        File::open(path.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| format!("Cannot persist assignment ledger: {e}"))
}
fn reconcile(ledger: &mut Ledger) -> Vec<Value> {
    let mut changes = Vec::new();
    for c in &mut ledger.claims {
        if matches!(c.ownership, Ownership::Active { .. }) && liveness(&c.process) == Liveness::Dead
        {
            let before = c.clone();
            c.ownership = Ownership::Recoverable;
            changes.push(json!({"reason":"verified_dead", "before":before, "after":c}));
        }
    }
    changes
}
fn eligible<'a>(stories: &'a [Js], root: &Path, statuses: &[&str]) -> Result<Vec<&'a Js>, String> {
    let ranked = rank::ordered_candidates(stories, statuses)?;
    let ctx = lint::Context {
        stories: Some(stories),
        root: Some(root),
        ..Default::default()
    };
    let mut result = Vec::new();
    for story in ranked {
        if lint::ready(&lint::lint_story(story, &ctx)?) {
            result.push(story);
        }
    }
    Ok(result)
}
fn legacy_ids(root: &Path, ledger: &Ledger) -> Result<Vec<String>, String> {
    let worktrees = git(root, &["worktree", "list", "--porcelain", "-z"])?;
    let mut ids = std::collections::BTreeSet::new();
    let mut headers = std::collections::HashMap::new();
    for field in worktrees.split('\0') {
        let Some(path) = field.strip_prefix("worktree ") else {
            continue;
        };
        let worktree = Path::new(path);
        if !worktree.is_dir() {
            return Err(format!("Cannot inspect registered worktree {path}; ownership remains unknown. Restore or explicitly prune its registration before selection."));
        }
        let file = if worktree.join(config::FILE).exists() {
            worktree.join(config::FILE)
        } else {
            worktree.join(config::LEGACY_FILE)
        };
        let loaded = config::load(Some(file.to_str().ok_or("Worktree path is not UTF-8.")?))?;
        let settings = config::parse(&loaded.raw)?;
        let dir = list::backlog_dir(None, worktree, &settings)?;
        for story in list::headers(&dir, &mut headers)? {
            let id = story.get("id").string();
            if story.get("status").string() == "in-progress"
                && !ledger.claims.iter().any(|c| c.story_id == id)
            {
                ids.insert(id);
            }
        }
    }
    Ok(ids.into_iter().collect())
}
fn reply(
    reason: &str,
    before: &Ledger,
    after: &Ledger,
    claim: Option<&Claim>,
    legacy: &[String],
    changes: &[Value],
    preparation: Option<&Js>,
) -> String {
    let owners: Vec<_> = after.claims.iter().map(|c| json!({"story_id":c.story_id,"liveness":liveness(&c.process),"state":c.ownership,"worktree":c.worktree})).collect();
    let next_action = if let Some(story) = preparation {
        let id = story.get("id").string();
        format!("Recorded WIP reservations: {}. Legacy statuses hold individual stories, not global execution capacity. Prepare {id} under the coordinator's existing preparation ownership; inspect existing work and PRs before editing. Run story show {id} and story lint {id} --json. Resolve readiness and repository admission; request assignment again before implementation. No execution claim or sprint forecast is granted. Unknown sprint velocity does not block preparation.", after.claims.len())
    } else {
        match reason {
            "recovery_required" => "Run assignment next in claim.worktree on claim.branch; preserve and inspect existing work before resuming.",
            "recovery_ineligible" => "Inspect recorded work and restore readiness and completed dependencies before claiming; do not reset story status.",
            "capacity_full" => "Wait for a recorded owner to release or reconcile a verified terminated owner.",
            "owner_conflict" => "Use a distinct run owner; the recorded owner has another live or uncertain process identity.",
            "no_eligible_story" => "Wait for completed dependencies or missing WSJF/readiness inputs. Inspect story rank --include-refined --json and story lint --json; do not claim or reset blocked work.",
            "stale_token" => "Discard this token. Read assignment status to locate the current recorded owner.",
            "owner_not_alive" => "Run assignment reconcile; do not send a heartbeat on behalf of a terminated or unverified process.",
            "status" | "reconciled" => "Inspect owners and legacy_in_progress; run assignment next with a verified live local owner PID.",
            "released" => "The token is invalid. Run assignment next for another story when capacity is free.",
            _ => "Use the current token for heartbeat or release; story acceptance remains independent."
        }.to_owned()
    };
    format!(
        "{}\n",
        json!({"reason":reason,"before":before,"after":after,"claim":claim,"legacy_in_progress":legacy,"owners":owners,"changes":changes,"reserved_wip":after.claims.len(),"preparation":preparation.map(|s| json!({"story_id":s.get("id").string(),"status":s.get("status").string()})),"legacy_next_action":if legacy.is_empty() { None } else { Some("These IDs lack verified execution ownership. Quarantine them individually; do not take them over or reset their statuses. Bare statuses do not reserve execution WIP.") },"next_action":next_action})
    )
}

pub fn run(args: &[String]) -> Result<(String, u8), Failure> {
    if args.is_empty() || args.iter().any(|a| matches!(a.as_str(), "--help" | "-h")) {
        return Ok((HELP.into(), 0));
    }
    let op = args[0].as_str();
    if !["next", "status", "reconcile", "heartbeat", "release"].contains(&op) {
        return Err(Failure {
            code: 2,
            message: HELP.into(),
        });
    }
    let flags = Flags::parse(&args[1..])?;
    if !flags.positional.is_empty() {
        return Err("Unexpected assignment arguments.".into());
    }
    for name in flags.names() {
        let allowed = matches!(name.as_str(), "config" | "json")
            || (op == "next" && matches!(name.as_str(), "owner" | "pid"))
            || (matches!(op, "heartbeat" | "release") && name == "token");
        if !allowed {
            return Err(format!("Unsupported flag --{name} for assignment {op}.").into());
        }
    }
    let required = |name| {
        flags
            .value(name)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| format!("assignment {op} requires --{name}."))
    };
    let new_owner = if op == "next" {
        Some((
            required("owner")?.to_owned(),
            identity(
                required("pid")?
                    .parse()
                    .map_err(|_| "--pid must be a positive integer.")?,
            )?,
        ))
    } else {
        None
    };
    let provided_token = if matches!(op, "heartbeat" | "release") {
        Some(required("token")?)
    } else {
        None
    };
    let loaded = config::load(flags.value("config"))?;
    let settings = config::parse(&loaded.raw)?;
    let root = PathBuf::from(git(&loaded.root, &["rev-parse", "--show-toplevel"])?)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if loaded.root.canonicalize().map_err(|e| e.to_string())? != root {
        return Err("Assignment configuration must belong to the Git worktree root.".into());
    }
    let common = PathBuf::from(git(
        &root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    let state_dir = common.join("story-assignment");
    fs::create_dir_all(&state_dir).map_err(|e| format!("Cannot create assignment state: {e}"))?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(state_dir.join("lock"))
        .map_err(|e| e.to_string())?;
    lock.lock()
        .map_err(|e| format!("Cannot lock assignment state: {e}"))?;
    let path = state_dir.join("ledger.json");
    let mut ledger = load(&path)?;
    let before = ledger.clone();
    let dir = list::backlog_dir(None, &root, &settings)?;
    let stories = if op == "next" {
        list::stories(&dir, None)?
    } else {
        Vec::new()
    };
    let changes = if matches!(op, "next" | "reconcile") {
        reconcile(&mut ledger)
    } else {
        Vec::new()
    };
    let legacy = legacy_ids(&root, &ledger)?;
    let mut selected = None;
    let mut preparation = None;
    let reason;
    if let Some(token) = provided_token {
        let Some(i) = ledger
            .claims
            .iter()
            .position(|c| matches!(&c.ownership, Ownership::Active {token:t} if t == token))
        else {
            return Ok((
                reply(
                    "stale_token",
                    &before,
                    &ledger,
                    None,
                    &legacy,
                    &changes,
                    None,
                ),
                1,
            ));
        };
        if op == "heartbeat" {
            if liveness(&ledger.claims[i].process) != Liveness::Alive {
                return Ok((
                    reply(
                        "owner_not_alive",
                        &before,
                        &ledger,
                        Some(&ledger.claims[i]),
                        &legacy,
                        &changes,
                        None,
                    ),
                    1,
                ));
            }
            ledger.claims[i].heartbeat = now();
            selected = Some(ledger.claims[i].clone());
            reason = "heartbeat";
        } else {
            selected = Some(ledger.claims.remove(i));
            reason = "released";
        }
    } else if let Some((owner, process)) = new_owner {
        if let Some(c) = ledger
            .claims
            .iter()
            .find(|c| c.owner == owner && matches!(c.ownership, Ownership::Active { .. }))
        {
            reason = if c.process != process {
                "owner_conflict"
            } else if c.worktree != root || c.branch != current_branch(&root)? {
                "recovery_required"
            } else if !eligible(&stories, &root, &["ready", "in-progress"])?
                .iter()
                .any(|s| s.get("id").string() == c.story_id)
            {
                "recovery_ineligible"
            } else {
                "resumed"
            };
            selected = Some(c.clone());
        } else if let Some(i) = ledger
            .claims
            .iter()
            .position(|c| matches!(c.ownership, Ownership::Recoverable))
        {
            if ledger.claims[i].worktree != root
                || ledger.claims[i].branch != current_branch(&root)?
            {
                reason = "recovery_required";
                selected = Some(ledger.claims[i].clone());
            } else if !eligible(&stories, &root, &["ready", "in-progress"])?
                .iter()
                .any(|s| s.get("id").string() == ledger.claims[i].story_id)
            {
                reason = "recovery_ineligible";
                selected = Some(ledger.claims[i].clone());
            } else {
                let c = &mut ledger.claims[i];
                c.owner = owner;
                c.process = process;
                c.heartbeat = now();
                c.ownership = Ownership::Active { token: token()? };
                selected = Some(c.clone());
                reason = "recovered";
            }
        } else if ledger.claims.len() >= settings["team"]["wipLimit"].as_u64().unwrap_or(1) as usize
        {
            reason = "capacity_full";
        } else {
            let available = |story: &&Js| {
                let id = story.get("id").string();
                !legacy.contains(&id) && !ledger.claims.iter().any(|c| c.story_id == id)
            };
            let next = eligible(&stories, &root, &["ready"])?
                .into_iter()
                .find(available);
            if let Some(story) = next {
                let claim = Claim {
                    story_id: story.get("id").string(),
                    owner,
                    process,
                    heartbeat: now(),
                    worktree: root.clone(),
                    branch: current_branch(&root)?,
                    head: git(&root, &["rev-parse", "--verify", "HEAD"]).ok(),
                    ownership: Ownership::Active { token: token()? },
                };
                selected = Some(claim.clone());
                ledger.claims.push(claim);
                reason = "claimed";
            } else {
                preparation = rank::ordered_candidates(&stories, &["ready", "refined", "draft"])?
                    .into_iter()
                    .find(available);
                reason = if preparation.is_some() {
                    "preparation_required"
                } else {
                    "no_eligible_story"
                };
            }
        }
    } else {
        reason = if op == "reconcile" {
            "reconciled"
        } else {
            "status"
        };
    }
    if op != "status" {
        persist(&path, &ledger)?;
    }
    Ok((
        reply(
            reason,
            &before,
            &ledger,
            selected.as_ref(),
            &legacy,
            &changes,
            preparation,
        ),
        0,
    ))
}
