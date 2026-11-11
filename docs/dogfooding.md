# Dogfooding with GitHub Issues

The local backlog is `backlog/ST-###.md`. GitHub Issues mirrors these stories in the repository named by `.story-time/settings.json`.
The configuration uses `sync: ask`, so publishing remains an explicit action.

## Create a feature story

```sh
git fetch --prune origin
node skills/story-time/scripts/story.mjs config
node skills/story-time/scripts/story.mjs new --title "Feature outcome" --type story --persona "story-time maintainer" --statement "As a story-time maintainer, I want an observable outcome, so that I can plan feature work." --ac "Given the starting state, when the action occurs, then the observable result follows."
node skills/story-time/scripts/story.mjs push ST-### --dry-run
node skills/story-time/scripts/story.mjs push ST-###
```

Replace the example wording and ID with the actual feature.
Use the story-create, story-ready, story-kickoff, and story-accept skills for the lifecycle.
Authenticate with `gh auth login`. Tokens stay outside committed settings; `.story-time/.gitignore` excludes `*.env`.

## PR checks

Start a declaration line with `Stories:` or `Story:`.
For example: `Stories: ST-001 (#11)`.
Only references on declaration lines count.
Every local ID must have an existing `backlog/ST-###.md` file.
Every issue number must name a GitHub issue with a `story`, `bug`, `spike`, or `chore` label.
A pull request number is not a story issue.
An incident with no declaration can pass; it still needs a bug story within one business day.
A declared invalid reference fails even for incidents.

The ID check compares newly added story filenames with the base branch and older open PRs.
The older PR keeps a contested ID.
Fetch and recreate the newer story with its next available ID.
GitHub API failures fail the check; they do not mean the issue or competing story is absent.

After the workflows exist on the default branch, require the exact job contexts `story-link` and `story-ids` in its branch protection or ruleset.
Workflow files alone run checks; required status checks prevent merging red checks.
Keep existing required checks when adding these contexts.
The CI token uses read-only permissions. Fork PRs receive no repository write credentials.

## Verify locally

```sh
rustc --edition=2021 --test scripts/story-policy.rs -o /tmp/story-policy-tests
/tmp/story-policy-tests
rustc --edition=2021 --test test/story-policy-contract.rs -o /tmp/story-policy-contract
/tmp/story-policy-contract
rustc --edition=2021 scripts/story-policy.rs -o /tmp/story-policy
REPO=Team-Teddy-Development/story-time PR_BODY='Stories: ST-001' /tmp/story-policy story-link
npm test
```

For the ID check, supply `REPO`, `PR`, `BASE`, and the exact `HEAD_SHA`.
It reads local Git history and GitHub PR file lists without changing issues.

## Rust conversion acceptance gate

Scope: the new story-mandate execution chain.

| Implementation | Language | Role | Status |
| --- | --- | --- | --- |
| `scripts/story-policy.rs` | Rust | Reference validation, GitHub reads, ID conflict detection, unit tests | Converted at introduction |
| `test/story-policy-contract.rs` | Rust | Repository integration checks | Converted at introduction |
| Workflow YAML | Configuration | Compile and invoke Rust with platform event data | Not executable application logic |
| Git and GitHub CLI | Third-party interfaces | Git history and authenticated GitHub API access | External dependencies |

Gate: MET when both Rust test binaries pass and the real checker runs.
No new executable implementation in this chain uses another language.
The existing Node story CLI, installer, and test runner remain unchanged and outside this conversion scope.
They still run the existing story lifecycle and regression suite; their conversion is separate work.
