use std::{fs, path::PathBuf, process::Command};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn policy_contract_leaves_the_temp_dir_empty() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch =
        Scratch(std::env::temp_dir().join(format!("story-policy-temp-{}", std::process::id())));
    let _ = fs::remove_dir_all(&scratch.0);
    let build = scratch.0.join("build");
    let temp = scratch.0.join("tmp");
    fs::create_dir_all(&build).unwrap();
    fs::create_dir_all(&temp).unwrap();

    let contract = build.join("story-policy-contract");
    let compiled = Command::new("rustc")
        .args(["--edition=2021", "--test"])
        .arg(repo.join("test/story-policy-contract.rs"))
        .arg("-o")
        .arg(&contract)
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let output = Command::new(&contract)
        .current_dir(&repo)
        .env("TMPDIR", &temp)
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .env("NODE_DISABLE_COMPILE_CACHE", "1")
        .env_remove("STORY_TIME_CONFIG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "policy contract failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut leftovers: Vec<String> = fs::read_dir(&temp)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    leftovers.sort();
    assert!(
        leftovers.is_empty(),
        "the policy contract left {} entries in TMPDIR:\n{}",
        leftovers.len(),
        leftovers.join("\n")
    );
}
