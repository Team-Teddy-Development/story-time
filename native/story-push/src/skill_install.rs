use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const ALL: [Agent; 2] = [Agent::Claude, Agent::Codex];

    pub fn parse(name: &str) -> Option<Vec<Agent>> {
        match name {
            "claude" => Some(vec![Agent::Claude]),
            "codex" => Some(vec![Agent::Codex]),
            "all" => Some(Agent::ALL.to_vec()),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }

    pub fn skills_root(self, target: &Target) -> PathBuf {
        let root = match (self, target) {
            (Agent::Claude, Target::User { home, .. }) => home.join(".claude/skills"),
            (Agent::Codex, Target::User { home, codex_home }) => match codex_home {
                Some(codex_home) => codex_home.join("skills"),
                None => home.join(".agents/skills"),
            },
            (Agent::Claude, Target::Project(project)) => project.join(".claude/skills"),
            (Agent::Codex, Target::Project(project)) => project.join(".agents/skills"),
        };
        normalize(&root)
    }
}

pub enum Target {
    User {
        home: PathBuf,
        codex_home: Option<PathBuf>,
    },
    Project(PathBuf),
}

pub struct Copy {
    pub agent: Agent,
    pub skill: String,
    pub dest: PathBuf,
}

pub fn plan(agents: &[Agent], target: &Target, skills: &[String]) -> Vec<Copy> {
    agents
        .iter()
        .flat_map(|&agent| {
            let root = agent.skills_root(target);
            skills.iter().map(move |skill| Copy {
                agent,
                skill: skill.clone(),
                dest: root.join(skill),
            })
        })
        .collect()
}

pub fn skill_names(skills_dir: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(skills_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

pub fn install(src: &Path, dest: &Path) -> io::Result<()> {
    remove(dest)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_tree(src, dest, src)
}

pub fn remove(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn copy_tree(src: &Path, dest: &Path, skill_root: &Path) -> io::Result<()> {
    if src.strip_prefix(skill_root).unwrap() == Path::new("scripts/test") {
        return Ok(());
    }
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let path = entry?.path();
            copy_tree(&path, &dest.join(path.file_name().unwrap()), skill_root)?;
        }
        fs::set_permissions(dest, fs::metadata(src)?.permissions())
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}

/// Lexical normalization as Node's path.join and path.resolve do it: no symlinks are followed.
pub fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                Some(Component::RootDir) => {}
                _ => parts.push(part),
            },
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        PathBuf::from(".")
    } else {
        parts.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_matches_node_path_join() {
        for (input, expected) in [
            ("/a/b/../c/", "/a/c"),
            ("/../a", "/a"),
            ("a/../..", ".."),
            ("a/..", "."),
            ("./x//y/.", "x/y"),
        ] {
            assert_eq!(normalize(Path::new(input)), Path::new(expected), "{input}");
        }
    }
}
