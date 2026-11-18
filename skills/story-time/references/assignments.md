# Local agent assignments

ST-168 adds execution ownership beside story status. It does not redefine readiness,
acceptance, dependencies, tracker synchronization, or Cartographer admission.

## Commands

Run from a local Linux Git worktree with the repository's configuration.

```bash
story assignment next --owner run-unique-id --pid 12345
story assignment status
story assignment heartbeat --token TOKEN
story assignment reconcile
story assignment release --token TOKEN
```

For manual kickoff of a named story, run `story assignment next --owner RUN
--pid PID --story ID`. This uses the same reservation lock and capacity as
ranked selection. It never falls back to another story. A `story_unavailable`
or `story_mismatch` result does not admit work. Confirm the returned story ID
before changing status. A direct `story set` is a lifecycle edit, not an
execution reservation or a supported substitute for kickoff.

For dispatch that cannot take over old work, add `--fresh-only` to the targeted
command. Start work only on `claimed`. Under the same lock, this mode skips
reconciliation and returns `reservation_exists` if the requested story or run
already has a reservation. It does not change existing claims or tokens, even
when an owner has exited. Every recorded claim still counts toward WIP. Do not
retry without `--fresh-only` when this mode refuses or the CLI lacks the flag.

Use the actual persistent agent process ID. The process must be visible in the
same host environment. The token is a bearer capability; keep command results
out of public logs. The CLI always emits structured JSON for successful
operations and expected ownership refusals. Invalid input or corrupt state fails
closed. Exit zero alone does not mean a story was assigned. Check `reason`.

## Exact supervised recovery

From the recorded worktree and branch, use:

```bash
story assignment recover --story ID --previous-owner OLD_RUN \
  --previous-process "$OLD_PROCESS_JSON" --owner NEW_RUN --pid NEW_PID
```

`OLD_PROCESS_JSON` is the `process` object from the inspected recoverable claim, not the complete assignment response. Keep that response private. The command checks the prior run, process, story, worktree, and branch under the native lock. It transfers only a recoverable claim with a verified terminated predecessor. It does not reconcile other claims or admit a new one.

Start only on `recovered` and a verified replacement claim. The success result contains the new bearer token; keep it in the supervisor's private records. A mismatch returns `recovery_mismatch` with exit 1 and no claim. Do not fall back to `next`.

## Ownership invariants

A repository-wide ledger lives at `<git-common-dir>/story-assignment/ledger.json`.
Linked worktrees share its kernel-backed file lock. Atomic replacement preserves
the previous ledger on pre-rename write failure. Tokens use the existing secure
random generator. Ledger files and the lock are created with mode 600.

Each active claim records story ID, run ID, opaque token, heartbeat, process
identity, worktree, branch, and initial HEAD. Process identity includes machine
ID, boot ID, PID, and process start time. Reused PIDs and zombies are not live
owners. Another machine or unreadable identity is unknown, not dead.

Selection first resumes the same owner, then offers verified terminated work,
then chooses a ready story through the existing WSJF ranking and readiness lint.
Dependencies must still be done. Recovery requires the original worktree and
branch. An ineligible recovery is reported, not bypassed with unrelated new work.

`reserved_wip` counts recorded claims, including uncertain and recoverable owners,
against the configured repository WIP limit. Bare in-progress statuses are not
execution evidence. `legacy_in_progress` lists individually quarantined IDs from
all registered linked worktrees. Those IDs cannot receive a new claim or a
preparation recommendation, even when another worktree marks them ready. Their
presence does not block an unrelated eligible story. `legacy_next_action` explains
the hold; no lifecycle status or ownership is released.

This is a local shared-repository safety limit, not a cross-machine scheduler or
a measurement of team capacity. Participating agents must use this protocol; it
cannot prevent an unrelated agent from bypassing it.

## Recovery and migration

Heartbeat age is diagnostic only. `next` without `--fresh-only` and `reconcile` invalidate an active
token only after verifying termination. They retain the story and work location
as recoverable. The next owner receives a new token. Old heartbeat and release
requests cannot modify that replacement claim.

The commands never delete a branch, worktree, PR, or source file, and never
change a story's status. A claim remains counted while a PR waits for CI. Explicit
release removes execution ownership; it does not establish acceptance or
silently requeue in-progress work.

For an existing backlog with untracked in-progress stories, inspect each named
story's actual owner, agent, branch, and PR. Do not reset the backlog based on age
or absence of recent commits. Resume or transfer active work through its owner.
Only after evidence establishes abandonment may the authorized coordinator use
the normal story lifecycle tools to reconcile that story. Preserve its partial
work and provenance. Unknown evidence keeps that story blocked, not the whole
execution queue.

## Loop integration

Use the assignment step at each authorized loop tick. Work returned by
`claimed`, `resumed`, or `recovered` still passes project-specific scope
admission and story-kickoff. With a free slot but no eligible ready story,
`preparation_required` nominates one unblocked, scored candidate through the same
WSJF ranking. Read `preparation.story_id` and `next_action`. This recommendation
creates no claim and changes no story. Establish existing coordinator preparation
ownership and inspect prior work or PRs before editing. Restore readiness and
admission, then request an execution claim before implementation. Unknown sprint
velocity prevents a forecast, not this preparation step.

If no scored, unblocked candidate exists, `no_eligible_story` names the missing
prerequisites to wait for. Do not repeat a whole-backlog inventory to simulate
progress. Do not apply a separate status-count WIP gate after the native result.
Installing a skill does not change a cached session prompt. Existing loops that
already invoke the installed CLI receive its new result on their next tick;
verify that tick before claiming adoption.

## Verification

```bash
cargo test --manifest-path native/story-push/Cargo.toml --test assignment_contract
```

The tests execute the real CLI in temporary repositories and linked worktrees,
with live and terminated process fixtures, concurrent claimants, stale tokens,
malformed state, and retained uncommitted work. They do not claim that the live
Cartographer backlog has been reconciled or that its 83 legacy owners are dead.
