use std::io::Write;
use std::process::ExitCode;
use std::time::{Instant, SystemTime};
use story_time_native::otlp;

fn main() -> ExitCode {
    let started = Instant::now();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        args.push("help".into());
    }
    let (operation, exit_code) = dispatch(&args);
    let _ = std::io::stdout().flush();
    otlp::export_command(&otlp::CommandEvent {
        operation,
        exit_code,
        duration: started.elapsed(),
        finished: SystemTime::now(),
        correlation_id: otlp::correlation_id(),
    });
    ExitCode::from(exit_code)
}

/// Runs the command and returns its bounded operation name (a dispatch-table name or
/// `unknown`) with its exit code.
fn dispatch(args: &[String]) -> (&'static str, u8) {
    if args.len() == 2 && args[0] == "new" && matches!(args[1].as_str(), "--help" | "-h") {
        println!(
            "Usage: story new --title TITLE [--statement TEXT] [--ac CRITERION]...\n\
            [--type story|spike|bug|chore] [--epic ID] [--json]\n\n\
            Creates a draft story and returns its allocated ID. Use the returned ID\n\
            for subsequent commands; do not assume the next number.\n\
            Creation is native. This help command is read-only."
        );
        return ("new", 0);
    }
    let command = args[0].as_str();
    let Some((name, run)) = story_time_native::NATIVE_COMMANDS
        .iter()
        .find(|(name, _)| *name == command)
    else {
        eprintln!("{}", story_time_native::help::unknown_command(command));
        return ("unknown", 2);
    };
    match run(&args[1..]) {
        Ok((output, code)) => {
            print!("{output}");
            (name, code)
        }
        Err(error) => {
            eprintln!("{}", error.message);
            (name, error.code as u8)
        }
    }
}
