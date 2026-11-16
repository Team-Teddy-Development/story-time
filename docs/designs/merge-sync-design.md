# Merge Sync: Keep Story Issues in Step with Merged Pull Requests

*   **Author:** Eric Jernigan
*   **Status:** Draft
*   **Date:** 2026-11-16
*   **Stories:** ST-151 (#344), ST-152 (#346)
*   **Predecessor:** `docs/designs/merge-sync.md` (EGM brainstorm brief, with its correction block)

--------------------------------------------------------------------------------

## Overview

story-time keeps each story as a card file (`backlog/ST-###.md`) and mirrors it
to a tracker issue with `story push`. A pull request declares its stories on a
`Stories: ST-### (#issue)` line, and the `story-link` check
(`.github/workflows/story-link.yml` → `scripts/story-policy.rs story-link`)
verifies that each named story or issue exists.

This design adds two checks around the merge, one on each side of it:

1.  **Before merge (ST-152):** `story-link` also fails when a named story is not
    `done` on the PR's head branch, unless the reference is marked `(partial)`.
    This moves the `story-accept` step into the PR, where the author can still
    act on it.
2.  **After merge (ST-151):** a new `story-sync` workflow on the default branch
    runs `story push` for the cards the merge changed. It then posts one comment
    on each named issue. The comment names the PR and the merge commit. The
    workflow mirrors card state to the tracker. It never changes a card.

The backlog card stays the single source of truth. `story push` stays the only
thing that opens or closes a story issue. `story-accept` stays the only way to
set a story to `done`.

## The Problem

Story issues close only when an agent runs `story push` after `story-accept`.
When that step is skipped, the issue stays open, or it drifts from its card.
The ST-151 source case, in `Varsity-Engine/varsity-mcp`:

*   PR #80 merged with no `Stories:` line.
*   Issue #79 had no ID title prefix and no labels until a later groom
    republished it.

There are two separate failures, and one fix does not cover both:

*   **Acceptance is skipped.** The PR merges while its story is still
    `in-progress`. No later automation can fix this without inventing
    acceptance. A ticked box is not acceptance: `story-accept` also checks the
    Definition of Done, CI, review threads, and evidence per criterion.
*   **Publishing is skipped.** The card on the default branch says `done`, but
    nobody ran `story push`, so the issue is still open. The same happens for
    title, label, and body drift. Automation can fix this one safely, because
    it only copies a state that a reviewed merge already set.

Three facts in the current code constrain the fix:

*   `story push` sets the issue state from the card. It closes the issue when
    the card is `done`, `split`, or `archived`, and reopens it otherwise
    (`native/story-push/src/gateway.rs:400`, `:511`, `github_state` at `:912`).
    A GitHub closing keyword (`Closes #12`) therefore conflicts with it: the
    keyword closes the issue on merge, and the next push reopens it.
*   `story push` writes back to the card on every create and update. It sets
    `remote`, `remote_url`, and `updated`, and it appends a changelog line
    (`native/story-push/src/story.rs:144-185`). A CI run of `story push`
    therefore leaves uncommitted card changes.
*   The CLI tracker gateway has no comment operation. The hosted MCP server
    reads and replies to comments through its own GitHub App
    (`native/story-push/src/mcp.rs`, `story_comments`), but `story push` does not.

## Goals

*   A PR that names a story that is not `done` and not marked `(partial)` fails
    `story-link` before merge, and the failure names the story and its status.
    (ST-152)
*   After a merge to the default branch, every issue named on the PR's
    `Stories:` line has exactly one comment that names the PR and the merge
    commit, and a rerun adds no second comment. (ST-151 AC1)
*   After that run, each named issue's open or closed state, title, labels, and
    body match what `story push` writes for the card on the default branch.
    (ST-151 AC2, AC3)
*   No automation changes any card's `status`, and the workflow checkout is
    clean when the run ends. It pushes no commit. (ST-151 AC4, AC7)
*   A repository with `sync: off`, a files-only tracker, a push with no PR, or
    a PR with no `Stories:` line gets no tracker writes and a "skipped" report
    that includes the commit SHA. (ST-151 AC5, AC6)

## Non-Goals

*   Setting any story to `done` from CI. Only `story-accept` does that.
*   GitHub or Gitea closing keywords (`Closes #N`) as a way to close story
    issues.
*   Trackers other than GitHub in the first slice. The logic lives in the Rust
    CLI so that a later Gitea or ADO wrapper reuses it, but no wrapper ships now.
*   Comments or notifications to dependent stories that become unblocked.
    `story rank` already derives unblocked work from `depends_on`.
*   Installing the workflows into consumer repositories. They stay playbook
    templates that each repository copies, as `story-link.yml` and
    `story-ids.yml` do today.
*   A `released` or deployed state after `done`.

## Decisions

The author delegated each open decision to TypeSafe Jev (typed choice). Each
line records the choice and Jev's confidence.

| # | Question | Decision | Confidence |
| --- | --- | --- | --- |
| Q1 | How does a CI push stay clean? | New `story push --no-write-back` flag. In that mode a card with no `remote` is skipped and reported, never created. | author choice |
| Q2 | Where does post-merge logic live? | A new CLI verb `story sync-merge` in `native/story-push`, with a comment operation on the gateway. | 0.80 |
| Q3 | Closing keywords | ST-152's check fails a PR body with `close`/`fix`/`resolve` keywords that point at a story issue. | 0.43 (weak) |
| Q4 | Roll-out of ST-152 | Mode `warn` or `fail`, set by the workflow variable `STORY_ACCEPTANCE`. The template ships `warn`. | 0.86 |
| Q5 | Deploy-time done | Out of scope. Done means accepted at merge. Teams that need deployment use `(partial)`. | 0.82 |
| R1 | What does ST-152 require? | Card `status: done` **and** every acceptance criterion box ticked. | 0.98 |
| R2 | Post-merge trigger | `push` to the default branch. The job then looks up the PR from the commit. | 1.00 |
| R3 | Evidence grammar per criterion | Out of scope. `story-accept` and PR review own the evidence. | 0.76 |

R2 note: Jev first chose `pull_request: closed` (0.60, confidence 0.20). With
the added fact that a `pull_request` job from a fork gets a read-only token
and cannot write issues, Jev chose `push` (1.00).

## Open Questions

*   **Q6 — Parser drift.** `story-link` keeps its own `references()` in
    `scripts/story-policy.rs`, and `story sync-merge` gets a copy in the crate.
    This design shares one fixture file between both test suites to catch
    drift. A single binary for both checks would remove the copy, but then
    `story-link` must install the `story` binary. Left for a later story.

## Critical User Journeys (CUJs)

### CUJ 1: An agent opens a PR before acceptance

*   **Given**: PR body `Stories: ST-151 (#344)`. `backlog/ST-151.md` on the
    head branch has `status: in-progress` and two unticked criteria.
    `STORY_ACCEPTANCE=fail`.
*   **When**: The `story-link` check runs.
*   **Then**: The check fails. It prints
    `ST-151 is in-progress with 2 unticked criteria; run story-accept or mark the reference (partial)`.
    With `STORY_ACCEPTANCE=warn`, it prints the same text as a warning and passes.

### CUJ 2: A multi-PR story

*   **Given**: PR body `Stories: ST-152 (#346) (partial)`, and the card is
    `in-progress`.
*   **When**: `story-link` runs, then the PR merges and `story-sync` runs.
*   **Then**: `story-link` passes. `story-sync` comments on #346 and leaves it
    open, because the card is not done.

### CUJ 3: Accepted story merges

*   **Given**: The PR set `backlog/ST-151.md` to `done` through `story-accept`,
    and issue #344 is open.
*   **When**: The squash merge pushes commit `abc123` to `main`, and
    `story-sync` runs `story sync-merge --sha abc123 --before <prev>`.
*   **Then**: #344 is closed and has one comment
    `Merged in #345 (abc123)` with a hidden marker. `git status --porcelain` is
    empty. A rerun posts no second comment and makes no change.

### CUJ 4: Direct push, or no Stories line

*   **Given**: A commit is pushed to `main` with no associated PR, or its PR body
    has no `Stories:` line.
*   **When**: `story-sync` runs.
*   **Then**: It writes nothing to the tracker. It prints
    `skipped: no pull request for abc123` (or `no Stories line in #N`) and
    exits 0.

### CUJ 5: Closing keyword

*   **Given**: PR body `Stories: ST-151 (#344)` and the text `Closes #344`.
*   **When**: `story-link` runs.
*   **Then**: It fails with
    `#344 is a story issue; remove the closing keyword, story push closes it after acceptance`.
    `warn` mode only warns.

## Technical Plan (Architecture)

### Components

```text
           PR open/edit/sync                         push to main
                  |                                        |
      .github/workflows/story-link.yml          .github/workflows/story-sync.yml
                  |                                        |
      scripts/story-policy.rs story-link        story sync-merge --sha S --before B
        - references() (+ partial flag)            (native/story-push, new verb)
        - read head-branch cards                   1. load .story-time/settings.json
        - status + tick check   (ST-152)           2. gateway.pull_for_commit(S)
        - closing-keyword check (ST-152)           3. stories_line::parse(PR body)
        - STORY_ACCEPTANCE warn|fail               4. changed cards: git diff B..S -- backlog/
                  |                                5. push plan, write_back = false
              pass / fail                          6. gateway.comment_once(issue, marker)
                                                   7. JSON report, exit 0 (or 1 on error)
                                                           |
                                                   GitHub issues (via gh)
```

### Pre-merge check (ST-152)

`scripts/story-policy.rs` already parses `Story:`/`Stories:` lines and checks
that each ID has a card. It gains three rules, in this order:

1.  **Partial marker.** A reference followed by `(partial)` on the same
    declaration line is exempt from the acceptance rule. The marker binds to
    the nearest ID or `#N` before it.
2.  **Acceptance.** For each non-partial `ST-###`, the check reads
    `backlog/ST-###.md` from the PR checkout (the head branch, through
    `actions/checkout`). It reads `status:` from the frontmatter and counts
    `- [ ]` lines under `## Acceptance Criteria`. The rule passes only for
    `status: done` with zero unticked boxes. A bare `#N` with no ID is not
    checked, because there is no card to read.
3.  **Closing keywords.** The check scans the whole body for
    `close|closes|closed|fix|fixes|fixed|resolve|resolves|resolved` followed by
    `#N`, case-insensitive. It fails when `#N` is the `remote` of a named card,
    or when it is an issue that carries the story label (the same label lookup
    `validate_links` uses).

`STORY_ACCEPTANCE` selects the outcome of rules 2 and 3. `fail` exits 1. `warn`
or unset prints `::warning::` lines and exits 0. The existing link rules stay
hard failures in both modes. An `incident` label exempts rules 2 and 3, as it
already exempts a missing reference.

### Post-merge sync (ST-151)

**Trigger and token.** `story-sync.yml` runs on `push` to the default branch,
with `permissions: contents: read, issues: write, pull-requests: read`. The
job uses `GITHUB_TOKEN`, which the CLI already prefers through `GH_TOKEN`. A
`push` job always runs default-branch code with the repository token. Fork PRs
are covered, because the merged commit is on the base repository.

**`story sync-merge`.** A new verb in `native/story-push`, GitHub-only in v1:

1.  Load the config. If `sync` is `off`, or `tracker` is not a supported
    tracker, return `{"skipped": "<reason>"}` and exit 0 (AC5). Gitea and ADO
    return `skipped: tracker gitea not supported by sync-merge yet`.
2.  Find the PR: `GET /repos/{repo}/commits/{sha}/pulls`, keeping only PRs whose
    `merged_at` is set. No PR → skip and report (AC6).
3.  Parse the PR body with `stories_line::parse`. No references → skip and
    report (AC6).
4.  Build the push set: the cards that `git diff --name-only B..S -- <backlogDir>`
    changed, plus every named ID that has a card file. When `B` is all zeros
    (new branch), use `S^1`.
5.  Run the existing push planner and executor with `write_back = false`
    (Q1). The issue state follows the card through the existing
    `github_state` close or reopen (AC2). The title, labels, and body follow
    the card through the existing `update` (AC3). A card with no `remote` is
    skipped with reason `unlinked`.
6.  For each named issue (`(#N)` on the line, or the named card's `remote`),
    call `comment_once(issue, marker, body)`. The marker is
    `<!-- story-time:merge-sync sha=<S> -->`. The method lists the issue
    comments and posts only when no comment contains the marker (AC1).
7.  Print a JSON report: `pr`, `sha`, `pushed[]`, `skipped[{id, reason}]`,
    `commented[]`, `existing[]`. Any tracker error exits 1 and names the step.

The verb never writes a card (AC4, AC7). It never sets a status: it calls
only the planner and executor, and with `write_back = false` the executor
performs no `atomic_write`.

**`story push --no-write-back`.** `execute()` in `lib.rs` today calls
`story::link` and `atomic_write` after each create or update. With the flag:

*   the planner turns every `create` into `skip` (reason `unlinked`);
*   the executor runs `update` and the state sync, but skips `story::link` and
    `atomic_write`;
*   the push lock is still taken, so a parallel local write cannot interleave.

### Data shapes

```text
stories_line::Reference { id: Option<String>, issue: Option<u64>, partial: bool }
stories_line::parse(body: &str) -> Vec<Reference>
stories_line::closing_targets(body: &str) -> Vec<u64>

Gateway::pull_for_commit(&self, sha: &str) -> Result<Option<Pull { number: u64, body: String }>, String>
Gateway::comment_once(&mut self, issue: &str, marker: &str, body: &str) -> Result<Commented, String>
enum Commented { Posted(url), Existing }
```

## Alternatives Considered

### Alternative A: Auto-accept on merge when every box is ticked (brainstorm hypothesis)

*   **Pros**: Closes stories even when the agent forgot `story-accept`.
*   **Cons**: A ticked box is not acceptance. It skips the Definition of Done,
    CI, review threads, and evidence checks. It needs a follow-up bot PR to
    record `done`, which brings write tokens, loop guards, and PRs whose
    checks do not run under `GITHUB_TOKEN`.
*   **Reason for Rejection**: Rejected by author: it creates a second, weaker
    path to `done`. ST-151 AC4 forbids status changes.

### Alternative B: GitHub closing keywords (`Closes #N`)

*   **Pros**: No new code.
*   **Cons**: Closes the issue at merge, before acceptance. The next
    `story push` reopens it, because the card is not done
    (`gateway.rs:912`).
*   **Reason for Rejection**: Rejected by author: two closers that disagree.
    The design bans the keywords for story issues (Q3).

### Alternative C: Post-merge job on `pull_request: closed`

*   **Pros**: The PR body is in the event, so no API lookup is needed.
*   **Cons**: A fork PR gets a read-only token and no secrets, so the job
    cannot write issues. `pull_request_target` gets a write token but mixes
    base code with PR-supplied data. A direct push is not seen.
*   **Reason for Rejection**: Rejected by author (through Jev, R2): the
    `push` trigger covers forks and direct pushes.

### Alternative D: Plain `story push` in CI, then discard card changes

*   **Pros**: No CLI change. Unlinked cards get issues.
*   **Cons**: The new issue number is lost. If the card is done, the issue is
    closed in the same run, and the next local push misses it in the
    open-issue title lookup and creates a duplicate. The cleanup is YAML with
    no tests.
*   **Reason for Rejection**: Rejected by author (Q1).

### Alternative E: Logic in workflow shell (`gh` + `jq`), or in `story-policy.rs`

*   **Pros**: Less Rust, or reuse of the existing parser.
*   **Cons**: Shell holds untested business logic (Article VIII). A
    `story-policy.rs` subcommand has no gateway, so a Gitea wrapper means a
    rewrite.
*   **Reason for Rejection**: Rejected by author (through Jev, Q2).

### Alternative F: Read-only sweep (`story merged`) that only comments on gaps

*   **Pros**: No tracker state changes. Low risk.
*   **Cons**: Leaves done stories with open issues, which is the ST-151 problem.
*   **Reason for Rejection**: Rejected by author: it does not meet ST-151
    AC2 or AC3.

--------------------------------------------------------------------------------

## Non-Functional Requirements

*   **Security.** `story-sync` runs only on `push` to the default branch, with
    `issues: write` and no `contents: write`. The PR body reaches the CLI as
    API data, never through `${{ }}` in a `run:` line. `story-link` already
    passes the body through `env`. Consumer repositories install the CLI with
    a pinned `--rev`, so a parser change cannot start mass edits by surprise.
*   **Idempotency.** The comment marker includes the merge SHA. Close, reopen,
    and update are already idempotent in `story push`. A rerun of a run
    changes nothing.
*   **Cost.** One run makes one PR lookup and one diff. Per named story, it
    makes one push update and one comment list plus at most one post. Each run
    builds the CLI with `cargo install --locked`, which costs about 2–4 minutes
    of runner time without a cache.
*   **Roll-out and rollback.** ST-152 ships in `warn` mode. Switching to `fail`
    is a one-line workflow change. Rolling back `story-sync` means deleting
    the workflow file. It writes no card and no commit, so it leaves nothing
    to undo in git.

--------------------------------------------------------------------------------

### Additional Resources

*   `docs/designs/merge-sync.md`: the EGM brainstorm brief and its correction block.
*   `docs/playbook.md` §3: the enforcement templates.
*   `docs/native-story-push.md`: the CLI build and install.
*   `Varsity-Engine/varsity-mcp` #79 and #80: the source incident.

### Future Improvements

*   **Skill text.** `story-accept` says "when a story's PR merges". It should
    say "before requesting merge". This change is outside the ST-151 and ST-152
    criteria, so it needs a follow-up `chore` story.
*   **Gitea and ADO wrappers** for `story sync-merge`, reusing the verb.
*   **One parser.** Have `story-link` call the `story` binary, and delete the
    copy in `story-policy.rs` (Q6).
*   **Unblock notices.** Report dependents that become workable.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

### Target Files to Create/Modify

#### 1. `native/story-push/src/stories_line.rs` (Create)

*   **Type / Scope**: Rust domain module (ST-151, ST-152).
*   **Proposed Changes**: Add `Reference { id, issue, partial }`,
    `parse(body) -> Vec<Reference>`, and `closing_targets(body) -> Vec<u64>`.
    The grammar is the one in `scripts/story-policy.rs::references()`: a line
    whose label before `:` is `story` or `stories` (case-insensitive), with
    words split on non-`[A-Za-z0-9#_-]`. It adds `(partial)` after a reference.
    Unit tests read `tests/fixtures/stories-lines.tsv`.

#### 2. `native/story-push/src/sync_merge.rs` (Create)

*   **Type / Scope**: Rust CLI verb (ST-151).
*   **Proposed Changes**: `pub fn run(args) -> Result<(String, i32), Failure>`.
    The flags are `--sha` (required), `--before`, `--json`, `--config`, and
    `--dir`. It runs steps 1–7 of "Post-merge sync". It takes a `MergeTracker`
    trait (`pull_for_commit`, `comment_once`, plus the existing `Tracker`) so
    that tests use a fake. It calls `git diff --name-only` through
    `std::process::Command`.

#### 3. `native/story-push/src/lib.rs` (Modify)

*   **Type / Scope**: Rust CLI core (ST-151).
*   **Proposed Changes**:
    *   Add `"no-write-back"` to `PUSH_FLAGS`.
    *   Give `plans()` a `write_back: bool`. When it is false, a `create`
        becomes `skip` with reason `unlinked`.
    *   Give `execute()` a `write_back: bool`. When it is false, skip
        `story::link` and `atomic_write`, and report each update as pushed.
    *   Register `("sync-merge", sync_merge::run)` in the command table, and
        add `mod stories_line; mod sync_merge;`.
    *   Keep `--dry-run` working with `--no-write-back`.

#### 4. `native/story-push/src/gateway.rs` (Modify)

*   **Type / Scope**: Rust tracker adapter (ST-151).
*   **Proposed Changes**:
    *   Add `pull_for_commit(sha)`. It runs `gh api repos/{repo}/commits/{sha}/pulls`
        and keeps the PR with `merged_at` set.
    *   Add `comment_once(issue, marker, body)`. It runs
        `gh api repos/{repo}/issues/{n}/comments --paginate`, searches for the
        marker, and posts with `gh issue comment --body-file -` (stdin) only
        when the marker is absent.
    *   The Gitea and ADO branches return
        `Err("sync-merge supports github only")`.
    *   Unit tests use the existing fake-`gh` harness (`gateway.rs:1690`).

#### 5. `native/story-push/src/help.rs` (Modify)

*   **Type / Scope**: CLI help text.
*   **Proposed Changes**: Document `story sync-merge` and
    `story push --no-write-back`. `tests/help_contract.rs` and
    `tests/help_lists_native.rs` must list the new verb.

#### 6. `native/story-push/tests/sync_merge_contract.rs` (Create)

*   **Type / Scope**: Rust contract tests (ST-151 AC1–AC7).
*   **Proposed Changes**: One test per criterion, against a fake `gh` and a
    temp git repository:
    *   One comment per issue, and a rerun posts none (AC1).
    *   A done card closes an open issue, and a not-done card reopens a closed
        one (AC2).
    *   Drift in the title, labels, or body is fixed (AC3).
    *   No card file changes. Assert the bytes of every card before and after
        (AC4, AC7).
    *   `sync: off` and `tracker: none` report skipped with zero `gh` calls
        (AC5).
    *   No PR, and no `Stories:` line, report skipped with the SHA (AC6).
    *   `git status --porcelain` is empty after the run (AC7).

#### 7. `native/story-push/tests/push_flag_contract.rs` (Modify)

*   **Type / Scope**: Rust contract test (ST-151).
*   **Proposed Changes**: Check that `--no-write-back` leaves the card bytes
    unchanged after an update. Check that it skips an unlinked card with
    reason `unlinked` and makes zero create calls.

#### 8. `scripts/story-policy.rs` (Modify)

*   **Type / Scope**: Rust CI checker (ST-152).
*   **Proposed Changes**:
    *   Extend `references()` to record `(partial)`.
    *   Add `validate_acceptance(body, incident, read_card, issue_labels) -> Vec<Finding>`,
        which applies rules 2 and 3 of "Pre-merge check". A `Finding` has `id`
        and `message`.
    *   In `link()`, read `STORY_ACCEPTANCE`. With `fail`, any finding exits 1.
        With `warn` or unset, print `::warning::` and exit 0.
    *   Add tests in the file's `#[cfg(test)]` module for: done and ticked
        passes; not done fails; done with an unticked box fails; partial
        passes; incident passes; a closing keyword for a story issue fails; a
        closing keyword for a non-story issue passes; warn mode exits 0.
    *   The tests also read `tests/fixtures/stories-lines.tsv`.

#### 9. `tests/fixtures/stories-lines.tsv` (Create)

*   **Type / Scope**: Shared test data (Q6).
*   **Proposed Changes**: Rows of `body<TAB>expected references`. Both parsers
    must agree on every row: valid, malformed, missing, unknown ID, `#N` only,
    `(partial)`, and lowercase `stories:`.

#### 10. `.github/workflows/story-link.yml` (Modify)

*   **Type / Scope**: CI configuration (ST-152).
*   **Proposed Changes**: Add `STORY_ACCEPTANCE: warn` to the
    `Validate declared stories` step. Its `actions/checkout` already checks out
    the PR merge ref, so the card files are present.

#### 11. `.github/workflows/story-sync.yml` (Create)

*   **Type / Scope**: CI configuration (ST-151).
*   **Proposed Changes**:
    *   Trigger on `push` to `main`, with `concurrency: story-sync` and
        `cancel-in-progress: false`.
    *   Set the job permissions to `contents: read`, `issues: write`, and
        `pull-requests: read`.
    *   Run `actions/checkout` with `fetch-depth: 0`, then
        `dtolnay/rust-toolchain@stable`.
    *   Install with `cargo install --path native/story-push --locked` in this
        repository. The playbook copy uses
        `--git https://github.com/Team-Teddy-Development/story-time --rev <sha>`.
    *   Run `story sync-merge --sha "$GITHUB_SHA" --before "$BEFORE" --json`,
        with `GH_TOKEN: ${{ github.token }}` and
        `BEFORE: ${{ github.event.before }}`.
    *   End with `git status --porcelain`, which must print nothing.

#### 12. `docs/playbook.md` (Modify)

*   **Type / Scope**: Documentation.
*   **Proposed Changes**:
    *   In §3, add the `story-sync.yml` template, the `(partial)` marker, the
        `STORY_ACCEPTANCE` warn-to-fail roll-out, and the closing-keyword ban.
    *   In §2, change rule 5 ("Close the loop") to state that the PR carries the
        `done` card and that `story-sync` closes the issue after merge.

#### 13. `backlog/ST-151.md` and `backlog/ST-152.md` (Modify)

*   **Type / Scope**: Story cards.
*   **Proposed Changes**:
    *   ST-151: name `story sync-merge` and `--no-write-back` in the Notes.
        Record that a card with no `remote` is skipped in CI (Q1).
    *   ST-152: change AC1 to "status done and every criterion ticked" (R1).
        Add one AC for closing keywords (Q3) and one AC for the warn and fail
        mode (Q4).
    *   Publish both with `story push` after the edit.

### Verification Plan

*   **Compilation/Build**: `cargo build --manifest-path native/story-push/Cargo.toml --release --locked`
*   **Unit Tests**: `cargo test --manifest-path native/story-push/Cargo.toml --locked`;
    `rustc --edition=2021 --test scripts/story-policy.rs -o "$T/policy" && "$T/policy"`
*   **Static Analysis & Linting**: `cargo fmt --manifest-path native/story-push/Cargo.toml --check`;
    `cargo clippy --manifest-path native/story-push/Cargo.toml --locked --all-targets -- -D warnings`;
    `story lint ST-151 ST-152`
*   **Test Coverage Requirement**: One named test per acceptance criterion of
    ST-151 (7) and ST-152 (6 after the edit). A criterion with no test is
    unmet.
*   **E2E/Integration**:
    *   Merge one PR in story-time that names a `done` story. Confirm that its
        issue closes and has one `Merged in #N (sha)` comment.
    *   Rerun the workflow from the Actions tab. Confirm that it posts no
        second comment.
    *   Open a PR that names an `in-progress` story. Confirm that `story-link`
        warns in `warn` mode.

--------------------------------------------------------------------------------

*Drafted with EGM*
