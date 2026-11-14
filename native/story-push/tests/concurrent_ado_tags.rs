use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

// A stub `az` that keeps one work item (revision and tags) in $AZ_STUB_STATE and
// enforces the JSON Patch `test` on /rev the way Azure DevOps does. The first two
// `work-item show` calls wait for each other, so both pushes read the item before
// either writes its tags.
const STUB_AZ: &str = r#"
use std::{env, fs, path::{Path, PathBuf}, thread, time::{Duration, Instant}};

fn lock(state: &Path) -> PathBuf {
    let lock = state.join("lock");
    let started = Instant::now();
    while fs::create_dir(&lock).is_err() {
        if started.elapsed() > Duration::from_secs(30) { eprintln!("stub lock timeout"); std::process::exit(9); }
        thread::sleep(Duration::from_millis(5));
    }
    lock
}
fn load(state: &Path) -> (u64, String) {
    let text = fs::read_to_string(state.join("item")).unwrap();
    let (rev, tags) = text.split_once('\n').unwrap();
    (rev.parse().unwrap(), tags.to_string())
}
fn save(state: &Path, rev: u64, tags: &str) { fs::write(state.join("item"), format!("{rev}\n{tags}")).unwrap(); }
fn after<'a>(text: &'a str, marker: &str) -> &'a str { &text[text.find(marker).unwrap_or_else(|| panic!("missing {marker} in {text}")) + marker.len()..] }

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let state = PathBuf::from(env::var("AZ_STUB_STATE").unwrap());
    fs::OpenOptions::new().create(true).append(true).open(state.join("calls")).and_then(|mut f| { use std::io::Write; writeln!(f, "{:?}", args) }).unwrap();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["boards", "work-item", "update", ..] => {
            let held = lock(&state);
            let (rev, mut tags) = load(&state);
            if let Some(at) = words.iter().position(|w| *w == "--fields") {
                for field in &words[at + 1..] {
                    if field.starts_with("--") { break; }
                    if let Some(value) = field.strip_prefix("System.Tags=") { tags = value.to_string(); }
                }
            }
            save(&state, rev + 1, &tags);
            fs::remove_dir(held).unwrap();
            println!("{{\"id\":12,\"rev\":{}}}", rev + 1);
        }
        ["boards", "work-item", "show", ..] => {
            let held = lock(&state);
            let (rev, tags) = load(&state);
            fs::remove_dir(held).unwrap();
            let ticket = (1..=2).find(|n| fs::OpenOptions::new().write(true).create_new(true).open(state.join(format!("read-{n}"))).is_ok());
            if ticket.is_some() {
                let started = Instant::now();
                while !state.join("read-2").exists() {
                    if started.elapsed() > Duration::from_secs(30) { eprintln!("second push never read the work item"); std::process::exit(9); }
                    thread::sleep(Duration::from_millis(5));
                }
            }
            println!("{{\"id\":12,\"rev\":{rev},\"fields\":{{\"System.Tags\":\"{tags}\"}}}}");
        }
        ["devops", "invoke", ..] => {
            let file = words[words.iter().position(|w| *w == "--in-file").unwrap() + 1];
            let patch = fs::read_to_string(file).unwrap();
            let held = lock(&state);
            let (rev, _) = load(&state);
            let tested: u64 = after(&patch, "\"path\":\"/rev\",\"value\":").split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
            if tested != rev {
                fs::remove_dir(held).unwrap();
                fs::write(state.join(format!("conflict-{tested}-{rev}")), "").unwrap();
                eprintln!("ERROR: TF26071: This work item has been changed by someone else since you opened it.");
                std::process::exit(1);
            }
            let tags = after(&patch, "\"path\":\"/fields/System.Tags\",\"value\":\"").split('"').next().unwrap().to_string();
            save(&state, rev + 1, &tags);
            fs::remove_dir(held).unwrap();
            println!("{{\"id\":12,\"rev\":{}}}", rev + 1);
        }
        _ => { eprintln!("unexpected az call {:?}", args); std::process::exit(2); }
    }
}
"#;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new(kind: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-concurrent-ado-{kind}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn compile_stub(dir: &Path) {
    let mut rustc = Command::new("rustc")
        .args(["--edition=2021", "--crate-name", "az", "-o"])
        .arg(dir.join("az"))
        .arg("-")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    rustc
        .stdin
        .take()
        .unwrap()
        .write_all(STUB_AZ.as_bytes())
        .unwrap();
    let out = rustc.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn checkout(story_type: &str, epic: &str) -> Dir {
    let dir = Dir::new("checkout");
    fs::create_dir_all(dir.0.join(".story-time")).unwrap();
    fs::create_dir_all(dir.0.join("backlog")).unwrap();
    fs::write(
        dir.0.join(".story-time/settings.json"),
        json!({"tracker":"ado","sync":"auto","ado":{"organization":"https://dev.azure.com/acme","project":"Shop","process":"agile"}}).to_string(),
    )
    .unwrap();
    let story = json!({"id":"ST-001","title":"Native push","status":"ready","type":story_type,"epic":epic,"size":3,"remote":12,"created":"2026-11-14","sections":{"Story":"As a maintainer I need tags kept.","Acceptance Criteria":"- [ ] Given two pushes, When both finish, Then both tag changes remain."}});
    fs::write(
        dir.0.join("backlog/ST-001.md"),
        story_time_native::story::serialize(&story).unwrap(),
    )
    .unwrap();
    dir
}

fn push(checkout: &Dir, bin: &Dir, state: &Dir) -> Output {
    Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["push", "ST-001", "--json"])
        .current_dir(&checkout.0)
        .env_clear()
        .env("PATH", &bin.0)
        .env("AZ_STUB_STATE", &state.0)
        .env("AZURE_DEVOPS_EXT_PAT", "test-only")
        .output()
        .unwrap()
}

fn tags(state: &Dir) -> BTreeSet<String> {
    let text = fs::read_to_string(state.0.join("item")).unwrap();
    text.split_once('\n')
        .unwrap()
        .1
        .split(';')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

#[test]
fn two_pushes_that_change_different_ado_tags_at_once_both_keep_their_change() {
    let bin = Dir::new("bin");
    compile_stub(&bin.0);
    let state = Dir::new("state");
    fs::write(
        state.0.join("item"),
        "1\nstory-time; ST-001; epic:EP-1; team",
    )
    .unwrap();
    let re_epic = checkout("story", "EP-2");
    let retype = checkout("chore", "EP-1");

    let outputs = thread::scope(|s| {
        let a = s.spawn(|| push(&re_epic, &bin, &state));
        let b = s.spawn(|| push(&retype, &bin, &state));
        [a.join().unwrap(), b.join().unwrap()]
    });
    let calls = fs::read_to_string(state.0.join("calls")).unwrap_or_default();
    for output in &outputs {
        assert!(
            output.status.success(),
            "{}\ncalls:\n{calls}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["pushed"][0]["action"], "update");
    }
    assert_eq!(
        tags(&state),
        ["story-time", "ST-001", "epic:EP-2", "chore", "team"]
            .map(String::from)
            .into(),
        "the re-epic, the retype, and the team tag must all survive\ncalls:\n{calls}"
    );
    assert!(
        fs::read_dir(&state.0).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("conflict-")),
        "the second tag write must hit the revision check and retry\ncalls:\n{calls}"
    );
}
