# Merge Sync: Keep Story Issues in Step with Merged Pull Requests

*   **Author:** Eric Jernigan
*   **Status:** In Review
*   **Revision:** 3.4 (after EGM review Stage 9 re-audit)
*   **Date:** 2026-11-16
*   **Stories:** ST-151 (#344), ST-152 (#346). Prerequisites: ST-153 (#348),
    ST-154 (#350).
*   **Predecessor:** `docs/designs/merge-sync.md`, the brainstorm brief.

--------------------------------------------------------------------------------

## Overview

story-time keeps each story as a card file (`backlog/ST-###.md`). The `story`
CLI mirrors each card to a tracker issue with `story push`. A pull request
declares its stories on a line such as `Stories: ST-151 (#344)`. The
`story-link` check verifies that each named story has a card, and that each
named `#N` is an issue with a story-type label, not a pull request.

The completion step is the `story-accept` agent skill. An agent runs it,
checks each acceptance criterion against evidence and the Definition of Done
(the `definitionOfDone` list in `.story-time/settings.json`), ticks the
criteria boxes, and sets the card's `status` to `done`. Then `story push`
closes the issue. The skill's description says to run it "when a story's PR
merges". The card edit then needs a second commit after the merge. In the
source case below, `story push` was not run after the merge.

This design adds a check before the merge and a sync after it:

1.  **Check before merge (ST-152).** A new read-only CLI verb,
    `story check-pr`, replaces the body of every `story-link` workflow. It
    keeps today's link rules and adds new rules that run in `warn` or `fail`
    mode. The main one is the acceptance rule: each story named on
    the PR must be `done` with every criterion ticked, unless the reference is
    marked `(partial)`. In `fail` mode the PR fails. In `warn` mode, the
    default, the check only warns. To pass the rule, the agent runs
    `story-accept` on the PR branch and commits the `done` card in the PR. ST-153 changes the
    skill text to say so.
2.  **Sync after merge (ST-151).** A new `story-sync` workflow runs
    `story sync-merge` after each push to the default branch. The verb
    updates the issue of each story that changed since the last successful
    sync from the card on the default branch. It closes or reopens the issue
    and fixes its title, labels, and body. It posts one comment on each issue
    named by each merged PR in that range. It never writes a card file and
    never changes a status.

The card stays the single source of truth for story state. Within
story-time, one tracker gateway opens and closes story issues; `story push`
and `story sync-merge` both use it. GitHub also closes an issue when a merged
PR body has a closing keyword (`Closes #N`). In `fail` mode, `check-pr`
rejects such a keyword unless the card already says `done`. In `warn` mode
it warns, and `story-sync` reopens the issue. `story-accept` stays the only way to set
`done`.

## The Problem

Within story-time, story issues close only when an agent runs `story push`
after `story-accept`.
When that step is skipped, the issue stays open, or its title and labels drift
from the card. The ST-151 source case is in `Varsity-Engine/varsity-mcp`, a
repository that uses story-time:

*   PR #80 merged with no `Stories:` line.
*   Issue #79 had no ID title prefix and no labels. They appeared only after a
    later backlog grooming pass (a review that re-publishes cards) ran
    `story push`.

There are two separate failures:

*   **Acceptance is skipped.** The PR merges while its story is still
    `in-progress`. No automation can fix this after the fact without inventing
    acceptance. A ticked box alone is not acceptance.
*   **Publishing is skipped.** The card on the default branch says `done`, but
    nobody ran `story push`, so the issue is still open or out of date.
    Automation can fix this, because it only copies a state that is already on
    the default branch.

Five facts in the current code constrain the fix:

*   `story push` sets the issue state from the card. It closes the issue when
    the card is `done`, `split`, or `archived`, and reopens it otherwise
    (`native/story-push/src/gateway.rs:400`, `:511`, `github_state` at `:912`).
*   `story push` writes back to the card on every create and update. It sets
    `remote`, `remote_url`, and `updated`, and appends a changelog line
    (`native/story-push/src/story.rs:144-185`). Running it as-is in CI leaves
    uncommitted card changes.
*   The CLI gateway has no comment operation. story-time's hosted MCP server
    (the remote service agents use to edit stories) comments through its own
    GitHub App (`native/story-push/src/mcp/comments.rs`).
*   The playbook's `story-link` template (`docs/playbook.md:147-190`) is
    bash, and consumer repositories copy it. Only this repository runs the Rust checker
    `scripts/story-policy.rs`. A rule added only to that checker never
    reaches the repositories where the source incident happened.
*   Each `story push` process runs `gh label create <name> --force` with no
    `--color` for each label it applies (`gateway.rs:886-901`). `gh` picks a
    random color when none is given, and `--force` overwrites the existing
    color. Every fresh process therefore recolors the labels it touches. A CI
    job runs a fresh process on every merge. ST-154 fixes this before
    `story-sync` ships.

## Terms and Settings

*   **Card fields.** `status` is the story state. `remote` is the linked issue
    number. `pending_create` is a marker that `story push` writes before an
    issue create that it has not yet confirmed (`native/story-push/src/lib.rs:316-340`).
*   **Reference.** One story on a `Stories:` line, in the form
    `<ID> [(#N)] [(partial)]` or `#N [(partial)]`. `<ID>` uses the
    repository's prefix, for example `ST-151`. A reference is one unit:
    `(partial)` applies to the whole unit, whatever comes between it and the
    ID.
*   **`.story-time/settings.json` keys:**
    *   `prefix`: the story ID prefix (default `ST`). Both verbs read it.
    *   `backlogDir`: the card folder (default `backlog`). Both verbs read it.
    *   `tracker`: `files` (no tracker; cards only), `github`, `gitea`, `ado`,
        `jira`, or `linear`. "Unsupported" below means any value other than
        `github`.
    *   `sync`: `off`, `ask`, or `auto`. In CI, `ask` and `auto` both allow
        `story sync-merge` to write: adding the workflow is the consent, and
        no one is there to answer a question. `off` makes the verb skip.
*   **`STORY_ACCEPTANCE`.** A GitHub repository variable
    (`vars.STORY_ACCEPTANCE`), `warn` or `fail`. When it is unset, the check
    uses `warn`. A pull request cannot change a repository variable. It can
    change its own workflow file or `.story-time/settings.json`, so the mode
    must not live there.

## Goals

*   In `fail` mode, a PR that names a story that is not `done` with every
    criterion ticked, and not marked `(partial)`, fails `story-link`. The
    failure names the story, its status, and its unticked count. A
    `(partial)` reference and an `incident` PR pass. A closing keyword aimed
    at a story issue that no passing reference covers is a finding. `warn`
    mode warns and passes. (ST-152 AC1–AC6. AC1 and AC2 read the merge ref;
    AC5 is amended by D7 and D15; AC6's mode comes from `vars.STORY_ACCEPTANCE` (see Terms and Settings).
    D-numbers refer to Decisions.)
*   The same rule runs in every repository that adopts the new template,
    from one tested Rust implementation. (ST-152)
*   After a successful `story-sync` run, every issue named by a merged PR in
    the synced range has exactly one comment for that PR's merge commit. A
    rerun adds no second comment. (ST-151 AC1, as amended by D13)
*   After that run, each issue whose card changed in the synced range has the
    open or closed state, title, labels, and body that `story push` would
    write for the card on the default branch. (ST-151 AC2, AC3)
*   Neither `check-pr` nor `sync-merge` writes a card file, and the workflow
    checkout is clean at the end. The workflow pushes no commit. (ST-151 AC4, AC7)
*   With `sync: off` or an unsupported tracker, the run writes nothing to the
    tracker. A push with no PR, or a PR with no `Stories:` line, gets no
    merge comment. Its changed cards are still mirrored, and a status change
    gets a state comment. A skipped run, and a
    push with no PR or no `Stories:` line, name the commit SHA. (ST-151 AC5, AC6 as amended by D16)
*   **Success measures.** The story-time maintainer computes both with
    `gh` queries over the last 30 days of merged PRs and records them in the
    ST-151 and ST-152 acceptance evidence.
    *   Publishing: merged PRs whose named story is `done` on the default
        branch while its issue is still open. The target is 0 after
        `story-sync` ships.
    *   Acceptance: merged references, not marked `(partial)`, whose card was
        not `done` at the merge commit. The target is 0 after the switch to
        `fail`.

## Non-Goals

*   Setting any story to `done` from CI. Only `story-accept` does that.
*   Trackers other than GitHub in this release. The logic is in the Rust CLI,
    so a later Gitea or ADO wrapper can reuse it, but no wrapper ships now.
*   Closing keywords in commit messages. GitHub acts on them when a commit
    lands on the default branch, but `check-pr` reads only the PR body.
    `story-sync` reopens such an issue if the card's change is in its range,
    and `--all` reopens it in any case.
*   Comments to dependent stories that become unblocked.
*   Installing workflows into consumer repositories automatically.
*   A `released` or deployed state after `done`. A team whose Definition of
    Done requires deployment marks its references `(partial)`.
*   A public `story push` flag that skips write-back.
*   A machine-readable evidence format per criterion. `story-accept` and PR
    review own the evidence.

## Decisions

The author delegated each open decision to TypeSafe Jev, a decision service
that picks one option from a fixed list. The Confidence column is Jev's
certainty in its answer, from 0 to 1. Below 0.5, the call is marked weak, and
reviewers should challenge those first. The author adopted every call in this table for this
revision.

| # | Question | Decision | Confidence |
| --- | --- | --- | --- |
| D1 | Where does the pre-merge rule live? | A read-only CLI verb `story check-pr`, called by every `story-link` template. | 0.96 |
| D2 | Where does post-merge logic live? | A CLI verb `story sync-merge`, with a comment operation on the gateway. | 0.80 |
| D3 | How does CI avoid card writes? | `sync-merge` calls the gateway's `update` directly. That call writes no file. `story push` is unchanged. | 1.00 |
| D4 | What does the acceptance rule require? | Card `status: done` and every acceptance criterion box ticked. | 0.98 |
| D5 | Post-merge trigger | `push` to the default branch. | 1.00 |
| D6 | Which card state is mirrored? | The card on the default-branch HEAD at the start of the run. | 0.80 |
| D7 | Closing keywords | Allowed on a reference that passes the acceptance rule. | 0.88 |
| D8 | Roll-out of the acceptance rule | `warn` first. | 0.86 |
| D9 | Card with no `remote` named on a PR | A `check-pr` finding ("run story push before merge"). Skipped with `sync: off`. | 0.98 |
| D10 | CI install of the `story` binary | `actions/cache` keyed on the pinned source revision; `cargo install` on a miss. | 0.30 (weak) |
| D13 | Ordering of runs | One run at a time (a concurrency group that queues, without cancelling the running one). Each run covers the range from the last successful run to HEAD. | 0.61 |
| D14 | A bare `#N` reference | Mapped to the card whose `remote` is `N`, and then checked like an ID reference. | 0.22 (weak) |
| D15 | A closing keyword on an unnamed story issue | A finding when the target has a story-type label and no passing reference covers it. | 0.56 |
| D16 | Changed cards on a push with no PR or no `Stories:` line | Mirrored, with a state comment on a status change. Only merge comments need a PR and a `Stories:` line. This amends ST-151 AC6. | 0.42 (weak); state comment 0.09 (weak) |
| D18 | Trigger to switch to `fail` | The repository owner switches when the 7-day warn count is 0. | 0.22 (weak) |

D5 note: Jev first chose a `pull_request: closed` trigger with confidence
0.20. It was then given one more fact: a `pull_request` job from a fork gets
a read-only token and cannot write issues. With that fact it chose `push`,
with confidence 1.00.

## Critical User Journeys (CUJs)

The PR and issue numbers below are examples.

### CUJ 1: An agent opens a PR before acceptance

*   **Given**: PR #900, body `Stories: ST-901 (#902)`. On the PR's merge ref,
    `backlog/ST-901.md` has `status: in-progress` and two unticked criteria.
    `vars.STORY_ACCEPTANCE` is `fail`.
*   **When**: `story-link` runs `story check-pr`.
*   **Then**: The check fails with
    `ST-901 is in-progress with 2 unticked criteria; run story-accept or mark the reference (partial)`.
    With `warn`, the same text is a warning, and the check passes.

### CUJ 2: A story delivered over several PRs

*   **Given**: PR #903, body `Stories: ST-904 (#905) (partial)`. The card is
    `in-progress`.
*   **When**: `story-link` runs. Later the PR merges, and `story-sync` runs.
*   **Then**: `story-link` passes, because `(partial)` covers the whole
    reference. `story-sync` comments on #905 and leaves it open.

### CUJ 3: An accepted story merges

*   **Given**: PR #906 set `backlog/ST-907.md` to `done` through
    `story-accept`. Issue #908 is open.
*   **When**: The squash merge pushes commit `abc123`, and `story-sync` runs.
*   **Then**: #908 is closed and has one comment,
    `Merged in #906 (abc123)`, with a hidden marker. `git status --porcelain`
    prints nothing. A rerun posts no second comment and changes nothing.

### CUJ 4: Direct push

*   **Given**: Commit `def456` is pushed to the default branch with no PR. It
    sets `backlog/ST-909.md` to `archived`.
*   **When**: `story-sync` runs.
*   **Then**: ST-909's issue is closed and gets one state comment,
    `Status archived from def456`. No merge comment is posted. The report
    says `no pull request for def456; mirrored 1 card`.

### CUJ 5: Closing keywords

*   **Given**: PR body `Stories: ST-904 (#905) (partial)` and the text
    `Closes #905`. `vars.STORY_ACCEPTANCE` is `fail`.
*   **When**: `story-link` runs.
*   **Then**: It fails with
    `#905 is not covered by a passing reference; remove the closing keyword`.
    The same failure occurs for `Fixes #910`, where #910 is the story issue of
    an unnamed ST-911. `Closes #908` with ST-907 `done` and named passes.

### CUJ 6: A burst of merges

*   **Given**: Three PRs merge within one minute.
*   **When**: The first `story-sync` run starts. The second waits, and the
    third replaces the second in the queue.
*   **Then**: The third run covers the range from the first run's commit to
    HEAD. That range includes the second PR's changes and its merge commit,
    so the second PR's cards are mirrored and its issues get their comment.

## Proposed Architecture

### Components

```text
  PR opened / edited / synced                      push to default branch
             |                                              |
   story-link.yml (template and repo)              story-sync.yml (one at a time)
             |                                              |
   story check-pr  (read-only)                     story sync-merge
     - parse references                              - range = last successful run .. HEAD
     - link rules (hard fail)                        - merged PRs in the range
     - acceptance, unlinked, keyword                 - mirror cards changed in the range
       rules (warn | fail)                           - one marked comment per PR per issue
             |                                       - report; job summary on failure
         pass / fail                                         |
                                                    tracker gateway (GitHub via gh)
             \______ shared: reference parser, card reader, gateway ______/
```

Both verbs live in `native/story-push`, the crate that builds the `story`
binary. They share one reference parser and one card reader.

### Pre-merge check: `story check-pr` (ST-152)

**Input.** The PR body comes from an environment variable, never through
`${{ }}` in a `run:` line. The mode comes from `vars.STORY_ACCEPTANCE`. Card
paths resolve through the `backlogDir` setting (default `backlog`). The cards
come from the PR's merge ref (`refs/pull/N/merge`), which `actions/checkout`
uses for `pull_request` events. If the default branch moves after the event,
the merged tree can differ until the check runs again. A PR with merge
conflicts has no merge ref. Checkout then fails before `check-pr` runs, and
the check stays red until the author resolves the conflicts.

**Rules, in order:**

1.  **Link rules (always a hard failure, as today).**
    *   Each `<ID>` has a card file.
    *   Each `#N` is an issue with a story-type label (`story`, `spike`,
        `bug`, or `chore`), not a PR.
    *   A PR with no reference fails, unless it has the `incident` label.
2.  **Acceptance.** For each reference not marked `(partial)`, the card is
    `status: done`, and every entry that the existing parser
    `story::acceptance_criteria` (`native/story-push/src/story.rs:236`)
    returns is done. That parser also reads `## Exit Criteria` for spikes, so
    a spike is held to its exit criteria. A
    bare `#N` reference uses the card whose `remote` is `N`. If no card has
    that remote, the reference is a finding.
3.  **Link consistency.** For each reference not marked `(partial)`, the card
    has a `remote`. When the reference also has `(#N)`, `N` equals that
    `remote`. This rule is skipped when `sync` is `off`.
4.  **Closing keywords.** GitHub's closing keywords are `close`, `fix`, and
    `resolve` in any tense, followed by an optional colon and then `#N`,
    `owner/repo#N`, or an issue URL. For each one in the body that targets
    this repository, the issue is covered by a reference that passed rules
    2 and 3, or it is not a story-type issue. Otherwise it is a finding.

Rules 2–4 are new, and `STORY_ACCEPTANCE` selects their outcome. `fail` exits
1. `warn` or unset prints one `::warning::` line per finding and exits 0. The
`incident` label exempts rules 2–4. Rule 1 is today's behavior, so it fails in
both modes, and the warn-first roll-out adds no new hard failure.

### Post-merge sync: `story sync-merge` (ST-151)

**Trigger, jobs, and token.** `story-sync.yml` runs on `push` to the default
branch, on a weekly schedule, and on `workflow_dispatch`. All three share one
concurrency group with `cancel-in-progress: false`, so runs happen one at a
time. GitHub keeps at most one waiting run, and a newer run replaces it.

The workflow has two jobs:

*   **Build.** It has only `contents: read`. It restores the cached binary or runs
    `cargo install --locked` at the pinned revision. It passes the binary to
    the next job as an artifact. Third-party build scripts therefore never run
    in a job that holds a write token.
*   **Sync.** It has `contents: read`, `issues: write`, `pull-requests: read`,
    and `actions: read`. It checks out with `fetch-depth: 0` and
    `persist-credentials: false`. It runs the binary with `GH_TOKEN` set on
    that step only.

The tracker repository in settings (`github.repo`) must equal the workflow's
repository, because `GITHUB_TOKEN` can write only there.

**Failures.** The run sorts each failed tracker call into one of two kinds:

*   **Retryable.** A 5xx response, a rate limit, or a network error. The run
    exits 1 and is not "successful", so the next run covers its range again.
*   **Permanent.** A 404, a transferred issue, a reference mismatch, or a
    repository mismatch. The run reports it in the job summary and does not
    fail for it, so the range still advances.

**Steps:**

1.  **Gate.** Read the settings. With `sync: off` or an unsupported tracker,
    report `skipped (<reason>) at <sha>` and exit 0. If `github.repo` is not
    the workflow repository, report a permanent failure and stop.
2.  **Range.** The range starts at the head SHA of the last successful
    push-triggered run of this workflow on the default branch, and ends at
    the current HEAD. It contains the first-parent commits between them.
    Every merge commit and squash commit on the default branch is a
    first-parent commit. Scheduled and dispatched `--all` runs post no
    comments, so they never advance the range. The run takes the `--all`
    path, and reports why, in three cases: there is no earlier success (the
    first run, a renamed workflow, or expired run history); the anchor is
    not an ancestor of HEAD (for example after a force push); or the run
    history cannot be read. A run on the `--all` path posts no comments, so
    the PRs in that range get none. This is an accepted gap.
3.  **Merged PRs.** For each first-parent commit in the range, ask GitHub for
    its pull requests. Keep each PR that is merged, has the default branch as
    its base, and has a `merge_commit_sha` in the range.
4.  **Mirror.** Select every card that the range changed, and every card
    named by a kept PR. Read each card at HEAD.
    *   A card deleted at HEAD is skipped with reason `deleted`.
    *   A card with `pending_create` is skipped with reason `pending_create`.
    *   A card with no `remote` is skipped with reason `unlinked`.
    *   For each other card, call the gateway's `update`. That call writes the
        title, labels, and body, and closes or reopens the issue from the
        card's status. It writes no file.

    The merge comment in step 5 satisfies ST-151 AC2 for each named story.
    A card that no kept PR names, but whose `status` differs between the
    range start and HEAD, gets one state comment instead: `Status <new
    status> from <commit>`. This covers reverts, direct pushes, and unnamed
    cards. The commit is the last commit in the range that changed that card.
    The comment carries its own marker,
    `<!-- story-time:v1:merge-sync-state=<commit> -->`, so it never collides
    with a merge marker. The marker check, not the result of `update`,
    decides whether to post. A rerun therefore posts a comment that an
    earlier failed run missed, and never posts a second one.
5.  **Comment.** For each kept PR, parse its references. For each referenced
    issue, post one comment, unless one already carries this PR's marker.
    *   The comment is `Merged in #<PR> (<merge sha>)` followed by
        `<!-- story-time:v1:merge-sync=<merge sha> -->`.
    *   The issue is the reference's `(#N)`, or else its card's `remote`.
    *   A mismatch between them is a permanent failure, and no comment is
        posted.
    *   The comment list is read with pagination.
    *   A PR with no `Stories:` line gets no comment, and the report names the
        PR.
6.  **Report.** Print a JSON report with `range`, `prs`, `updated`, `skipped`
    (each with an ID and a reason), `commented`, `existing` (issues that
    already had the marker), and `failed` (each marked retryable or
    permanent). Write any failure to the job summary. Exit 1 only for a
    retryable failure.

**Backstop.** `story sync-merge --all` mirrors every linked card from HEAD and
posts no comments. A rate-limit response is a retryable failure.

### Comment marker

The marker follows the existing versioned namespace
(`native/story-push/src/mcp/comments.rs:11`,
`<!-- story-time:v1:resolves=github:... -->`). The `resolves=` parser
accepts only its own exact form, so it does not read a `merge-sync=` marker
as a resolution.

## Alternatives Considered

### Alternative A: Auto-accept on merge when every box is ticked (brainstorm hypothesis)

*   **Pros**: Closes stories even when the agent forgot `story-accept`.
*   **Cons**: A ticked box is not acceptance. This skips the Definition of
    Done and the evidence checks. It needs a bot PR to record `done`, which
    brings write tokens and loop guards.
*   **Reason for Rejection**: It creates a second, weaker path to `done`.

### Alternative B: Ban closing keywords on every story issue

*   **Pros**: Only story-time closes story issues.
*   **Cons**: It drops GitHub's PR-to-issue link even when the card is
    already `done`.
*   **Reason for Rejection**: When the reference passes the acceptance rule,
    the keyword and the card agree. Only uncovered targets conflict, and rule
    4 catches them (D7, D15).

### Alternative C: Post-merge job on `pull_request: closed`

*   **Pros**: The PR body is in the event, so no lookup is needed.
*   **Cons**: A fork PR gets a read-only token. `pull_request_target` gets a
    write token, but it runs with data that the PR supplies. A direct push
    is not seen.
*   **Reason for Rejection**: Fork PRs could not be synced, and direct pushes
    would be missed (D5).

### Alternative D: Plain `story push` in CI, then discard card changes

*   **Pros**: No CLI change. Unlinked cards get issues.
*   **Cons**: For a card with no `remote`, `story push` takes the direct
    create path (`lib.rs:224-232`). Discarding the write-back loses the new
    issue number, so the next local push creates a duplicate issue.
*   **Reason for Rejection**: It creates duplicate issues (D3).

### Alternative E: Keep the rule in `scripts/story-policy.rs`, or in workflow shell

*   **Pros**: It reuses the existing checker, or needs no Rust.
*   **Cons**: The checker reaches only this repository. Shell holds
    untested business logic. Article VIII of the owner's agent constitution
    (outside this repository) says: "All backend business logic (BLL) and
    data access logic (DAL) must use Rust."
*   **Reason for Rejection**: Consumer repositories would not get the rule
    (D1).

### Alternative F: Parallel runs, each mirroring its own diff

*   **Pros**: No waiting.
*   **Cons**: A run that reads HEAD early and finishes late writes older
    state, and nothing selects that card again.
*   **Reason for Rejection**: State can stay stale with no report (D13).

### Alternative G: Scheduled reconcile only (`--all` on a timer)

*   **Pros**: One path that converges.
*   **Cons**: No per-PR comments. Issues stay stale until the next tick.
    Each tick costs calls for every linked card.
*   **Reason for Rejection**: It misses ST-151 AC1. It is kept as the weekly
    backstop.

### Alternative H: Prebuilt release binaries instead of a cache

*   **Pros**: There is no build on a cache miss (estimated at 2–4 minutes).
*   **Cons**: It needs a release pipeline, a build per platform, and checksum
    publishing.
*   **Reason for Rejection**: The pipeline adds per-platform builds and
    checksum publishing to this feature. The decision is weak (D10), and it is listed under Future Improvements.

### Alternative I: Reusable workflow (`workflow_call`) instead of copied templates

*   **Pros**: One pin bump updates every consumer. There is no copy drift.
*   **Cons**: Consumers depend on a cross-repository call. The caller's
    token scopes still need setup in each repository.
*   **Reason for Deferral**: The copied template
    matches today's `story-link` and `story-ids` practice. It is listed
    under Future Improvements.

### Alternative J: Status-only acceptance rule

*   **Pros**: One field to read instead of two. It trusts `story-accept`.
*   **Cons**: A hand-set `done` with unticked boxes passes.
*   **Reason for Rejection**: Ticked boxes are cheap to check and catch that
    case (D4).

### Alternative K: `fail` from day one

*   **Pros**: Enforcement starts at once.
*   **Cons**: Every open PR that names an `in-progress` story fails before
    ST-153 has changed the agents' instructions.
*   **Reason for Rejection**: It blocks work that followed the old
    instructions (D8).

## Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| `story-link` is not a required check | A PR with no `Stories:` line merges (the source case). | Roll-out makes `story-link` required. `story-sync` still mirrors the changed cards (D16). |
| A revert PR has no `Stories:` line | The card goes back to `in-progress` while the issue stays closed. | The changed card is mirrored, and the issue reopens (D16). The revert gets no merge comment, but it gets the state comment. |
| An agent hand-edits `done` and ticks the boxes | `check-pr` passes without `story-accept`. | Accepted risk. PR review and the `story-accept` changelog line are the controls. |
| Legacy noise in `warn` mode | Every open PR that names an `in-progress` story warns. | The warn count is the trigger for the switch to `fail` (Roll-out step 4). |
| The switch to `fail` never happens | Acceptance stays optional. | Roll-out step 4 names the trigger and the owner. The second success measure counts merged references that skipped acceptance. |
| Tracker outage partway through a run | Some issues are not updated. | A retryable failure keeps the range, so the next push run covers it again. |
| A permanent failure (404, mismatch) | One issue cannot be updated. | It is reported in the job summary, and the range advances, so it does not block later runs. |
| Rate limits during `--all` or a large range | Calls are throttled. | A rate-limit response is retryable, so the next run continues. Pacing is a Future Improvement. |
| Labels recolored on each run | Label colors change on every merge. | ST-154 fixes the gateway before `story-sync` ships. |
| Warn-mode keyword close | A keyword closes an issue whose card is not done. | If the card is named or changed, the same run reopens it. Otherwise the weekly `--all` reopens it, up to 7 days later. `fail` mode prevents it. |
| First run, renamed workflow, expired run history, or force push | The run takes the `--all` path, so the PRs in that range get no merge comment. | Accepted gap. State is still mirrored. |
| A build script reads the token | Third-party code during `cargo install` reads a write token. | The build runs in a separate job with only `contents: read`. The sync job checks out with `persist-credentials: false`. |
| Tracker repository differs from the code repository | `GITHUB_TOKEN` cannot write. | Step 1 stops with a permanent failure. A cross-repository token is out of scope. |

## Non-Functional Requirements

*   **Security.** `story-sync` writes only on `push`, schedule, or dispatch
    runs on the default branch, with no `contents: write`. The build runs in a
    job with only `contents: read`, and the sync job checks out with
    `persist-credentials: false`, so no third-party build code can read a
    write token. `check-pr` is read-only. The PR
    body reaches the binary as API data or through `env`. Consumers pin the
    binary to a source revision.
*   **Idempotency.** The gateway's `update` and its close or reopen give the
    same result when repeated (`gateway.rs:465-516`). The comment marker
    includes the merge SHA.
*   **Cost (estimates, not measured).**
    *   Each first-parent commit in the range costs one PR lookup.
    *   Each selected card costs one `issue view`, one `issue edit`, and at
        most one close or reopen.
    *   Each issue that a PR names costs one paginated comment list and at
        most one comment post.
    *   A cached run should take under a minute. A cache miss adds a 2–4
        minute `cargo install`.
    *   PR-branch caches are not shared between sibling PRs, so `check-pr`
        builds once per PR branch until a default-branch run fills the cache.

## Roll-out

1.  Merge ST-154, so that labels keep their colors, and ST-153, so that agents
    run `story-accept` before merge.
2.  Ship `check-pr` with `vars.STORY_ACCEPTANCE` unset (`warn`). Make
    `story-link` a required check.
3.  In each consumer repository, replace the bash `story-link` workflow with
    the new template. Add `story-sync.yml` with that repository's default
    branch and pinned revision. A repository that does not do this keeps
    today's link-only check and gets no post-merge sync.
4.  The repository owner sets `vars.STORY_ACCEPTANCE=fail` when the
    repository's `story-link` runs show no warnings for 7 days (D18).
5.  To roll back `story-sync`, delete its workflow file. It writes no card
    and no commit, so nothing in git needs undoing.

## Additional Resources

*   `docs/designs/merge-sync.md`: the brainstorm brief.
*   `docs/playbook.md` §3: the enforcement templates.
*   `docs/native-story-push.md`: the CLI build and install.
*   `Varsity-Engine/varsity-mcp` #79 and #80: the source incident.
*   EGM (Elephant-Goldfish Model) review reports for this design are on
    PR #345.

## Future Improvements

*   Gitea and ADO wrappers for both verbs.
*   A reusable `workflow_call` workflow (Alternative I).
*   Prebuilt release binaries (Alternative H).
*   Unblock notices for dependent stories.
*   Pacing for `--all` if rate limits appear.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

### Phases

Each phase ends with every listed command green. Commit the failing tests
first, then the implementation, as in commits 8580020 and ba9b94c. ST-154 (#350) merges before phase 4.

| Phase | Items | Exit check |
| --- | --- | --- |
| 1. Parser | 1 | `cargo test` (native suite) |
| 2. `check-pr` verb | 2, 4 (`issue_kind`), 5 (check-pr), 6, 7, 14 | Native suite, `npm test` |
| 3. Switch `story-link` | 9, 10, 12 (§3 `story-link` template) | Native suite, both `rustc --test` policy suites, `story-link` green on its own PR |
| 4. `sync-merge` verb | 3, 4, 5 (sync-merge), 6, 8, 14 (sync-merge) | Native suite, `npm test` |
| 5. `story-sync` workflow | 11, 12 (rest) | Phases 1–4 checks, then the E2E steps in the Verification Plan |

"Native suite" means the three `cargo` commands under Verification Plan.

### Target Files to Create/Modify

#### 1. `native/story-push/src/stories_line.rs` (Create)

*   **Type / Scope**: Rust domain module, shared by both verbs. Declared as
    `mod stories_line;` (private), like the other verb helpers in `lib.rs`.
*   **Proposed Changes**:
    *   `pub(crate) struct Reference { pub(crate) id: Option<String>, pub(crate) issue: Option<u64>, pub(crate) partial: bool }`.
    *   `pub(crate) fn parse(body: &str, prefix: &str) -> Vec<Reference>`.
        *   It reads only lines whose label before `:` is `story` or `stories`,
            in any case.
        *   An ID is `<prefix>-` followed by 3 or more digits, the same grammar
            as `scripts/story-policy.rs:5-9`. `#0` is not a reference.
        *   An ID or a `#N` starts a unit. A `(#N)` after an ID, separated
            from it only by spaces, joins that unit. `(partial)` marks the
            current unit. Commas and spaces separate units.
    *   `pub(crate) fn closing_targets(body: &str, repo: &str) -> Vec<u64>`.
        It returns the issue numbers that a closing keyword targets in `repo`
        only. The forms are `#N`, `owner/repo#N`, and
        `https://github.com/owner/repo/issues/N`, after an optional colon. It
        is used by `check-pr` only.
    *   Unit tests, each with its expected result:
        *   `Stories: ST-904 (#905) (partial)` gives one unit with both
            fields and `partial: true`.
        *   `Stories: #344` gives one unit with `issue: Some(344)` and no ID.
        *   `Stories: ST-001 (#12), ST-002 (#13)` gives two units.
        *   `stories: ST-001` (lowercase label) gives one unit.
        *   `ST-01`, `ST-001x`, `ST-../001`, and `#0` give no unit.
        *   A body with no `Stories:` line gives an empty list.
        *   With prefix `XY`, `XY-001` is a unit and `ST-001` is not.
        *   `close`, `closes`, `closed`, `fix`, `fixes`, `fixed`, `resolve`,
            `resolves`, and `resolved`, each with and without a colon, give a
            target. `owner/other#5` in repo `owner/repo` gives none.

#### 2. `native/story-push/src/check_pr.rs` (Create)

*   **Type / Scope**: Rust CLI verb (ST-152). `mod check_pr;` (private).
*   **Inputs**:
    *   Flags: `--body-file PATH`, `--json`. Any other flag is a usage error
        (exit 2), as `push` does (`lib.rs:493-506`).
    *   The body comes from `--body-file` when given, else from `PR_BODY`,
        else it is empty. An empty body fails rule 1 unless it is an incident.
    *   `IS_INCIDENT`: the PR is an incident only when the value is exactly
        `true`.
    *   `STORY_ACCEPTANCE`: unset, empty, or `warn` selects warn mode; `fail`
        selects fail mode. Any other value is a usage error (exit 2):
        `STORY_ACCEPTANCE must be warn or fail.`
    *   `REPO` (owner/name) and `GH_TOKEN` are needed only when the body has a
        `#N` reference or a closing keyword. `REPO` is the repository for every
        issue lookup, whatever the `tracker` setting is, as today's checker
        does.
    *   Settings: `prefix`, `backlogDir`, and `sync`, through the existing
        `context()` (`lib.rs:114`).
*   **Signature**: `pub(crate) fn run(args: &[String]) -> Result<(String, u8), Failure>`,
    the type of a `NATIVE_COMMANDS` entry (`lib.rs:46`).
*   **Card checks**: Cards are read with
    `lenient_story::parse_noting_drops(text)?.0.to_value()`, as `push` does
    (`lib.rs:211-212`). Rule 2 uses `story::acceptance_criteria`
    (`story.rs:236`). These cases are decided:
    *   A card with zero criteria is a finding: `no acceptance criteria`.
    *   A plain bullet with no checkbox counts as not done (the parser's
        existing rule).
    *   A spike with both sections is judged on `## Acceptance Criteria`,
        because the parser reads that section first. The design accepts this.
*   **Issue kinds**: a trait in this module,
    `pub(crate) trait IssueSource { fn kind(&mut self, n: u64) -> Result<IssueKind, String>; }`,
    with `enum IssueKind { Story, Other, PullRequest }`.
    *   It is implemented by `GhIssues` in this module, not by `Gateway`, so
        it works for every tracker. `GhIssues` runs
        `gh api repos/{REPO}/issues/{n}` through the `pub(crate)` constructor
        `gateway::Cli::gh()` that item 4 adds.
    *   A `pull_request` key means `PullRequest`. A `story`, `spike`, `bug`,
        or `chore` label means `Story`.
    *   An API failure is a rule 1 failure (`issue lookup failed`), never
        `Other`.
    *   Rule 1 looks up each `#N` reference. Rule 4 looks up each keyword
        target that no accepted reference covers. A target of kind `Other` or
        `PullRequest` passes rule 4.
    *   An accepted reference is one that is not `(partial)` and passes rules
        2 and 3. A `(partial)` reference therefore never covers a keyword.
*   **Output**:
    *   Each finding is one line, `<reference>: <message>`. The messages are:

        | Rule | Message |
        | --- | --- |
        | 1 | `no Stories: line`, `card not found`, `not a story issue`, `is a pull request`, `issue lookup failed` |
        | 2 | `no card has remote N`; `<ID> is <status> with <k> unticked criteria; run story-accept or mark the reference (partial)`; `<ID> has no acceptance criteria` |
        | 3 | `<ID> has no remote; run story push before merge`; `(#N) does not match remote M` |
        | 4 | `#N is not covered by a passing reference; remove the closing keyword` |

        Each card gives at most one rule 2 finding. This table matches the
        CUJ 1, CUJ 5, and D9 text.
    *   Every finding is reported. The check does not stop at the first one.
    *   `run` writes findings to stderr with `eprintln!`. For each rule 1
        failure, and each rule 2–4 finding in fail mode, it writes
        `FAIL: <line>` and then `::error::<line>`. Each warn-mode finding is
        `::warning::<line>`.
    *   `run` returns `Ok((stdout, code))`. Stdout carries only the PASS line,
        or only the JSON object with `--json`. Findings still go to stderr
        with `--json`.
    *   A pass prints `PASS: every declared story reference exists.`, the
        existing text (`scripts/story-policy.rs:151`). In warn mode with
        findings, it adds ` Warnings: <n>.`
    *   An incident with no reference prints the existing text:
        `PASS: incident exception; file the bug story within one business day.`
    *   `--json` prints one object:
        `{"mode":"warn|fail","incident":bool,"references":[{"id":str|null,"issue":int|null,"partial":bool}],"findings":[{"rule":1-4,"reference":str,"message":str}],"result":"pass|warn|fail"}`.
*   **Exit codes**: 0 for a pass or warn result; 1 for any rule 1 failure, or
    any finding in fail mode; 2 for a usage error.

#### 3. `native/story-push/src/sync_merge.rs` (Create)

*   **Type / Scope**: Rust CLI verb (ST-151). `mod sync_merge;` (private).
*   **Inputs**:
    *   Signature: `pub(crate) fn run(args: &[String]) -> Result<(String, u8), Failure>`.
    *   Flags: `--all`, `--json`, `--workflow FILE` (default
        `story-sync.yml`). Any other flag is a usage error (exit 2).
    *   `GITHUB_REPOSITORY`: the workflow repository. Step 1 compares it with
        `github.repo`.
    *   `GITHUB_REF_NAME`: the default branch (the push trigger runs only on
        it).
    *   HEAD comes from `git rev-parse HEAD`.
    *   Without `--all`, an unset `GITHUB_REPOSITORY` or `GITHUB_REF_NAME` is
        a usage error. `--all` needs neither: it uses `github.repo`.
    *   When `GITHUB_REF_NAME` is set, it must equal the repository's default
        branch (`gh repo view {repo} --json defaultBranchRef`). Otherwise the
        run is a usage error. This keeps a dispatch from a feature branch
        from mirroring that branch's cards (D6).
    *   `sync` comes from settings. A missing `sync` is `off`
        (`config.rs:216`), and `tracker: files` always has `sync: off`
        (`config.rs:227-229`), so both skip at step 1.
*   **Card selection**:
    *   Range mode: `git diff --name-only --no-renames <start> HEAD --
        <backlogDir>`.
    *   `--all`: `git ls-tree -r --name-only HEAD -- <backlogDir>`.
    *   In both modes a path is kept when its extension is `.md` and its
        stem passes `lib::valid_id` (`lib.rs:96-107`) with the configured
        prefix. This is the card-file grammar, so `ST-1` is a valid card
        stem. The 3-digit rule applies only to PR body references.
    *   Each card is read at HEAD with `git show HEAD:<path>` and parsed with
        `lenient_story::parse_noting_drops(text)?.0.to_value()`.
    *   Skip reasons:
        *   `deleted`: the file is absent at HEAD.
        *   `unparseable`: the parse fails. It is listed in `skipped` only,
            and the job summary names it.
        *   `id_mismatch`: the frontmatter `id` differs from the file stem.
        *   `pending_create`: `PENDING_CREATE` is set (`lib.rs:223`).
        *   `unlinked`: the card has no `remote` (`lib.rs:217-219`).
    *   The status at the range start comes from
        `git show <start>:<path>`. A card absent there counts as a change.
*   **References in PR bodies**:
    *   An ID maps to its card.
    *   A bare `#N` maps to the card whose `remote` is `N` (D14). If no card
        matches, no comment is posted, and the report lists `#N` under
        `skipped` with reason `unmapped`.
    *   `sync-merge` does not check issue labels. `check-pr` rule 1 is that
        gate.
*   **Output**:
    *   Plain text prints one line per action:
        *   `updated ST-1 (#2)`
        *   `skipped ST-3: <reason>`
        *   `commented #2 (PR #5)`
        *   `state #2 (<commit>)`
        *   `existing #2 (PR #5)`
        *   `failed ST-4: <retryable|permanent>: <message>`
    *   A summary line follows:
        `range <from>..<to> (<range|all>): <p> PRs, <u> updated, <s> skipped, <c> commented, <f> failed`.
    *   A push with no PR adds `no pull request for <sha>; mirrored <n>
        card(s)`, with `card` or `cards` by count.
    *   A step 1 skip prints only `skipped (<reason>) at <sha>`.
    *   The `--all` path adds its reason, for example
        `all: no earlier successful push run`.
    *   `--json` prints one object:
        `{"range":{"from":str|null,"to":str,"mode":"range|all","reason":str|null},"prs":[int],"updated":[str],"skipped":[{"id":str,"reason":str}],"commented":[{"issue":int,"pr":int|null,"kind":"merge|state"}],"existing":[{"issue":int,"pr":int|null}],"failed":[{"target":str,"kind":"retryable|permanent","message":str}]}`.
    *   When `GITHUB_STEP_SUMMARY` is set, the plain text is appended to it
        in a fenced block.
*   **Exit codes**: 0, unless a retryable failure occurred (1) or a usage
    error (2). A failed `git` subprocess is a retryable failure.
*   **Comment text**: The merge comment is `Merged in #<PR> (<sha>)`, a
    newline, and then the marker. The state comment has the same form. Both
    the text and the marker use the full 40-character SHA.
*   **Git calls** run through `std::process::Command`:
    *   `git rev-list --first-parent <start>..HEAD`
    *   `git merge-base --is-ancestor <start> HEAD`
    *   `git diff` (as above)
    *   `git log -1 --first-parent --format=%H <start>..HEAD -- <path>`
    *   `git show <rev>:<path>`

#### 4. `native/story-push/src/gateway.rs` (Modify)

*   **Type / Scope**: Rust tracker adapter (ST-151).
*   **Error kinds.**
    *   Add `pub enum ErrorKind { Retryable, Permanent }` and
        `pub struct TrackerError { pub kind: ErrorKind, pub message: String }`.
    *   Add `Cli::run_classified`, which has the same body as `Cli::run`
        (`gateway.rs:48-138`). It reads stderr to classify the failure, but it
        never copies stderr into `message`. The test at `gateway.rs:1748-1754`
        stays green, and a new test asserts the same for `run_classified`.
    *   `Cli::run` becomes `run_classified(..).map_err(|e| e.message)`, so
        existing callers and messages do not change.
    *   The classification rules, applied in order:

        | Condition | Kind |
        | --- | --- |
        | Timeout, spawn failure, unreadable output | Retryable |
        | stderr has `HTTP 429`, `rate limit`, or `HTTP 5` followed by two digits | Retryable |
        | stderr has `HTTP 404`, `HTTP 410`, or `Could not resolve to an issue` | Permanent |
        | `remote` is not a positive integer (`gateway.rs:466-468`) | Permanent |
        | Any other non-zero exit, including `HTTP 401` and `HTTP 403` | Retryable |

        Authentication failures are retryable on purpose: the job goes red,
        and the range is kept.
    *   A transferred issue is permanent. `update_classified` requests
        `--json labels,state,url` from `issue view`. When the URL names a
        repository other than `github.repo`, it returns
        `Permanent("issue transferred")` before any edit.
*   **Constructor**: `pub(crate) fn Cli::gh() -> Cli`, for `check-pr`'s
    `GhIssues` (item 2). `Cli` becomes `pub(crate)`.
*   **New methods**: GitHub backend only, each taking `&mut self`. Every call
    passes `--repo {repo}`. Issue numbers are `u64`.
    *   `last_success(workflow, branch) -> Result<Option<String>, TrackerError>`
        runs `gh run list --repo {repo} --workflow {workflow} --branch {branch} --event push --status success --limit 1 --json headSha`.
    *   `pulls_for_commit(sha) -> Result<Vec<MergedPull>, TrackerError>` runs
        `gh api repos/{repo}/commits/{sha}/pulls`. It returns
        `MergedPull { number: u64, body: String, base: String, merged_at: Option<String>, merge_commit_sha: Option<String> }`
        with no filter. `sync_merge.rs` applies the step 3 filter, because
        that filter needs the range.
    *   `comment_once(issue, marker, body) -> Result<CommentOutcome, TrackerError>`
        returns `enum CommentOutcome { Posted, Existing }`.
        *   It lists the comments with
            `gh api --paginate repos/{repo}/issues/{n}/comments --jq '.[] | [.user.login, .body] | @json'`.
        *   The list uses `--jq '.[] | [.user.id, .body] | @json'`.
        *   A comment counts as existing only when its author's numeric user
            ID is 41898282 (`github-actions[bot]`) and one of its lines
            equals the marker exactly. This matches the numeric-ID rule in
            `mcp/comments.rs:1-5`. A marker that any other user posts
            therefore cannot suppress a comment.
        *   Otherwise it posts with
            `gh issue comment {n} --repo {repo} --body-file -`.
    *   `update_classified(story) -> Result<Remote, TrackerError>`.
        *   `ensure_github_labels` (`gateway.rs:886`) and `github_state`
            (`:912`) change to return `TrackerError` and to call
            `run_classified`.
        *   `update` becomes `update_classified(..).map_err(|e| e.message)`,
            the same adapter as `Cli::run`, so `push` sees no change.
*   **Trait.** Add `pub trait MergeTracker: Tracker` (`lib.rs:123`) with the
    four methods above, implemented for `Gateway`. For `Backend::Ado` and
    `Backend::Gitea`, each method returns
    `Permanent("unsupported tracker")`. Step 1 skips those trackers before
    any call is made.
*   **Unit tests** extend `stub_cli` (`gateway.rs:1686`) with match arms for
    `run list`, `api .../pulls`, `api ... comments`, and `issue comment`, and
    add `url` to its `issue view` answer. They also add the classification
    table cases.
*   The label fix is ST-154, not this item.

#### 5. `native/story-push/src/lib.rs` (Modify)

*   **Type / Scope**: Rust CLI core.
*   **Proposed Changes**:
    *   Phase 2: add `mod stories_line;`, `mod check_pr;`, and
        `("check-pr", check_pr::run)` to the command table.
    *   Phase 4: add `mod sync_merge;` and `("sync-merge", sync_merge::run)`.
    *   `push`, `plans`, and `execute` are unchanged.

#### 6. Native command lists (Modify)

*   **Type / Scope**: Help and documentation that tests check against the
    command table. Each phase that registers a verb adds it to every file
    below, in the same commit.
*   **Files and text**:
    *   `skills/story-time/scripts/usage.txt` (`help.rs:5` embeds it). Add:
        *   Insert both lines after the `metrics` line. Keep the two-space
            indent, and start each description at column 43, as the
            existing rows do.
        *   `check-pr [--body-file F] [--json]`: "Check a PR's Stories: line
            and acceptance (CI)".
        *   `sync-merge [--all] [--workflow F] [--json]`: "Mirror merged
            cards and comment on story issues (CI)".
    *   `skills/story-time/scripts/story.mjs:34`: add `'check-pr'` and
        `'sync-merge'` to `NATIVE_ONLY` (`tests/node_unknown_command.rs`).
    *   `AGENTS.md:5`, `README.md` (the list starting "Native `story init`"),
        and both lists in `skills/story-time/SKILL.md` (starting at lines 99
        and 108): add `check-pr` and `sync-merge`
        (`tests/doc_native_lists.rs`).
    *   `skills/story-time/SKILL.md` command table: add these two rows.
        *   `` | `story check-pr [--body-file F] [--json]` | Check a PR's `Stories:` line, acceptance, and closing keywords (CI). Native Rust; read-only | ``
        *   `` | `story sync-merge [--all] [--json]` | Mirror merged cards to their issues and comment once per PR (CI). Native Rust | ``
    *   `native/story-push/tests/help_lists_native.rs`: assert that the help
        output contains `check-pr` and `sync-merge`.
    *   `docs/native-story-push.md`: one paragraph for each verb, made from
        the item 2 and item 3 Inputs and Exit codes.
*   `native/story-push/src/help.rs` and `tests/help_contract.rs` do not
    change.

#### 7. `native/story-push/tests/check_pr_contract.rs` (Create)

*   **Type / Scope**: Rust contract tests (ST-152).
*   **Harness**: the shared stub in item 14.
*   **Tests**, one or more for each criterion of ST-152, as amended:
    *   AC1: a `done` card with every box ticked passes.
    *   AC2: in fail mode, a card that is not done fails. So does a done card
        with one unticked box.
    *   AC3: `(partial)` passes, including the CUJ 2 string.
    *   AC4: an incident PR with no reference passes and prints the incident
        text.
    *   AC5: in fail mode, a keyword on an uncovered story issue fails. A
        keyword on a covered issue passes, and so does a keyword on an issue
        with no story-type label.
    *   AC6: warn mode exits 0 and prints `::warning::`.
*   **Extra tests**, each with its expected result:
    *   A spike with unticked Exit Criteria (and no Acceptance Criteria) fails
        rule 2 in fail mode.
    *   A done card with zero criteria fails rule 2 in fail mode.
    *   An unlinked card is a finding. With `sync: off`, it is not.
    *   A bare `#N` uses the card whose `remote` is `N`.
    *   An ID and `(#N)` that disagree fail in fail mode and warn in warn mode.
    *   `Closes: owner/repo#N` and an issue URL each count as a keyword.
    *   The `XY-` prefix and a custom `backlogDir` work.
    *   `STORY_ACCEPTANCE=strict` exits 2.
    *   A label API failure (stub exit 1) exits 1.
    *   With `tracker: files`, `Stories: #N` on a story issue passes rule 1.
    *   These are moved from `test/story-policy-contract.rs` (item 9):
        `ST-001` passes, `Related ST-001` and `ST-999999999999` fail with
        `FAIL:`, and an incident PR with an invalid reference still fails.

#### 8. `native/story-push/tests/sync_merge_contract.rs` (Create)

*   **Type / Scope**: Rust contract tests (ST-151).
*   **Harness**: the item 14 stub, a temporary git repository, and
    `GITHUB_REPOSITORY` and `GITHUB_REF_NAME` set. The stub answers
    `repo view` with the default branch.
*   **Tests**, one or more for each criterion of ST-151, as amended:
    *   AC1: each PR gets one comment on each issue, and a rerun posts none.
    *   AC2: the issue closes and reopens with the card's status.
    *   AC3: an issue whose title, labels, or body differs from its card is
        updated to match the card.
    *   AC4: the card bytes are unchanged. The issue of a named story that is
        not done is not closed.
    *   AC5: `sync: off` and `tracker: files` each make zero `gh` calls.
    *   AC6, as amended: with no PR, changed cards are mirrored. A status
        change gets one state comment, and no merge comment is posted. The
        report names the SHA.
    *   AC7: `git status --porcelain` is empty after the run.
*   **Extra tests**, each with its expected result:
    *   The range starts at the last successful push run, so the PR of a
        skipped run gets its comment. A successful `--all` run does not move
        the start.
    *   A start commit that is not an ancestor of HEAD, or an empty run
        history, takes the `--all` path and reports why.
    *   One push that carries two PRs gets both comments.
    *   A merge commit (not a squash) of a 50-commit branch makes one PR
        lookup.
    *   A 503 response exits 1. A 404 exits 0, and the next run's range
        starts after it.
    *   A rerun posts the comment that a failed run missed, once. This holds
        for the merge comment of a named card and for the state comment of
        an unnamed card.
    *   A marker posted by another user does not suppress the comment.
    *   The report lists deleted, `pending_create`, `id_mismatch`, and
        `unmapped` items, each with its reason.
    *   `GITHUB_REF_NAME=feature` exits 2.
    *   `--all` mirrors a card that the range did not change.

#### 9. `scripts/story-policy.rs` and `test/story-policy-contract.rs` (Modify)

*   **Type / Scope**: This repository's CI checker and its contract suite.
*   **`scripts/story-policy.rs`**:
    *   Remove `references` (:16), `validate_links` (:42), `link` (:126),
        and the `story-link` arm (:221).
    *   Remove the link unit tests (:239-310), and keep the ID tests
        (:311-327). Keep the `story_file("backlog/nested/ST-001.md") == None`
        assertion from `invalid_and_path_traversal_ids_are_not_references` as
        its own test, `nested_story_paths_are_not_story_files`.
    *   Change line 1 to `use std::{collections::BTreeSet, env, process::Command};`.
        Only `link` uses `Path`, and the contract suite compiles this file
        with `-Dwarnings` (`test/story-policy-contract.rs:36-42`).
    *   Change the usage string to `Usage: story-policy story-ids`.
    *   `story_id` and `story_file` stay, because `ids` uses them.
*   **`test/story-policy-contract.rs`**:
    *   Remove `link()` (:58) and the three `real_cli_*` link tests (:91-117).
        Item 7 now covers them.
    *   In `cartographer_preparation_keeps_distinct_story_ids_and_no_runtime_authority`
        (:364; the call is at :400), replace the `link("Stories: ST-002")` assertion with an
        assertion that `backlog/ST-002.md` exists.
    *   In the meta-test list (:168-174), drop
        `real_cli_rejects_missing_and_undeclared_stories`.
*   `.story-time/settings.json` `testCommand` does not change, because both
    suites still exist.

#### 10. `.github/workflows/story-link.yml` (Modify)

*   **Type / Scope**: CI configuration (ST-152). The job ID stays
    `story-link`, which the ruleset requires
    (`.github/story-mandate-ruleset.json:19`). The workflow has one job. The
    token has read-only scopes and is exposed only to the check step. The
    rustc step is removed. The `story-policy-tests` job still runs both
    `rustc --test` policy suites.
*   **Full file**:

    ```yaml
    name: story-link
    on:
      pull_request:
        types: [opened, edited, synchronize, reopened, labeled, unlabeled]
    permissions:
      contents: read
      issues: read
      pull-requests: read
    jobs:
      story-link:
        runs-on: ubuntu-latest
        steps:
          - uses: actions/checkout@v4
            with:
              persist-credentials: false
          - uses: dtolnay/rust-toolchain@stable
          - id: cache
            uses: actions/cache@v4
            with:
              path: ~/.story-root/bin/story
              key: story-${{ hashFiles('native/story-push/**', 'skills/story-time/scripts/usage.txt') }}
          - if: steps.cache.outputs.cache-hit != 'true'
            run: cargo install --locked --path native/story-push --bin story --root ~/.story-root
          - name: Check declared stories
            env:
              PR_BODY: ${{ github.event.pull_request.body }}
              IS_INCIDENT: ${{ contains(github.event.pull_request.labels.*.name, 'incident') }}
              STORY_ACCEPTANCE: ${{ vars.STORY_ACCEPTANCE }}
              REPO: ${{ github.repository }}
              GH_TOKEN: ${{ github.token }}
            run: ~/.story-root/bin/story check-pr
    ```

    In this repository, a PR can change the checker that checks it, as it
    can today with `scripts/story-policy.rs`. The control is the ruleset's
    required review, which sees the checker change in the diff.

#### 11. `.github/workflows/story-sync.yml` (Create)

*   **Type / Scope**: CI configuration (ST-151).
*   **Full file**:

    ```yaml
    name: story-sync
    on:
      push:
        branches: [main]
      schedule:
        - cron: '17 6 * * 1'
      workflow_dispatch:
        inputs:
          all:
            type: boolean
            default: true
    concurrency:
      group: story-sync
      cancel-in-progress: false
    permissions: {}
    jobs:
      build:
        runs-on: ubuntu-latest
        permissions:
          contents: read
        steps:
          - uses: actions/checkout@v4
            with:
              persist-credentials: false
          - uses: dtolnay/rust-toolchain@stable
          - id: cache
            uses: actions/cache@v4
            with:
              path: ~/.story-root/bin/story
              key: story-${{ hashFiles('native/story-push/**', 'skills/story-time/scripts/usage.txt') }}
          - if: steps.cache.outputs.cache-hit != 'true'
            run: cargo install --locked --path native/story-push --bin story --root ~/.story-root
          - uses: actions/upload-artifact@v4
            with:
              name: story-bin
              path: ~/.story-root/bin/story
      sync:
        needs: build
        if: github.ref == format('refs/heads/{0}', github.event.repository.default_branch)
        runs-on: ubuntu-latest
        permissions:
          contents: read
          issues: write
          pull-requests: read
          actions: read
        steps:
          - uses: actions/checkout@v4
            with:
              fetch-depth: 0
              persist-credentials: false
          - uses: actions/download-artifact@v4
            with:
              name: story-bin
              path: ${{ runner.temp }}/story-bin
          - run: chmod +x "$RUNNER_TEMP/story-bin/story"
          - name: Sync merged stories
            env:
              GH_TOKEN: ${{ github.token }}
              ALL: ${{ github.event_name == 'schedule' || inputs.all == true }}
            run: |
              if [ "$ALL" = true ]; then
                "$RUNNER_TEMP/story-bin/story" sync-merge --all --json
              else
                "$RUNNER_TEMP/story-bin/story" sync-merge --json
              fi
          - run: test -z "$(git status --porcelain)"
    ```

    A dispatch with `all: false` runs the range path and posts comments. It
    is not a push run, so it never moves the range start. A dispatch from any
    branch other than the default branch skips the `sync` job.

#### 12. `docs/playbook.md` (Modify)

*   **Type / Scope**: Documentation.
*   **Consumer templates.** §3 replaces the bash `story-link` template with
    the item 10 file, and adds the item 11 file. Each is copied with three
    substitutions:
    *   The install step becomes
        `cargo install --locked --git https://github.com/Team-Teddy-Development/story-time --rev <sha> story-time-native --bin story --root ~/.story-root`.
        It does not check out story-time's source.
    *   The cache key becomes `story-<sha>`.
    *   `branches: [main]` becomes the consumer's default branch.
*   **Text changes**:
    *   §3 also adds:
        *   the `vars.STORY_ACCEPTANCE` roll-out and its trigger (D18);
        *   `(partial)`;
        *   the closing-keyword rule;
        *   the default-branch note.
    *   §3 mechanism table (`playbook.md:139-145`): add a `story-sync` row.
    *   §2 rule 5: the PR carries the `done` card, and `story-sync` closes
        the issue after the merge.
    *   §5 step 2 (`playbook.md:290`): add `story-sync.yml` to the workflows
        to copy.

#### 13. `backlog/ST-151.md`, `backlog/ST-152.md` (No change; already published)

*   **Type / Scope**: Story cards.
*   **Status**: These amendments are in the published cards (#344, #346,
    2026-11-16):
    *   ST-151: AC1 (D13), AC6 (D16 and the state comment), and Out of
        Scope.
    *   ST-152: AC1, AC2, and AC5, plus `depends_on: [ST-153]`.
    *   The re-estimates: ST-151 to 8, ST-152 to 5.

    No step remains.

#### 14. Shared test harness (Create, Modify)

*   `native/story-push/tests/common/mod.rs` (Create). It starts with
    `#![allow(dead_code)]`, because not every test crate uses every helper.
    *   `pub fn install_stub_gh(bin_dir: &Path)` compiles the stub with
        `rustc`, as `push_lenient_contract.rs:121-138` does today.
    *   The stub reads stdin to the end first, because `--body-file -`
        writes to it.
    *   It appends its arguments to `gh-calls`, so tests can count calls.
    *   It reads `responses.tsv` from its working directory. Each line is
        `<argument prefix>\t<exit code>\t<stdout file>\t<stderr text>`.
        *   The stdout file path is relative to the working directory.
        *   The stderr text contains no tab or newline.
        *   A line matches when the arguments, joined by single spaces,
            start with the prefix. The first match answers.
        *   No match exits 0 with no output.
    *   `PATH` is `bin_dir` plus the directory of the real `git`, as in
        `tests/new_contract.rs:33`.
*   `native/story-push/tests/push_lenient_contract.rs` (Modify). It calls
    `common::install_stub_gh` and writes a `responses.tsv` with its three
    current answers:
    *   `issue create` gives `https://github.com/acme/shop/issues/11`.
    *   `issue list` gives `[]`.
    *   `issue view` gives `{"labels":[],"state":"OPEN","url":"https://github.com/acme/shop/issues/11"}`.

### Verification Plan

*   **Compilation/Build**:
    `cargo build --manifest-path native/story-push/Cargo.toml --release --locked`
*   **Native suite**:
    *   `cargo test --manifest-path native/story-push/Cargo.toml --locked`
    *   `cargo fmt --manifest-path native/story-push/Cargo.toml --check`
    *   `cargo clippy --manifest-path native/story-push/Cargo.toml --locked --all-targets -- -D warnings`
*   **Policy suites**: the `testCommand` in `.story-time/settings.json`. It
    runs `npm test`, then both `rustc --test` suites.
*   **Backlog**: `story lint ST-151 ST-152 ST-153 ST-154`.
*   **Test coverage requirement**: each acceptance criterion of ST-151 (7)
    and ST-152 (6) has at least one named contract test.
*   **E2E/Integration**, in story-time itself. Preconditions: `sync` is not
    `off` (it is `auto` in `.story-time/settings.json`), and
    `vars.STORY_ACCEPTANCE` is unset.
    1.  Open a PR that names one linked `in-progress` story. `story-link`
        passes with one `::warning::` naming its status.
    2.  Merge a PR that names a `done` story. Its issue closes, with one
        merge comment.
    3.  Rerun `story-sync` for that push. No second comment appears.
    4.  Merge a second PR before the first PR's `story-sync` run finishes.
        Both PRs get their comments. The second run starts only after the
        first run completes: the run times do not overlap.
