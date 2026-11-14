use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn cli() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs")
}

fn sandbox(name: &str, settings: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("story-unknown-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("home")).unwrap();
    if let Some(settings) = settings {
        fs::create_dir_all(dir.join(".story-time")).unwrap();
        fs::write(dir.join(".story-time/settings.json"), settings).unwrap();
    }
    dir
}

fn node(dir: &Path, args: &[&str]) -> Output {
    Command::new("node")
        .arg(cli())
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", dir.join("home"))
        .current_dir(dir)
        .output()
        .unwrap()
}

fn native(dir: &Path, args: &[&str], env: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_story"));
    command
        .args(args)
        .env_clear()
        .env("HOME", dir.join("home"))
        .current_dir(dir);
    for (key, value) in env {
        command.env(key, value);
    }
    if !env.iter().any(|(key, _)| *key == "PATH") {
        command.env("PATH", "/nonexistent");
    }
    command.output().unwrap()
}

fn same(label: &str, node: &Output, native: &Output) {
    assert_eq!(
        String::from_utf8_lossy(&native.stderr),
        String::from_utf8_lossy(&node.stderr),
        "{label}: stderr"
    );
    assert_eq!(native.stdout, node.stdout, "{label}: stdout");
    assert_eq!(native.status.code(), node.status.code(), "{label}: exit");
}

#[test]
fn unknown_commands_and_help_flags_match_node_without_node() {
    let cases: [(&str, &[&str], Option<&str>); 10] = [
        ("unknown", &["no-such-command"], None),
        ("unknown-flags", &["no-such-command", "--json", "x"], None),
        ("help-long", &["--help"], None),
        ("help-short", &["-h"], None),
        ("bad-config", &["no-such-command"], Some("{ not json")),
        ("constructor", &["constructor"], None),
        ("to-string", &["toString"], None),
        ("has-own-property", &["hasOwnProperty"], None),
        ("proto", &["__proto__"], None),
        ("value-of", &["valueOf"], None),
    ];
    for (name, args, settings) in cases {
        let dir = sandbox(name, settings);
        let expected = node(&dir, args);
        assert_eq!(expected.status.code(), Some(2), "{name}: node exit");
        let actual = native(&dir, args, &[]);
        fs::remove_dir_all(&dir).unwrap();
        same(name, &expected, &actual);
    }
}

#[test]
fn the_native_binary_never_starts_a_node_cli() {
    let dir = sandbox("no-fallback", None);
    let fake = dir.join("fake-story.mjs");
    fs::write(&fake, "console.log('legacy');\n").unwrap();
    fs::create_dir_all(dir.join("home/.config/story-time")).unwrap();
    fs::write(
        dir.join("home/.config/story-time/runtime.json"),
        format!("{{\"legacyCli\":{:?}}}\n", fake.display().to_string()),
    )
    .unwrap();
    let expected = node(&dir, &["no-such-command"]);
    let path = PathBuf::from(std::env::var_os("PATH").unwrap_or_default());
    let actual = native(
        &dir,
        &["no-such-command"],
        &[("PATH", &path), ("STORY_TIME_LEGACY_CLI", &fake)],
    );
    fs::remove_dir_all(&dir).unwrap();
    same("no-fallback", &expected, &actual);
}
