use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const USAGE: &str = "Usage: story-install --agent claude|codex|all [--source DIR] [--dry-run]
       story-install --rollback [--dry-run]

Installs the story-time release at the source checkout's HEAD commit:
  export the commit to ~/.local/share/story-time/releases/<commit>,
  save the current ~/.local/bin/story and ~/.config/story-time/runtime.json,
  cargo install the native story binary from the export,
  copy the skills for the chosen agents, and point runtime.json at the export.
--rollback restores the most recent saved binary and runtime.json.
Run it from a checkout with:
  cargo run --locked --manifest-path native/story-push/Cargo.toml --bin story-install -- --agent all";

struct Layout {
    home: PathBuf,
    bin_root: PathBuf,
    releases: PathBuf,
    backups: PathBuf,
    runtime: PathBuf,
}

impl Layout {
    fn new(home: PathBuf) -> Self {
        let share = home.join(".local/share/story-time");
        Self {
            bin_root: home.join(".local"),
            releases: share.join("releases"),
            backups: share.join("backups"),
            runtime: home.join(".config/story-time/runtime.json"),
            home,
        }
    }

    fn binary(&self) -> PathBuf {
        self.bin_root.join("bin/story")
    }

    fn skills_dir(&self, agent: &str) -> PathBuf {
        match agent {
            "claude" => self.home.join(".claude/skills"),
            _ => self.home.join(".agents/skills"),
        }
    }
}

enum Step {
    Export {
        source: PathBuf,
        commit: String,
        release: PathBuf,
    },
    Backup {
        dir: PathBuf,
    },
    Build {
        crate_dir: PathBuf,
    },
    Skills {
        from: PathBuf,
        to: PathBuf,
    },
    Runtime {
        legacy_cli: PathBuf,
    },
}

impl Step {
    fn describe(&self, layout: &Layout) -> String {
        match self {
            Step::Export {
                commit, release, ..
            } => format!("export commit {commit} to {}", release.display()),
            Step::Backup { dir } => format!(
                "save {} and {} to {}",
                layout.binary().display(),
                layout.runtime.display(),
                dir.display()
            ),
            Step::Build { crate_dir } => format!(
                "cargo install --path {} --locked --root {} --force",
                crate_dir.display(),
                layout.bin_root.display()
            ),
            Step::Skills { from, to } => {
                format!("copy skills from {} to {}", from.display(), to.display())
            }
            Step::Runtime { legacy_cli } => format!(
                "write {} with legacyCli {}",
                layout.runtime.display(),
                legacy_cli.display()
            ),
        }
    }

    fn run(&self, layout: &Layout) -> Result<(), String> {
        match self {
            Step::Export {
                source,
                commit,
                release,
            } => export(source, commit, release),
            Step::Backup { dir } => backup(layout, dir).map_err(|e| e.to_string()),
            Step::Build { crate_dir } => {
                let status = Command::new("cargo")
                    .arg("install")
                    .arg("--path")
                    .arg(crate_dir)
                    .args(["--locked", "--force", "--bin", "story", "--root"])
                    .arg(&layout.bin_root)
                    .status()
                    .map_err(|e| format!("could not start cargo: {e}"))?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("cargo install failed with {status}"))
                }
            }
            Step::Skills { from, to } => install_skills(from, to).map_err(|e| e.to_string()),
            Step::Runtime { legacy_cli } => fs::create_dir_all(layout.runtime.parent().unwrap())
                .and_then(|()| {
                    fs::write(
                        &layout.runtime,
                        format!("{}\n", serde_json::json!({ "legacyCli": legacy_cli })),
                    )
                })
                .map_err(|e| e.to_string()),
        }
    }
}

struct Options {
    agents: Vec<&'static str>,
    source: PathBuf,
    dry_run: bool,
    rollback: bool,
}

fn parse(args: &[String]) -> Result<Option<Options>, String> {
    let mut options = Options {
        agents: Vec::new(),
        source: Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        dry_run: false,
        rollback: false,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--dry-run" => options.dry_run = true,
            "--rollback" => options.rollback = true,
            "--source" => {
                options.source = args.next().ok_or("--source needs a value")?.into();
            }
            "--agent" => {
                options.agents = match args.next().map(String::as_str) {
                    Some("claude") => vec!["claude"],
                    Some("codex") => vec!["codex"],
                    Some("all") => vec!["claude", "codex"],
                    _ => return Err("--agent must be claude, codex, or all".into()),
                }
            }
            other => return Err(format!("Unknown argument {other}.")),
        }
    }
    if !options.rollback && options.agents.is_empty() {
        return Err("--agent is required.".into());
    }
    Ok(Some(options))
}

