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
