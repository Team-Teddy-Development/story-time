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
The configured WIP limit is enforced conservatively across this repository's local
claims and unknown in-progress stories. Separate clones and other machines do not
share this ledger. A PID must identify the actual persistent agent, not a helper.

Tokens are local bearer capabilities. Do not publish assignment output containing
tokens. Releasing a token neither accepts nor resets its story. Operators retain
the existing tracker workflow and project-specific admission boundaries.
