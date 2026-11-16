# Merge Sync: Keep Story Issues in Step with Merged Pull Requests

*   **Author:** Eric Jernigan
*   **Status:** In Review
*   **Revision:** 3.2 (after EGM review Stage 8)
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

*   **Build.** It has `permissions: {}`. It restores the cached binary or runs
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
| A build script reads the token | Third-party code during `cargo install` reads a write token. | The build runs in a separate job with `permissions: {}`. The sync job checks out with `persist-credentials: false`. |
| Tracker repository differs from the code repository | `GITHUB_TOKEN` cannot write. | Step 1 stops with a permanent failure. A cross-repository token is out of scope. |

## Non-Functional Requirements

*   **Security.** `story-sync` writes only on `push`, schedule, or dispatch
    runs on the default branch, with no `contents: write`. The build runs in a
    job with `permissions: {}`, and the sync job checks out with
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

### Target Files to Create/Modify

#### 1. `native/story-push/src/stories_line.rs` (Create)

*   **Type / Scope**: Rust domain module, shared by both verbs.
*   **Proposed Changes**:
    *   `Reference { id: Option<String>, issue: Option<u64>, partial: bool }`.
    *   `parse(body: &str, prefix: &str) -> Vec<Reference>`. It finds the lines
        whose label before `:` is `story` or `stories` (case-insensitive). It
        splits each line into reference units: an ID or a `#N` starts a unit,
        a `(#N)` directly after an ID joins that unit, and `(partial)` marks
        the current unit.
    *   `closing_targets(body: &str) -> Vec<u64>`, used by `check-pr` only.
    *   Unit tests:
        *   `ST-904 (#905) (partial)` gives one partial unit with both fields.
        *   `#344` alone gives an issue-only unit.
        *   A lowercase label, a malformed ID, a path-traversal ID, and a
            missing line.
        *   The `XY-` prefix.
        *   Each keyword tense.

#### 2. `native/story-push/src/check_pr.rs` (Create)

*   **Type / Scope**: Rust CLI verb (ST-152).
*   **Proposed Changes**:
    *   `story check-pr [--body-file PATH] [--json]`. It reads `PR_BODY`,
        `IS_INCIDENT`, `STORY_ACCEPTANCE`, `REPO`, and `GH_TOKEN` from the
        environment.
    *   It runs rules 1–4. It gets labels through a `LabelSource` trait,
        backed by `gh api repos/{repo}/issues/{n}`. An API failure is an
        error, never "not a story".
    *   Exit codes: 0 for a pass or a warning; 1 for a link failure, or a
        finding in `fail` mode; 2 for a usage error.

#### 3. `native/story-push/src/sync_merge.rs` (Create)

*   **Type / Scope**: Rust CLI verb (ST-151).
*   **Proposed Changes**:
    *   `story sync-merge [--json]` runs steps 1–6.
    *   `story sync-merge --all [--json]` runs the backstop.
    *   A `MergeTracker` trait (`last_success`, `pulls_for_commit`, `update`,
        `comment_once`) allows a fake in tests. Each error carries a
        `retryable` or `permanent` kind.
    *   `git rev-list --first-parent`, `git merge-base --is-ancestor`,
        `git diff --name-only`, `git log -1 --format=%H <range> -- <path>`, and
        `git show <rev>:<path>` run through `std::process::Command`.
    *   It writes the job summary to `$GITHUB_STEP_SUMMARY` when set.

#### 4. `native/story-push/src/gateway.rs` (Modify)

*   **Type / Scope**: Rust tracker adapter (ST-151).
*   **Proposed Changes**:
    *   Add `last_success(workflow, branch)`, through
        `gh run list --event push --status success --json headSha`.
    *   Add `pulls_for_commit(sha)`, through
        `gh api repos/{repo}/commits/{sha}/pulls`, filtered as in step 3.
    *   Add `comment_once(issue, marker, body)`. It runs a paginated list,
        then `gh issue comment --body-file -` when the marker is absent.
    *   Implement `MergeTracker` for `Gateway`.
    *   Unit tests use the fake-`gh` harness at `gateway.rs:1690`.
    *   The label fix itself is ST-154, not this item.

