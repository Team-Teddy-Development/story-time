use std::{fs, path::PathBuf, process::Command};

fn run_helper_against(child_source: &str, name: &str) -> (Option<i32>, String, String) {
    let dir = std::env::temp_dir().join(format!("story-run-cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let child = dir.join("child.cjs");
    fs::write(&child, child_source).unwrap();
    let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills/story-time/scripts/test/run-cli.mjs")
        .canonicalize()
        .unwrap();
    let driver = format!(
        "import {{ runCli }} from {helper:?};\n\
         const r = await runCli({child:?}, [], {{ input: 'x'.repeat(1 << 20) }});\n\
         console.log(JSON.stringify({{ code: r.code }}));\n",
        helper = format!("file://{}", helper.display()),
        child = child.display().to_string(),
    );
    let output = Command::new("node")
        .args(["--input-type=module", "-e", &driver])
        .output()
        .unwrap();
    fs::remove_dir_all(&dir).unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const CLOSES_STDIN_THEN_EXITS: &str =
    "require('fs').closeSync(0); setTimeout(() => process.exit(EXIT), 300);";

#[test]
fn a_child_that_closes_stdin_early_reports_its_exit_without_epipe() {
    let (status, stdout, stderr) =
        run_helper_against(&CLOSES_STDIN_THEN_EXITS.replace("EXIT", "0"), "zero");
    assert!(!stderr.contains("EPIPE"), "unhandled EPIPE: {stderr}");
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(stdout.trim(), r#"{"code":0}"#);
}

#[test]
fn a_failing_child_that_closes_stdin_early_still_reports_its_failure() {
    let (status, stdout, stderr) =
        run_helper_against(&CLOSES_STDIN_THEN_EXITS.replace("EXIT", "3"), "three");
    assert!(!stderr.contains("EPIPE"), "unhandled EPIPE: {stderr}");
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(stdout.trim(), r#"{"code":3}"#);
}
