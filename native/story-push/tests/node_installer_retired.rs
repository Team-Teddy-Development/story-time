use std::{fs, path::PathBuf, process::Command};

const HISTORICAL: [&str; 5] = [
    "backlog/",
    "docs/verification/",
    "docs/planning/verification.md",
    "docs/designs/gap-sweep-review/",
    "native/story-push/tests/install_skills_contract.rs",
];

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn node_installer_and_its_test_are_gone() {
    for path in ["scripts/install.mjs", "test/install.test.mjs"] {
        assert!(!repo().join(path).exists(), "{path} still exists");
    }
}

#[test]
fn no_live_file_tells_anyone_to_run_the_node_installer() {
    let needle = ["node", "scripts/install.mjs"].join(" ");
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(repo())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let offenders: Vec<&str> = std::str::from_utf8(&out.stdout)
        .unwrap()
        .split('\0')
        .filter(|path| !path.is_empty() && !HISTORICAL.iter().any(|h| path.starts_with(h)))
        .filter(|path| {
            fs::read(repo().join(path))
                .map(|bytes| String::from_utf8_lossy(&bytes).contains(&needle))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "files still name {needle}: {offenders:?}"
    );
}
