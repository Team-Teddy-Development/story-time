---
name: story-pick
description: Choose the single highest-value story to work on next — WSJF rank over ready, unblocked stories, then checked against WIP limits, capacity, dependencies, and risk — and explain the choice with alternatives. Use when the user asks "what should I work on next?", "pick a story", "what's the highest-value thing", or at the start of a work session.
---

# Story Pick

`story` means the CLI from the **story-time** skill.

## 1. Stop starting, start finishing

```bash
story list --status in-progress --json
```

If anything is already in progress, especially if it's the user's own,
recommend finishing it first and ask whether they still want a new story.
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
**backlog-groom** skill.

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

When the user confirms:

```bash
story set ST-012 status=in-progress --note "picked: <one-line reason>"
```

Then hand off to the **story-kickoff** skill.
