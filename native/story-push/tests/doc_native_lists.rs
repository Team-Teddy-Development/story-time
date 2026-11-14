use std::{collections::BTreeSet, fs, path::PathBuf};

struct DocList {
    file: &'static str,
    start: &'static str,
    end: &'static str,
}

const LISTS: &[DocList] = &[
    DocList {
        file: "AGENTS.md",
        start: "native `story` executable for ",
        end: "\nBuild it with",
    },
    DocList {
        file: "AGENTS.md",
        start: "for commands other than ",
        end: ". Retain the old init source",
    },
    DocList {
        file: "README.md",
        start: "Native `story init`",
        end: " require Rust and Cargo.",
    },
    DocList {
        file: "skills/story-time/SKILL.md",
        start: "Use installed Rust `story` for ",
        end: "\nThe skill installer",
    },
    DocList {
        file: "skills/story-time/SKILL.md",
        start: "`story push` runs entirely in Rust.",
        end: " also run entirely in Rust.",
    },
];

fn read(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(file);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn table() -> BTreeSet<String> {
    story_time_native::NATIVE_COMMANDS
        .iter()
        .map(|(name, _)| name.to_string())
        .collect()
}

fn command(token: &str) -> String {
    let name = token.strip_prefix("story ").unwrap_or(token);
    match name {
        "adopt preview" => "adopt".to_string(),
        other => other.to_string(),
    }
}

fn named(span: &str) -> Vec<String> {
    span.split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| *token != "story")
        .map(command)
        .collect()
}

#[test]
fn every_doc_list_names_exactly_the_native_dispatch_table() {
    let table = table();
    let mut failures = Vec::new();
    for list in LISTS {
        let text = read(list.file);
        let from = text
            .find(list.start)
            .unwrap_or_else(|| panic!("{}: no list starting `{}`", list.file, list.start));
        let length = text[from..].find(list.end).unwrap_or_else(|| {
            panic!(
                "{}: no end `{}` after `{}`",
                list.file, list.end, list.start
            )
        });
        let names = named(&text[from..from + length]);
        let listed: BTreeSet<String> = names.iter().cloned().collect();
        if listed.len() != names.len() {
            failures.push(format!(
                "{} `{}`: repeats a command: {names:?}",
                list.file, list.start
            ));
        }
        let extra: Vec<_> = listed.difference(&table).collect();
        let missing: Vec<_> = table.difference(&listed).collect();
        if !extra.is_empty() || !missing.is_empty() {
            failures.push(format!(
                "{} `{}`: not native {extra:?}; leaves out {missing:?}",
                list.file, list.start
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_skill_command_table_marks_exactly_the_native_rows() {
    let table = table();
    let skill = read("skills/story-time/SKILL.md");
    let rows: Vec<&str> = skill
        .lines()
        .filter(|line| line.starts_with("| `story "))
        .collect();
    assert!(!rows.is_empty(), "no command table in the skill");
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|row| {
            let name = row["| `story ".len()..]
                .split([' ', '`'])
                .next()
                .unwrap_or_default();
            let native = table.contains(name);
            let marked = row.contains("Native Rust");
            (native != marked).then(|| format!("`{name}` native={native} marked={marked}: {row}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
