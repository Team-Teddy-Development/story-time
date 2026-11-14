use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const FAKE_TOKEN: &str = "fake-token-never-printed";

const STORIES: [(&str, &str); 7] = [
    (
        "ST-001",
        "---
id: ST-001
title: Reset password
status: ready
type: story
epic: EP-1
value: 8
time_criticality: 5
risk_reduction: 3
size: 3
---
## Story
As a user, I want a reset link, so that I can sign in.

## 2
Integer-named section comes first.

## Acceptance Criteria
- [ ] Given an email <exists> & is valid, when I ask, then a link is sent
* [X] Given a used token, when I open it, then it fails
- [x]no space keeps the box
-not an item

## Notes
Keep this & that.

## Out of Scope
- SMS reset

## Test Plan
Internal.

## Changelog
- 2026-11-02 created
",
    ),
    (
        "ST-002",
        "---
id: ST-002
title: Which queue?
status: refined
type: spike
size: 2
timebox: 2d
---
## Question
Which queue fits?

## Exit Criteria
- [ ] A recommendation is written

## Findings
None yet.
",
    ),
    (
        "ST-003",
        "---
id: ST-003
title: Login fails
status: ready
type: bug
size: 1
value: 5
time_criticality: 1
risk_reduction: 1
---
## Story
Login fails on Safari.

## Steps to Reproduce
1. Open Safari

## Expected
It works.

## Actual
It fails.

## Acceptance Criteria
- [ ] Given Safari, when I log in, then it works
",
    ),
    (
        "ST-004",
        "---
id: ST-004
title: It's `$HOME` and \"quotes\" & <tags>
status: draft
type: chore
size: 0.5
value: 1
time_criticality: 0
risk_reduction: 0
---
## Story
First paragraph
second line.


Second paragraph with 'single' quotes.

## Acceptance Criteria
- [ ] Given it's quoted, when exported, then the shell sees one argument
",
    ),
    ("ST-005", "---\nid: ST-005\ntitle: Bare\n---\n"),
    (
        "ST-006",
        "---\r\nid: ST-006\r\ntitle: \"ST-006: Already prefixed\"\r\nstatus: done\r\ntype: story\r\nsize: 5\r\nepic: EP-2\r\n---\r\n## Story\r\nCRLF story.\r\n\r\n## Acceptance Criteria\r\n- [ ] Given CRLF, when exported, then it matches\r\n",
    ),
    (
        "PAY-12",
        "---\nid: PAY-12\ntitle: Pay\ntype: spike\n---\n## Question\nWhy?\n",
    ),
];

