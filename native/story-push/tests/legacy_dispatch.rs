use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("story-dispatch-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let checkout = root.join("checkout");
        fake_cli(
            &checkout.join("skills/story-time/scripts/story.mjs"),
            "checkout",
        );
        fs::create_dir_all(checkout.join("native/story-push")).unwrap();
        fs::write(checkout.join("native/story-push/Cargo.toml"), "").unwrap();
        fs::create_dir_all(checkout.join("backlog/deeper")).unwrap();
        fake_cli(&root.join("release/story.mjs"), "runtime");
        fake_cli(&root.join("env/story.mjs"), "env");
        fs::create_dir_all(root.join("home/.config/story-time")).unwrap();
        fs::write(
            root.join("home/.config/story-time/runtime.json"),
            format!(
                "{{\"legacyCli\":{:?}}}",
                root.join("release/story.mjs").display().to_string()
            ),
        )
        .unwrap();
        fs::create_dir_all(root.join("project")).unwrap();
        Self(root)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fake_cli(path: &Path, label: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("console.log('{label}');\n")).unwrap();
}

fn dispatched_to(sandbox: &Sandbox, cwd: &str, env_cli: bool) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_story"));
    command
        .arg("list")
        .current_dir(sandbox.0.join(cwd))
        .env("HOME", sandbox.0.join("home"))
        .env_remove("STORY_TIME_LEGACY_CLI");
    if env_cli {
        command.env("STORY_TIME_LEGACY_CLI", sandbox.0.join("env/story.mjs"));
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn inside_a_checkout_without_the_variable_uses_the_checkout_cli() {
    let sandbox = Sandbox::new("checkout");
    assert_eq!(dispatched_to(&sandbox, "checkout", false), "checkout");
    assert_eq!(
        dispatched_to(&sandbox, "checkout/backlog/deeper", false),
        "checkout"
    );
}

#[test]
fn the_variable_wins_inside_and_outside_a_checkout() {
    let sandbox = Sandbox::new("env");
    assert_eq!(dispatched_to(&sandbox, "checkout", true), "env");
    assert_eq!(dispatched_to(&sandbox, "project", true), "env");
}

#[test]
fn outside_a_checkout_uses_the_runtime_file() {
    let sandbox = Sandbox::new("runtime");
    assert_eq!(dispatched_to(&sandbox, "project", false), "runtime");
}
