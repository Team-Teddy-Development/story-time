use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use story_time_native::skill_install::{self, Agent, Target};

const USAGE: &str = "Usage: story-install-skills --agent codex|claude|all [--scope user|project]
                            [--project DIR] [--codex-home DIR] [--source DIR]
                            [--force] [--dry-run]

  --source DIR  the story-time checkout or release whose skills/ to copy
                (default: the checkout this binary was built from)

  codex  user    ~/.agents/skills      (or $CODEX_HOME/skills with --codex-home)
  codex  project <project>/.agents/skills
  claude user    ~/.claude/skills
  claude project <project>/.claude/skills";

enum Failure {
    Usage(String),
    Fatal(String),
}

#[derive(Default)]
struct Flags {
    agent: Option<String>,
    scope: Option<String>,
    project: Option<String>,
    codex_home: Option<String>,
    source: Option<String>,
    force: bool,
    dry_run: bool,
}

fn parse(args: &[String]) -> Result<Flags, Failure> {
    let mut flags = Flags::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let Some(key) = arg.strip_prefix("--") else {
            return Err(Failure::Usage(format!("Unexpected argument \"{arg}\".")));
        };
        let slot = match key {
            "agent" => &mut flags.agent,
            "scope" => &mut flags.scope,
            "project" => &mut flags.project,
            "codex-home" => &mut flags.codex_home,
            "source" => &mut flags.source,
            "force" => {
                flags.force = true;
                continue;
            }
            "dry-run" => {
                flags.dry_run = true;
                continue;
            }
            _ => return Err(Failure::Usage(format!("Unknown flag {arg}."))),
        };
        let value = args
            .next()
            .ok_or_else(|| Failure::Usage(format!("--{key} needs a value.")))?;
        *slot = Some(value.clone());
    }
    Ok(flags)
}

fn target(flags: &Flags, home: PathBuf) -> Result<Target, Failure> {
    match flags.scope.as_deref().unwrap_or("user") {
        "user" => Ok(Target::User {
            home,
            codex_home: flags
                .codex_home
                .as_deref()
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
        }),
        "project" => {
            let cwd = std::env::current_dir().map_err(|e| Failure::Fatal(e.to_string()))?;
            let project = cwd.join(flags.project.as_deref().unwrap_or("."));
            Ok(Target::Project(skill_install::normalize(&project)))
        }
        other => Err(Failure::Fatal(format!(
            "Unknown scope \"{other}\". Use: user, project."
        ))),
    }
}

fn run(args: &[String]) -> Result<(), Failure> {
    let flags = parse(args)?;
    let Some(agent) = flags.agent.as_deref() else {
        return Err(Failure::Usage(format!("--agent is required.\n\n{USAGE}")));
    };
    let agents = Agent::parse(agent).ok_or_else(|| {
        Failure::Fatal(format!(
            "Unknown agent \"{agent}\". Use: claude, codex, all."
        ))
    })?;
    let cwd = std::env::current_dir().map_err(|e| Failure::Fatal(e.to_string()))?;
    let source = match &flags.source {
        Some(dir) => cwd.join(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    };
    let skills_dir = skill_install::normalize(&source.join("skills"));
    if !skills_dir.is_dir() {
        return Err(Failure::Fatal(format!(
            "No story-time skills folder at {}.\nPass --source DIR with the story-time checkout or release that holds skills/.",
            skills_dir.display()
        )));
    }
    let fatal = |e: std::io::Error| Failure::Fatal(e.to_string());
    let skills = skill_install::skill_names(&skills_dir).map_err(fatal)?;
    let home = std::env::home_dir()
        .filter(|home| home.is_absolute())
        .ok_or_else(|| Failure::Fatal("Cannot find your home folder.".into()))?;
    let plan = skill_install::plan(&agents, &target(&flags, home)?, &skills);

    let clashes: Vec<String> = plan
        .iter()
        .filter(|copy| copy.dest.exists())
        .map(|copy| format!("  {}", copy.dest.display()))
        .collect();
    if !clashes.is_empty() && !flags.force {
        return Err(Failure::Fatal(format!(
            "A skill already exists at each of these paths:\n{}\nRe-run with --force to replace them (local edits will be lost).",
            clashes.join("\n")
        )));
    }

    for copy in &plan {
        if flags.dry_run {
            println!(
                "{:<6} {:<20} -> {}",
                copy.agent.name(),
                copy.skill,
                copy.dest.display()
            );
        } else {
            skill_install::install(&skills_dir.join(&copy.skill), &copy.dest).map_err(fatal)?;
        }
    }
    if !flags.dry_run {
        let mut roots: Vec<&Path> = Vec::new();
        for copy in &plan {
            let root = copy.dest.parent().unwrap();
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
        println!("Installed {} story-time skills into:", skills.len());
        for root in roots {
            println!("  {}", root.display());
        }
        if agents.contains(&Agent::Codex) {
            println!("Codex: restart codex, then try \"$story-time\" or \"$story-pick\".");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
        Err(Failure::Fatal(message)) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}
