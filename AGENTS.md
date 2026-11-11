# Story-time development

## Find the story before editing

Use this repository's installed native `story` executable for `push`.
Build it with `cargo install --path native/story-push --locked`.
Other commands remain in the Node CLI; set `STORY_TIME_LEGACY_CLI` to its
absolute `skills/story-time/scripts/story.mjs` path for native dispatch.
Direct `node skills/story-time/scripts/story.mjs <command>` remains valid
for non-push commands only.

1. Read `.story-time/settings.json` with `story config --json`.
2. Read the requested story with `story show ST-###`. Otherwise, search `story list --json` for the requested outcome.
3. If no story covers the work, fetch remote branches, then use story-create before implementation.
4. Refine the story, run story-ready, and write the kickoff test plan before code changes. Work only within its criteria.
5. Declare `Stories: ST-### (#issue)` in the PR body. Run story-accept with evidence before closing the story.

These steps apply to subagents. Keep local stories in `backlog/` and mirror them to the configured GitHub repository.
Sync is `ask`: publish changes after the applicable approval step, not automatically.

## Delivery

New and replacement executable implementations and tests use Rust.
Run the existing Node tests and the Rust mandate tests before delivery.
PRs must pass `story-link` and `story-ids`.
An `incident` label exempts a PR without references from story-link; file its bug story within one business day.
For check behavior, commands, and branch protection setup, read `docs/dogfooding.md`.

The initial setup story bootstraps this mandate. Its initial draft precedes enforcement; retain that history.
