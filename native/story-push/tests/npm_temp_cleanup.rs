use std::{fs, path::PathBuf, process::Command};

struct EmptyTemp(PathBuf);
impl Drop for EmptyTemp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn npm_test_leaves_the_temp_dir_empty() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp =
        EmptyTemp(std::env::temp_dir().join(format!("story-npm-temp-{}", std::process::id())));
    let _ = fs::remove_dir_all(&temp.0);
    fs::create_dir_all(&temp.0).unwrap();

    let output = Command::new("npm")
        .arg("test")
        .current_dir(&repo)
        .env("TMPDIR", &temp.0)
        .env("TEMP", &temp.0)
        .env("TMP", &temp.0)
        // npm 11 keeps a fixed-name, reused compile cache in the temp dir; it is not a per-run leak.
        .env("NODE_DISABLE_COMPILE_CACHE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "npm test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut leftovers: Vec<String> = fs::read_dir(&temp.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    leftovers.sort();
    assert!(
        leftovers.is_empty(),
        "npm test left {} entries in TMPDIR:\n{}",
        leftovers.len(),
        leftovers.join("\n")
    );
}
