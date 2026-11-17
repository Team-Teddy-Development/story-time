---
name: story-pick
description: Choose the single highest-value story to work on next — WSJF rank over ready, unblocked stories, then checked against WIP limits, capacity, dependencies, and risk — and explain the choice with alternatives. Use when the user asks "what should I work on next?", "pick a story", "what's the highest-value thing", or at the start of a work session.
---

# Story Pick

`story` means the CLI from the **story-time** skill.

A **sprint-planning** skill artifact is a proposal, not permission to start work.
Recheck source freshness, readiness, dependencies, and WIP before pulling a selected
story. WIP limits concurrent work per person, not total planned stories. A missing
capacity is unknown; do not derive available points or dates from `sprintDays`.

## Local unattended agent loops

Use the native assignment command before choosing work. A story's status is not
proof that an agent still owns it. On this Linux machine, the assignment ledger
shares ownership across linked Git worktrees and reserves configured WIP slots.

```bash
story assignment status
story assignment next --owner <unique-run-id> --pid <agent-process-id>
```

Supply the long-running agent's PID, not the short-lived story command or an
unrelated shell. Keep its returned token private. Reuse the run ID for the same
agent only. The command checks ranked readiness and dependencies, but does not
change story status, grant scope admission, or mark acceptance.

Follow the JSON `reason` rather than interpreting every exit-zero result as work:

- `claimed` or `resumed`: use `claim.story_id`, then run story-kickoff.
- `recovered`: inspect and resume the partial work in `claim.worktree` on
  `claim.branch`; never create a fresh implementation beside it.
- `recovery_required` or `recovery_ineligible`: follow `next_action` to the
  recorded worktree or restore the story's prerequisites before requesting work.
- `capacity_full`, `legacy_ownership_unknown`, or `owner_conflict`: preserve
  current work. Reconcile the named owners; do not increase WIP or reset statuses.
- `no_eligible_story`: refine one near-term candidate or its concrete blocker.
  Unknown sprint velocity prevents a forecast, not a claim with free WIP.

Send `story assignment heartbeat --token <token>` while working. After acceptance
or an explicit handoff, run `story assignment release --token <token>`. Release
invalidates the token, not the story's lifecycle status. A released in-progress
story still needs explicit lifecycle reconciliation before another claim.

`story assignment reconcile` invalidates tokens only for verified terminated
local process identities. Old heartbeats alone never release work. Unknown
legacy in-progress stories remain protected and require named ownership evidence.
When an open PR exists, continue that PR instead of starting implementation again.
Missing admission, unresolved public decisions, and live owners remain blockers.

For repos without the native assignment command, use the manual workflow below;
do not substitute a private per-agent claim file. See
[local assignments](../story-time/references/assignments.md) for the recovery contract.

## 1. Stop starting, start finishing

```bash
story list --status in-progress --json
```

If anything is already in progress, especially if it's the user's own,
identify its current owner and finish or recover it first. In an owner-authorized
unattended loop, use the assignment protocol above instead of repeatedly asking
whether to continue.
The WIP limit is `team.wipLimit` from `story config` (default: one story
per person).

## 2. Rank

```bash
story rank --limit 5 --json
```

This ranks only `ready` stories whose dependencies are `done`, by
WSJF = (value + time_criticality + risk_reduction) / size. Ties go to the
smaller story, then the older one. Read `excluded` too: a high-value story
blocked by one small story usually means the blocker is the real pick.

If nothing ranks, say why (nothing ready, everything blocked, nothing
scored) and offer the **story-ready**, **story-estimate**, or
**refinement** skill.

## 3. Sanity-check the top candidates

WSJF is a heuristic, so check the top three against what it can't see:

- **Fits the time left.** Size against the days remaining in the sprint
  (`team.sprintDays`) and the team's pace (`team.velocity`), or the user's
  available time.
- **Fits the person.** Skills, access, the area of the codebase they know.
- **Unblocks others.** Does finishing it unblock excluded stories? That
  counts in its favour.
- **Risk early.** Between near-equal scores, prefer the one that retires
  more uncertainty.
- **Still true.** Re-read the story. If the criteria are out of date, it is
  not really ready. Send it back with the **story-edit** skill.

Overrule the rank only for a reason you can state in one line.

## 4. Recommend

```
Pick: ST-012 Reset password via emailed link — WSJF 5.33 (CoD 16 / size 3)
Why: highest score; unblocks ST-015; fits the 3 days left.
Alternatives:
  ST-018 (4.0) — needs the payments sandbox account, not here yet
  ST-021 (3.5) — larger; better next sprint
```

## 5. Commit to it

When the user selects it, or an authorized unattended assignment returns it:

```bash
story set ST-012 status=in-progress --note "picked: <one-line reason>"
```

Then hand off to the **story-kickoff** skill.
