use std::{
    fs,
    path::PathBuf,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.as_slice(), [command, flag] if command == "init" && (flag == "--help" || flag == "-h"))
    {
        println!("Usage: story init [--from answers.json] [--force] [--config path] [--json]\n\nWrite validated project settings to .story-time/settings.json.\n\n  --from FILE    Read settings from a JSON file; defaults apply when omitted.\n  --force        Replace an existing settings file after validation.\n  --config PATH  Write settings to an explicit path.\n  --json         Print the effective settings as JSON.\n  -h, --help     Show this help without reading or writing project files.\n\nGitHub settings use github.repo as owner/repo, for example acme/shop.\nGitea settings use separate gitea.baseUrl, gitea.owner, and gitea.repo fields.\nHelp is native Rust. Other init operations use the configured retained CLI.");
        return ExitCode::SUCCESS;
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
        eprintln!("Non-push commands require STORY_TIME_LEGACY_CLI or ~/.config/story-time/runtime.json with legacyCli. Push is fully native and needs neither Node nor this setting.");
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
