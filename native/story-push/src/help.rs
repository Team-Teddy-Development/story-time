//! `story help`: the usage text that the Node CLI also prints, from the one shared file.

use crate::{config, Failure};

const USAGE: &str = include_str!("../../../skills/story-time/scripts/usage.txt");

pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = config::Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    Ok(USAGE.into())
}

pub fn unknown_command(command: &str) -> String {
    format!("Unknown command \"{command}\".\n\n{}", USAGE.trim_end())
}
