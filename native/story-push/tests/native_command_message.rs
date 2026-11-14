use std::{fs, process::Command};

fn names(list: &str) -> Vec<String> {
    list.split(", ")
        .flat_map(|part| part.split(" and "))
        .map(|name| name.trim_start_matches("and ").to_lowercase())
        .collect()
}

#[test]
fn the_legacy_required_message_names_exactly_the_dispatch_table() {
    let root = std::env::temp_dir().join(format!("story-native-message-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("home")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_story"))
        .arg("stats")
        .env_clear()
        .env("PATH", "/nonexistent")
        .env("HOME", root.join("home"))
        .current_dir(&root)
        .output()
        .unwrap();
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let message = String::from_utf8(output.stderr).unwrap();
    let rest = message
        .strip_prefix("Commands other than ")
        .unwrap_or_else(|| panic!("unexpected message: {message}"));
    let (excluded, rest) = rest
        .split_once(
            " require STORY_TIME_LEGACY_CLI or ~/.config/story-time/runtime.json with legacyCli. ",
        )
        .unwrap_or_else(|| panic!("unexpected message: {message}"));
    let native = rest
        .strip_suffix(" are fully native and need neither Node nor this setting.\n")
        .unwrap_or_else(|| panic!("unexpected message: {message}"));
    let table: Vec<String> = story_time_native::NATIVE_COMMANDS
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(names(excluded), table);
    assert_eq!(names(native), table);
}