#### 5. `native/story-push/src/lib.rs` (Modify)

*   **Type / Scope**: Rust CLI core.
*   **Proposed Changes**: Register `check-pr` and `sync-merge` in the command
    table, and add the three `mod` lines. `push`, `plans`, and `execute` are
    unchanged.

#### 6. `native/story-push/src/help.rs` (Modify)

*   **Type / Scope**: CLI help text.
*   **Proposed Changes**: Document both verbs. Update
    `tests/help_contract.rs` and `tests/help_lists_native.rs`.

#### 7. `native/story-push/tests/check_pr_contract.rs` (Create)

*   **Type / Scope**: Rust contract tests (ST-152).
*   **Proposed Changes**: The tests run the built binary against a temporary
    backlog and a fake `gh`. One test covers each amended criterion:
    *   AC1: a `done`, fully ticked card passes.
    *   AC2: a card that is not done, or has an unticked box, fails in `fail`
        mode.
    *   AC3: `(partial)` passes, including the CUJ 2 string.
    *   AC4: an `incident` PR passes.
    *   AC5: a keyword on an uncovered story issue fails. On a covered issue
        it passes.
    *   AC6: `warn` mode exits 0 with `::warning::`.
    *   A spike with unticked Exit Criteria fails rule 2.
    *   Extra tests:
        *   An unlinked card is a finding, except with `sync: off`.
        *   A bare `#N` maps to its card.
        *   An ID and `(#N)` that disagree is a finding in `fail` mode and a
            warning in `warn` mode.
        *   `Closes: owner/repo#N` and an issue URL are recognized.
        *   The `XY-` prefix and a custom `backlogDir` work.

#### 8. `native/story-push/tests/sync_merge_contract.rs` (Create)

*   **Type / Scope**: Rust contract tests (ST-151).
*   **Proposed Changes**: The tests run against a fake `gh` and a temporary
    git repository with a bare `origin`.
    *   AC1: one comment per PR per issue, and a rerun posts none.
    *   AC2: close and reopen follow the card.
    *   AC3: drift is fixed.
    *   AC4: card bytes are unchanged.
    *   AC5: `sync: off` and `tracker: files` make zero `gh` calls.
    *   AC6 (amended): with no PR, changed cards are mirrored, no comment is
        posted, and the report names the SHA.
    *   AC7: `git status --porcelain` is empty.
    *   Extra tests:
        *   The range starts at the last successful push run, so a skipped
            run's PR is covered. A successful `--all` run does not advance it.
        *   A last-success commit that is not an ancestor, or a missing run
            history, takes the `--all` path.
        *   One push that carries two PRs gets both comments, and a true merge
            of a 50-commit branch makes one PR lookup.
        *   A retryable failure exits 1. A permanent failure (404) exits 0,
            and the next run's range starts after it.
        *   A rerun after a run that closed an issue but failed before
            commenting posts the AC2 state comment once.
        *   An unnamed card whose status changed gets one state comment with
            the `merge-sync-state` marker. A named card gets only the merge
            comment.
        *   Deleted and `pending_create` cards are skipped with their reasons.

#### 9. `scripts/story-policy.rs` (Modify)

*   **Type / Scope**: This repository's CI checker.
*   **Proposed Changes**: Remove the `story-link` subcommand and
    `references()`. Keep `story-ids`, the check that rejects a new story ID
    already used on the default branch or in an older open PR.

#### 10. `.github/workflows/story-link.yml` (Modify)

*   **Type / Scope**: CI configuration (ST-152).
*   **Proposed Changes**: Install the cached binary as in item 11. Run
    `story check-pr` with this `env`:
    *   `PR_BODY`: `github.event.pull_request.body`
    *   `IS_INCIDENT`: `contains(github.event.pull_request.labels.*.name, 'incident')`
    *   `REPO`: `github.repository`
    *   `GH_TOKEN`: `github.token`
    *   `STORY_ACCEPTANCE`: `vars.STORY_ACCEPTANCE`

