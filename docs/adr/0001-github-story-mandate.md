# GitHub story mandate

Status: Accepted

## Context

Story-time distributes a story lifecycle but has no repository-specific tracker configuration or PR story checks.
Maintainers need to use that lifecycle for their own feature work.

## Decision

Keep story files in `backlog/` and mirror them to GitHub Issues using the existing story CLI.
Use `sync: ask` so publishing stays explicit.
Require declaration lines in PR bodies and unique new story IDs across the base branch and older open PRs.
Implement the new checks and their tests in Rust.
Use read-only GitHub Actions permissions and preserve the existing owner-review and test requirements.
Record the bootstrap as ST-001; this initial setup introduces the mandate it follows thereafter.

## Consequences

A feature has a local story, a linked GitHub issue, and a PR with a `Stories:` line.
Missing references, invalid issue labels, API failures, and duplicate new IDs fail CI.
An incident can omit its story reference, but its bug story is due within one business day.
The checks validate links and filename uniqueness; story readiness and acceptance remain lifecycle steps.
Existing Node implementations remain outside this scoped Rust addition.
Removing the dedicated story-check ruleset and reverting the configuration PR restores the prior policy.