fn plan(options: &Options, layout: &Layout) -> Result<Vec<Step>, String> {
    let source = fs::canonicalize(&options.source)
        .map_err(|e| format!("{}: {e}", options.source.display()))?;
    let commit = git(&source, &["rev-parse", "HEAD"])?;
    let release = layout.releases.join(&commit);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut steps = vec![
        Step::Export {
            source,
            commit,
            release: release.clone(),
        },
        Step::Backup {
            dir: layout.backups.join(format!("{stamp:020}")),
        },
        Step::Build {
            crate_dir: release.join("native/story-push"),
        },
    ];
    steps.extend(options.agents.iter().map(|agent| Step::Skills {
        from: release.join("skills"),
        to: layout.skills_dir(agent),
    }));
    steps.push(Step::Runtime {
        legacy_cli: release.join("skills/story-time/scripts/story.mjs"),
    });
    Ok(steps)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("could not start git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn export(source: &Path, commit: &str, release: &Path) -> Result<(), String> {
    if release.is_dir() {
        return Ok(());
    }
    let partial = release.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial).map_err(|e| e.to_string())?;
    let mut archive = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["archive", "--format=tar", commit])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start git archive: {e}"))?;
    let unpacked = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(&partial)
        .stdin(archive.stdout.take().unwrap())
        .status()
        .map_err(|e| format!("could not start tar: {e}"))?;
    let archived = archive.wait().map_err(|e| e.to_string())?;
    if !archived.success() || !unpacked.success() {
        return Err(format!("exporting {commit} failed"));
    }
    fs::rename(&partial, release).map_err(|e| e.to_string())
}

fn backup(layout: &Layout, dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    for (current, saved) in [
        (layout.binary(), "story"),
        (layout.runtime.clone(), "runtime.json"),
    ] {
        if current.is_file() {
            fs::copy(&current, dir.join(saved))?;
        }
    }
    Ok(())
}

fn install_skills(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let skill = entry?.path();
        if skill.is_dir() {
            let dest = to.join(skill.file_name().unwrap());
            if dest.exists() {
                fs::remove_dir_all(&dest)?;
            }
            copy_skill(&skill, &dest, &skill)?;
        }
    }
    Ok(())
}

fn copy_skill(src: &Path, dest: &Path, skill_root: &Path) -> io::Result<()> {
    if src.strip_prefix(skill_root).unwrap() == Path::new("scripts/test") {
        return Ok(());
    }
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let path = entry?.path();
            copy_skill(&path, &dest.join(path.file_name().unwrap()), skill_root)?;
        }
        Ok(())
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}

fn rollback(layout: &Layout, dry_run: bool) -> Result<String, String> {
    let mut saved: Vec<PathBuf> = fs::read_dir(&layout.backups)
        .map_err(|_| "No saved install to restore.".to_string())?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_dir())
        .collect();
    saved.sort();
    let dir = saved.pop().ok_or("No saved install to restore.")?;
    if dry_run {
        return Ok(format!("restore {} and remove it\n", dir.display()));
    }
    for (from, to) in [
        (dir.join("story"), layout.binary()),
        (dir.join("runtime.json"), layout.runtime.clone()),
    ] {
        if from.is_file() {
            fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        } else if to.exists() {
            fs::remove_file(&to).map_err(|e| e.to_string())?;
        }
    }
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(format!(
        "Restored the install saved in {}.\n",
        dir.display()
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        eprintln!("HOME is not set.");
        return ExitCode::from(1);
    };
    let layout = Layout::new(home);
    if options.rollback {
        return match rollback(&layout, options.dry_run) {
            Ok(message) => {
                print!("{message}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        };
    }
    let steps = match plan(&options, &layout) {
        Ok(steps) => steps,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let mut unused_backup = None;
    for step in &steps {
        println!("{}", step.describe(&layout));
        if options.dry_run {
            continue;
        }
        if let Err(message) = step.run(&layout) {
            eprintln!("{message}");
            if let Some(dir) = unused_backup {
                let _ = fs::remove_dir_all(dir);
            }
            return ExitCode::from(1);
        }
        match step {
            Step::Backup { dir } => unused_backup = Some(dir),
            Step::Build { .. } => unused_backup = None,
            _ => {}
        }
    }
    if !options.dry_run {
        println!("Installed. Undo with: story-install --rollback");
    }
    ExitCode::SUCCESS
}