#### 11. `.github/workflows/story-sync.yml` (Create)

*   **Type / Scope**: CI configuration (ST-151).
*   **Proposed Changes**:
    *   **Triggers**: `push` to `main` (consumers change it to their default
        branch, because GitHub does not expand expressions in branch filters).
        Also `workflow_dispatch`, with an `all` boolean input, and a weekly
        `schedule` that runs `--all`.
    *   **Concurrency**: `concurrency: { group: story-sync, cancel-in-progress: false }`,
        shared by all three triggers.
    *   **Job `build`**, with `permissions: {}`:
        1.  `dtolnay/rust-toolchain@stable`.
        2.  `actions/cache` on `~/.cargo/bin/story`. This repository keys on
            a hash of `native/story-push/**` and `Cargo.lock`. The template
            keys on the pinned `--rev`.
        3.  On a miss, `cargo install --locked`. This repository installs
            from a checkout of its own source with `--path native/story-push`
            (checkout with `persist-credentials: false`). The template
            installs with
            `--git https://github.com/Team-Teddy-Development/story-time --rev <sha>`.
        4.  `actions/upload-artifact` of the binary.
    *   **Job `sync`**, which needs `build`, with `contents: read`,
        `issues: write`, `pull-requests: read`, and `actions: read`:
        1.  `actions/checkout` with `fetch-depth: 0` and
            `persist-credentials: false`.
        2.  `actions/download-artifact` of the binary.
        3.  `story sync-merge --json`, or `--all` for a dispatch or schedule
            run. Set `GH_TOKEN: ${{ github.token }}` on this step only.
        4.  `test -z "$(git status --porcelain)"`.

#### 12. `docs/playbook.md` (Modify)

*   **Type / Scope**: Documentation.
*   **Proposed Changes**:
    *   §3: Replace the bash `story-link` template with the `check-pr`
        template. Add `story-sync.yml`, the `vars.STORY_ACCEPTANCE` roll-out,
        `(partial)`, the closing-keyword rule, and the default-branch note.
    *   §2 rule 5: The PR carries the `done` card, and `story-sync` closes
        the issue after the merge.

#### 13. `backlog/ST-151.md`, `backlog/ST-152.md` (Modify; done with this revision)

*   **Type / Scope**: Story cards.
*   **Proposed Changes**: Amend ST-151 AC6 (D16) and its Out of Scope
    (D7). Replace the ST-151 Notes on `--no-write-back` (D3). Amend ST-152
    AC1 and AC2 (merge ref) and AC5 (D7, D15). Name `check-pr` in ST-152.
    Set ST-152 `depends_on: [ST-153]`. Publish both with `story push`.

### Verification Plan

*   **Compilation/Build**:
    `cargo build --manifest-path native/story-push/Cargo.toml --release --locked`
*   **Unit and contract tests**:
    `cargo test --manifest-path native/story-push/Cargo.toml --locked`
*   **Static Analysis & Linting**:
    *   `cargo fmt --manifest-path native/story-push/Cargo.toml --check`
    *   `cargo clippy --manifest-path native/story-push/Cargo.toml --locked --all-targets -- -D warnings`
    *   `story lint ST-151 ST-152 ST-153 ST-154`
*   **Test Coverage Requirement**: Each acceptance criterion of ST-151 (7) and
    ST-152 (6) has at least one named contract test.
*   **E2E/Integration**, in story-time itself:
    1.  Open a PR that names an `in-progress` story. `story-link` warns.
    2.  Merge a PR that names a `done` story. Its issue closes, with one
        comment.
    3.  Rerun `story-sync`. No second comment appears.
    4.  Merge two PRs within one minute. Both get their comments.

--------------------------------------------------------------------------------

*Drafted with EGM*
