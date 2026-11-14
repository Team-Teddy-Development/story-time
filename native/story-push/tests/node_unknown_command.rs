use std::{
    path::PathBuf,
    process::{Command, Output},
};

fn node_cli(command: &str) -> Output {
    let cli =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs");
    Command::new("node")
        .arg(cli)
        .arg(command)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

#[test]
fn the_node_cli_fails_and_names_the_native_binary_for_a_native_only_command() {
    for command in ["metrics", "plan", "adopt", "issues"] {
        let output = node_cli(command);
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(2), "{command}: {stderr}");
        assert!(output.stdout.is_empty(), "{command}");
        assert_eq!(
            stderr.lines().next(),
            Some(
                format!(
                    "The \"{command}\" command runs only in the native story binary (native/story-push); this Node CLI cannot run it."
                )
                .as_str()
            ),
            "{command}: {stderr}"
        );
    }
}

#[test]
fn the_node_cli_fails_on_an_unknown_command() {
    let output = node_cli("no-such-command");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr.lines().next(),
        Some("Unknown command \"no-such-command\".")
    );
}

#[test]
fn the_node_cli_never_calls_a_native_command_unknown() {
    let cli =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs");
    for (command, _) in story_time_native::NATIVE_COMMANDS {
        let dir = std::env::temp_dir().join(format!("st087-node-{}-{command}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let output = Command::new("node")
            .arg(&cli)
            .arg(command)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap())
            .env("HOME", &dir)
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_ne!(
            stderr.lines().next(),
            Some(format!("Unknown command \"{command}\".").as_str()),
            "native command {command} needs a Node handler or a NATIVE_ONLY entry"
        );
    }
}
