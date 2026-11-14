use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        args.push("help".into());
    }
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
    let command = args[0].as_str();
    if let Some((_, run)) = story_time_native::NATIVE_COMMANDS
        .iter()
        .find(|(name, _)| *name == command)
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
    eprintln!("{}", story_time_native::help::unknown_command(command));
    ExitCode::from(2)
}
