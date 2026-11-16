//! Shared helpers for contract tests that run the built `story` binary against a fake `gh`.
#![allow(dead_code)]

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// The fake `gh`. It drains stdin, logs its arguments to `gh-calls`, and answers from
/// `responses.tsv`: `<argument prefix>\t<exit code>\t<stdout file>\t<stderr text>`. A line
/// matches when the arguments, joined by single spaces, start with its prefix; the first match
/// answers. No match exits 0 with no output. Paths are relative to the working directory.
const STUB_GH: &str = r#"
use std::{env, fs, fs::OpenOptions, io::{Read, Write}};
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let joined = args.join(" ");
    let mut log = OpenOptions::new().create(true).append(true).open("gh-calls").unwrap();
    writeln!(log, "{}", joined).unwrap();
    let table = fs::read_to_string("responses.tsv").unwrap_or_default();
    for line in table.lines() {
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() == 4 && joined.starts_with(cells[0]) {
            if !cells[2].is_empty() {
                print!("{}", fs::read_to_string(cells[2]).unwrap());
            }
            if !cells[3].is_empty() {
                eprintln!("{}", cells[3]);
            }
            std::process::exit(cells[1].parse().unwrap());
        }
    }
}
"#;

/// Compiles the fake `gh` into `bin_dir/gh`.
pub fn install_stub_gh(bin_dir: &Path) {
    std::fs::create_dir_all(bin_dir).unwrap();
    let mut rustc = Command::new("rustc")
        .args(["--crate-name", "gh_stub", "-o"])
        .arg(bin_dir.join("gh"))
        .arg("-")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    rustc
        .stdin
        .take()
        .unwrap()
        .write_all(STUB_GH.as_bytes())
        .unwrap();
    let out = rustc.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Adds one answer to `dir/responses.tsv`, with its stdout stored in a numbered file.
pub fn respond(dir: &Path, prefix: &str, exit: i32, stdout: &str, stderr: &str) {
    let table = dir.join("responses.tsv");
    let count = std::fs::read_to_string(&table)
        .map(|t| t.lines().count())
        .unwrap_or(0);
    let file = format!("response-{count}.out");
    std::fs::write(dir.join(&file), stdout).unwrap();
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(table)
        .unwrap();
    writeln!(out, "{prefix}\t{exit}\t{file}\t{stderr}").unwrap();
}

/// `bin_dir` plus the directory of the real `git`, for an otherwise empty `PATH`.
pub fn path_with_git(bin_dir: &Path) -> std::ffi::OsString {
    let git = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let git = PathBuf::from(String::from_utf8(git.stdout).unwrap().trim());
    std::env::join_paths([bin_dir.to_path_buf(), git.parent().unwrap().to_path_buf()]).unwrap()
}
