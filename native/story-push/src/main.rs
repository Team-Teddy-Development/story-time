use std::{
    fs,
    path::PathBuf,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == "new" && matches!(args[1].as_str(), "--help" | "-h") {
        println!(
            "Usage: story new --title TITLE [--statement TEXT] [--ac CRITERION]...\n\
            [--type story|spike|bug|chore] [--epic ID] [--json]\n\n\
            Creates a draft story and returns its allocated ID. Use the returned ID\n\
            for subsequent commands; do not assume the next number.\n\
            Creation is native. This help command is read-only."
        );
        return ExitCode::SUCCESS;
    }
    let command = args.first().map(String::as_str);
    if let Some((_, run)) = story_time_native::NATIVE_COMMANDS
        .iter()
        .find(|(name, _)| Some(*name) == command)
    {
        return match run(&args[1..]) {
            Ok((output, code)) => {
                print!("{output}");
                ExitCode::from(code)
            }
            Err(error) => {
                eprintln!("{}", error.message);
                ExitCode::from(error.code as u8)
            }
        };
    }
    let legacy = std::env::var_os("STORY_TIME_LEGACY_CLI")
        .map(PathBuf::from)
        .or_else(checkout_cli)
        .or_else(|| {
            let path =
                PathBuf::from(std::env::var_os("HOME")?).join(".config/story-time/runtime.json");
            let raw: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
            raw["legacyCli"].as_str().map(PathBuf::from)
        });
    let Some(legacy) = legacy else {
        eprintln!("{}", story_time_native::legacy_required_message());
        return ExitCode::from(1);
    };
    match Command::new("node").arg(legacy).args(args).status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(_) => {
            eprintln!("Could not start the retained non-push CLI; check Node and the configured legacy path.");
            ExitCode::from(1)
        }
    }
}

fn checkout_cli() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .find(|dir| dir.join("native/story-push/Cargo.toml").is_file())
        .map(|dir| dir.join("skills/story-time/scripts/story.mjs"))
        .filter(|cli| cli.is_file())
}
