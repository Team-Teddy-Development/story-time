use std::{fs, path::PathBuf, process::Command};

fn drive_process_exec(exit: u8, name: &str) -> (String, String) {
    let dir = std::env::temp_dir().join(format!("story-exec-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let child = dir.join("tracker.cjs");
    fs::write(
        &child,
        format!(
            "require('fs').closeSync(0);\n\
             setTimeout(() => {{ console.log('tracker output'); console.error('tracker failed'); process.exit({exit}); }}, 300);\n"
        ),
    )
    .unwrap();
    let adapter = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills/story-time/scripts/src/adapters/process-exec.mjs")
        .canonicalize()
        .unwrap();
    let driver = format!(
        "import {{ processExec }} from {adapter:?};\n\
         try {{\n\
           const out = await processExec(process.execPath, [{child:?}], {{ input: 'x'.repeat(1 << 20) }});\n\
           console.log(JSON.stringify({{ resolved: out.trim() }}));\n\
         }} catch (error) {{\n\
           console.log(JSON.stringify({{ rejected: error.message }}));\n\
         }}\n",
        adapter = format!("file://{}", adapter.display()),
        child = child.display().to_string(),
    );
    let output = Command::new("node")
        .args(["--input-type=module", "-e", &driver])
        .output()
        .unwrap();
    fs::remove_dir_all(&dir).unwrap();
    (
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_tracker_that_closes_stdin_and_succeeds_resolves_with_its_output() {
    let (stdout, stderr) = drive_process_exec(0, "ok");
    assert!(!stderr.contains("EPIPE"), "unhandled EPIPE: {stderr}");
    assert_eq!(stdout, r#"{"resolved":"tracker output"}"#);
}

#[test]
fn a_tracker_that_closes_stdin_and_fails_rejects_with_its_error() {
    let (stdout, stderr) = drive_process_exec(2, "fail");
    assert!(!stderr.contains("EPIPE"), "unhandled EPIPE: {stderr}");
    assert_eq!(stdout, r#"{"rejected":"tracker failed"}"#);
}
