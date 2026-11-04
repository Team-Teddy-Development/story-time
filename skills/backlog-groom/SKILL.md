---
name: backlog-groom
description: Run a backlog refinement pass — health stats, readiness lint, stale and duplicate detection, oversized and unscored stories, and re-ordering — and return a prioritized list of proposed changes for approval. Use when the user says "groom", "refine the backlog", "backlog health", "clean up the backlog", or before sprint planning.
---

# Backlog Groom

`story` means the CLI from the **story-time** skill. Grooming proposes. The
user approves before anything is archived, merged, or split.

## 1. Take the temperature

```bash
story stats --json
story lint --json
story rank --include-refined --json
```

From these, note: open points versus ready points, and how many stories
are unestimated, stale, blocked, or not ready.

**Ready buffer.** Ask for (or read from the user's notes) the team's
velocity. The target is ready points worth 1.5–2 sprints. Below one sprint,
refining the top of the backlog matters more than anything else this
session.

## 2. Triage, in this order

Work top-down by rank, because refining stories nobody will reach is waste.

1. **Stale** (not updated in 30+ days, from `stats.stale`). For each: still
   wanted? Propose *refresh* (it needs work), *archive* (nobody wants it), or
   *keep* (fine, just old).
2. **Duplicates and overlaps.** Compare statements and criteria across open
   stories. Propose *merge* (keep the better id, move the unique criteria
   over, archive the other) or *link* (related, both stay).
3. **Oversized** (`size.small`, `ac.count`). Queue for the **story-split**
   skill.
4. **Unclear** (`statement.*`, `ac.*` errors) near the top of the rank. Queue
   for the **acceptance-criteria** or **story-edit** skill.
5. **Unscored** (`size.estimated`, `value.scored`). Queue a batch for the
   **story-estimate** skill.
6. **Blocked** (`independent.deps`). Either the blocker should rank above
   the stories it blocks, or the dependency can be removed.
7. **Order.** Compare the WSJF rank with what the user believes the
   priority is. Point out any big mismatch: either a score is wrong, or the
   belief is.

## 3. Propose

Present one table, ordered by impact:

| # | Story | Finding | Proposed action | Skill |
|---|---|---|---|---|

Wait for the user to approve rows (all, some, or edited).

## 4. Apply

Do the approved rows. Use `story set <id> status=archived --note "<why>"` for
archives. For merges, edit the survivor, then archive the other with
`--note "merged into ST-xxx"`. Merges and bulk changes are multi-step:
apply them as a saga as described in
[backends.md](../story-time/references/backends.md), so a failure doesn't
leave half a merge.

## 5. Report

Before and after: ready points, not-ready count, stale count. Then the list
of changes made, and anything left for the next session.
