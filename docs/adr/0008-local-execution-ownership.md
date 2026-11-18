# Local execution ownership beside story status

## Status

Implemented for ST-168. Local Linux CLI scope only.

## Context

The loop timer repeats prompts and owns no backlog state. Story selection treated
in-progress status as execution ownership. Loop 1 reported 83 in-progress stories
against WIP 1, with no active story assigned to that coordinator. A status count
cannot establish whether a worker is alive, dead, or unknown. No existing runtime
lease was found in that loop's coordinator record.

## Decision

Extend the native Story Time CLI beside ranking. Share an assignment ledger and
kernel-backed lock through the Git common directory. Reuse ranking and readiness
lint. Identify the owner by machine, boot, PID, and process start time. Keep
lifecycle status independent of ownership. Recover only a verified terminated
owner, preserve its work location, and fence its token before another claim.

Do not change the timer, build another scheduler, or bypass Cartographer intake
and admission. Protect unknown legacy work. No claim of automatic reconciliation
of old in-progress records is made.

## Consequences

Cooperating local agents can serialize claims across linked worktrees and resume
abandoned recorded work. Legacy ownership requires one evidence-backed migration.
The original implementation counted local claims and unknown in-progress stories
against WIP. The 2026-11-18 correction below replaces that global status gate. Separate clones and other machines do not
share this ledger. A PID must identify the actual persistent agent, not a helper.

## Correction: per-story quarantine, 2026-11-18

The live coordinator already consumed `assignment next`, but 242 bare legacy
statuses still blocked every assignment. This repeated the original ownership
error. The owner's active-evidence requirement applies to execution capacity,
not merely the presence of an in-progress status.

Keep unknown legacy IDs quarantined across linked worktrees. Never select them
for execution or preparation, change their statuses, or infer abandonment. Count
recorded reservations, including uncertain owners, against WIP. Keep verified
dead-owner recovery ahead of unrelated selection. When a free slot has no ready
candidate, nominate one existing-ranked, unblocked preparation candidate without
a claim, or name missing prerequisites. Preparation still needs coordinator
ownership and repository admission; unknown velocity is not an execution gate.

This replaces the global legacy-status blocker, not the ledger, token fencing,
readiness checks, or project admission. Jev selected per-story quarantine with
confidence 1.0 and supported its fit to ST-168's original intent at probability
0.90. Original criteria and estimates remain unchanged.

Tokens are local bearer capabilities. Do not publish assignment output containing
tokens. Releasing a token neither accepts nor resets its story. Operators retain
the existing tracker workflow and project-specific admission boundaries.
