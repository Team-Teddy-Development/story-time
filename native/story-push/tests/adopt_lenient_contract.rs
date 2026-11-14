use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const STORIES: [(&str, &str); 4] = [
    (
        "ST-001",
        "---\nid: ST-001\ntitle: Fix: login fails\nstatus: ready\n---\n\n## Story\nx\n",
    ),
    (
        "ST-002",
        "---\nid: ST-002\ntitle: Old\nstatus: draft\ntitle: New\nstatus: ready\n---\n\n## Story\nx\n",
    ),
    (
        "ST-003",
        "---\nid: ST-003\nstatus: done\n---\nProse before the first section.\n\n## Story\nx\n",
    ),
    (
        "ST-004",
        "---\r\nid: ST-004\r\ntitle: Fix: all three\r\nstatus: draft\r\nstatus: in-progress\r\nremote: 9\r\n---\r\nProse.\r\n\r\n## Story\r\nx\r\n## Story\r\ny\r\n",
    ),
];

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-adopt-lenient-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        fs::create_dir_all(path.join(".story-time")).unwrap();
        fs::write(
            path.join(".story-time/settings.json"),
            r#"{"tracker":"files","sync":"off","prefix":"ST","backlogDir":"backlog"}"#,
        )
        .unwrap();
        for (id, text) in STORIES {
            fs::write(path.join(format!("backlog/{id}.md")), text).unwrap();
        }
        Self(path)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn node_identity(file: &Path) -> Value {
    let module = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills/story-time/scripts/src/domain/story.mjs")
        .canonicalize()
        .unwrap();
    let script = format!(
        "import {{ parseStory }} from {module:?}; import {{ readFileSync }} from 'node:fs'; \
         const s = parseStory(readFileSync(process.argv[1], 'utf8')); \
         console.log(JSON.stringify({{ id: s.id ?? null, status: s.status ?? null, remote: s.remote ?? null }}));"
    );
    let out = Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .arg(file)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "Node parseStory refused {}: {}",
        file.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn adopt_preview_reads_every_story_node_parse_story_reads() {
    let p = Project::new();
    let out = Command::new(env!("CARGO_BIN_EXE_story"))
        .args(["adopt", "preview"])
        .current_dir(&p.0)
        .env_clear()
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !report.to_string().contains("unsupported_story"),
        "adoption refused a story Node reads: {report}"
    );
    assert_eq!(out.status.code(), Some(0), "{report}");
    let rows = report["backlog"]["files"].as_array().unwrap();
    assert_eq!(rows.len(), STORIES.len());
    for (id, _) in STORIES {
        let file = p.0.join(format!("backlog/{id}.md"));
        let row = rows
            .iter()
            .find(|r| r["path"] == format!("backlog/{id}.md"))
            .unwrap();
        let native = json!({"id": row["id"], "status": row["status"], "remote": row["remote"]});
        assert_eq!(native, node_identity(&file), "{id}");
    }
}