struct Sandbox(PathBuf);
impl Sandbox {
    fn new(settings: Option<&str>) -> Self {
        let path = std::env::temp_dir().join(format!(
            "story-export-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("backlog")).unwrap();
        let s = Self(path.canonicalize().unwrap());
        for (id, text) in STORIES {
            s.write(&format!("backlog/{id}.md"), text);
        }
        if let Some(settings) = settings {
            s.write(".story-time/settings.json", settings);
        }
        s.write(
            ".story-time/.github.env",
            &format!("GH_TOKEN={FAKE_TOKEN}\n"),
        );
        fs::set_permissions(
            s.0.join(".story-time/.github.env"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        s
    }
    fn write(&self, relative: &str, text: &str) -> &Self {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }
    fn files(&self) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
        fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let mode = fs::metadata(&path).unwrap().permissions().mode();
                if path.is_dir() {
                    into.insert(path.clone(), (mode, Vec::new()));
                    walk(&path, into);
                } else {
                    into.insert(path.clone(), (mode, fs::read(&path).unwrap()));
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&self.0, &mut files);
        files
    }
    fn run(&self, native: bool, args: &[&str]) -> Output {
        let mut cmd = if native {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_story"));
            cmd.env_clear().env("PATH", "/nonexistent");
            cmd
        } else {
            let mut cmd = Command::new("node");
            cmd.arg(legacy())
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap());
            cmd
        };
        cmd.arg("export")
            .args(args)
            .env("HOME", &self.0)
            .env("GH_TOKEN", FAKE_TOKEN)
            .env("AZURE_DEVOPS_EXT_PAT", FAKE_TOKEN)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
    /// Runs Node and native on the same tree and requires equal stdout, stderr, exit code, and files.
    fn compare(&self, args: &[&str]) -> Output {
        let before = self.files();
        let node = self.run(false, args);
        assert_eq!(before, self.files(), "Node changed files for {args:?}");
        let native = self.run(true, args);
        assert_eq!(before, self.files(), "native changed files for {args:?}");
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        assert_eq!(
            text(&native.stdout),
            text(&node.stdout),
            "stdout differs for {args:?}; native stderr: {}",
            text(&native.stderr)
        );
        assert_eq!(
            native.status.code(),
            node.status.code(),
            "exit code differs for {args:?}; native stderr: {}; node stderr: {}",
            text(&native.stderr),
            text(&node.stderr)
        );
        assert_eq!(
            text(&native.stderr),
            text(&node.stderr),
            "stderr differs for {args:?}"
        );
        assert!(!text(&native.stdout).contains(FAKE_TOKEN));
        native
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn legacy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/story-time/scripts/story.mjs")
}

const ALL_TRACKERS: &str = r#"{"tracker":"ado","sync":"ask",
  "github":{"repo":"acme/shop.web"},
  "ado":{"organization":"https://dev.azure.com/acme","project":"Team's Board","process":"scrum",
         "areaPath":"Shop\\Web","iterationPath":"Shop\\Sprint 1"},
  "gitea":{"baseUrl":"https://gitea.example.com/","owner":"acme","repo":"shop"}}"#;

#[test]
fn every_format_and_command_matches_node() {
    for settings in [
        Some(ALL_TRACKERS),
        Some(r#"{"tracker":"github","github":{"repo":"acme/shop"}}"#),
        Some(
            r#"{"tracker":"ado","ado":{"organization":"https://dev.azure.com/acme","project":"P"}}"#,
        ),
        Some(
            r#"{"tracker":"ado","ado":{"organization":"https://dev.azure.com/acme","project":"P","process":"basic"}}"#,
        ),
        Some(
            r#"{"tracker":"gitea","gitea":{"baseUrl":"https://g.example.com","owner":"o","repo":"r"}}"#,
        ),
        None,
    ] {
        let s = Sandbox::new(settings);
        for (id, _) in STORIES {
            for format in [
                None,
                Some("github"),
                Some("gitea"),
                Some("ado"),
                Some("markdown"),
            ] {
                for extra in [&[][..], &["--json"][..], &["--command"][..]] {
                    let mut args = vec![id];
                    if let Some(format) = format {
                        args.extend(["--format", format]);
                    }
                    args.extend(extra);
                    s.compare(&args);
                }
            }
        }
    }
}

#[test]
fn printed_commands_quote_like_node() {
    let s = Sandbox::new(Some(ALL_TRACKERS));
    let gh = s.compare(&["ST-004", "--format", "github", "--command"]);
    let gh = String::from_utf8(gh.stdout).unwrap();
    assert!(gh.starts_with(
        "gh issue create --repo acme/shop.web --title 'It'\\''s `$HOME` and \"quotes\" & <tags>' --label chore --label size:0.5 --label value:1 --body-file - <<'STORY_TIME_EOF'\n"
    ));
    assert!(gh.ends_with("\nSTORY_TIME_EOF\n"));
    let az = s.compare(&["ST-001", "--command"]);
    let az = String::from_utf8(az.stdout).unwrap();
    assert!(az.starts_with("az boards work-item create --org 'https://dev.azure.com/acme' --project 'Team'\\''s Board' --type 'Product Backlog Item' --title 'Reset password' \\\n  --description '<p>"));
    assert!(az.contains("--area 'Shop\\Web' \\\n  --iteration 'Shop\\Sprint 1' \\\n  --fields 'Microsoft.VSTS.Common.AcceptanceCriteria=<ul>"));
}

#[test]
fn failures_match_node() {
    let configured = Sandbox::new(Some(ALL_TRACKERS));
    let bare = Sandbox::new(None);
    for (s, args, code) in [
        (&configured, &[][..], 2),
        (&configured, &[""][..], 2),
        (&configured, &["", "--format", "jira"][..], 2),
        (&configured, &["--json"][..], 2),
        (&configured, &["ST-001", "--format"][..], 2),
        (&configured, &["st-001"][..], 1),
        (&configured, &["ST-404"][..], 1),
        (&configured, &["ST-001", "--format", "jira"][..], 1),
        (&configured, &["ST-001", "--format", ""][..], 1),
        (
            &configured,
            &["ST-001", "--format", "gitea", "--command"][..],
            1,
        ),
        (
            &configured,
            &["ST-001", "--format", "markdown", "--command"][..],
            1,
        ),
        (
            &configured,
            &["ST-001", "--format", "jira", "--command"][..],
            1,
        ),
        (&bare, &["ST-001", "--command"][..], 1),
        (&bare, &["ST-001", "--format", "github", "--command"][..], 1),
        (&bare, &["ST-001", "--format", "ado", "--command"][..], 1),
    ] {
        let out = s.compare(args);
        assert_eq!(out.status.code(), Some(code), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?} printed to stdout");
    }
    bare.write(".story-time/settings.json", r#"{"colour":"red"}"#);
    assert_eq!(bare.compare(&["ST-001"]).status.code(), Some(1));
}
