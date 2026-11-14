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
    if args.first().is_some_and(|s| s == "machine") {
        return match story_time_native::machine::run(&args[1..]) {
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
    if args.first().is_some_and(|s| s == "adopt") {
        let (output, code) = story_time_native::adoption::run(&args[1..]);
        print!("{output}");
        return ExitCode::from(code);
    }
    type Native = fn(&[String]) -> Result<String, story_time_native::Failure>;
    let native: Option<Native> = match args.first().map(String::as_str) {
        Some("init") => Some(story_time_native::init::run),
        Some("push") => Some(story_time_native::push),
        Some("config") => Some(story_time_native::config::run),
        Some("issues") => Some(story_time_native::issues),
        Some("show") => Some(story_time_native::show::run),
        Some("list") => Some(story_time_native::list::run),
        Some("secret") => Some(story_time_native::secret::run),
        Some("metrics") => Some(story_time_native::metrics::run),
        _ => None,
    };
    if let Some(run) = native {
        return match run(&args[1..]) {
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
        .or_else(checkout_cli)
        .or_else(|| {
            let path =
                PathBuf::from(std::env::var_os("HOME")?).join(".config/story-time/runtime.json");
            let raw: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
            raw["legacyCli"].as_str().map(PathBuf::from)
        });
    let Some(legacy) = legacy else {
        eprintln!("Commands other than init, push, adopt, plan, config, show, issues, list, secret, machine, and metrics require STORY_TIME_LEGACY_CLI or ~/.config/story-time/runtime.json with legacyCli. Init, push, adopt, plan, config, show, issues, list, secret, machine, and metrics are fully native and need neither Node nor this setting.");
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
