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
            Creation requires the retained Node CLI. This help command is native and read-only."
        );
        return ExitCode::SUCCESS;
    }
    if args.first().is_some_and(|s| s == "plan") {
        let (output, code) = story_time_native::planning::run(&args[1..]);
        print!("{output}");
        return ExitCode::from(code);
    }
    if args.first().is_some_and(|s| s == "adopt") {
        let (output, code) = story_time_native::adoption::run(&args[1..]);
        print!("{output}");
        return ExitCode::from(code);
    }
    if args.first().is_some_and(|s| s == "init") {
        return match story_time_native::init::run(&args[1..]) {
            Ok(output) => {
                print!("{output}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{}", error.message);
                ExitCode::from(error.code as u8)
            }
        };
    }
    if args.first().is_some_and(|s| s == "push") {
        return match story_time_native::push(&args[1..]) {
            Ok(output) => {
                print!("{output}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{}", error.message);
                ExitCode::from(error.code as u8)
            }
        };
    }
    let legacy = std::env::var_os("STORY_TIME_LEGACY_CLI")
        .map(PathBuf::from)
        .or_else(|| {
            let path =
                PathBuf::from(std::env::var_os("HOME")?).join(".config/story-time/runtime.json");
            let raw: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
            raw["legacyCli"].as_str().map(PathBuf::from)
        });
    let Some(legacy) = legacy else {
        eprintln!("Commands other than init, push, adopt, and plan require STORY_TIME_LEGACY_CLI or ~/.config/story-time/runtime.json with legacyCli. Init, push, adopt, and plan are fully native and need neither Node nor this setting.");
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
