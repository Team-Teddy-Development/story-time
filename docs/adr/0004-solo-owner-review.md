# Solo-owner review policy

## Status

Accepted by the owner on 2026-11-11. Supersedes the mandatory separate-human review prerequisite for Story-time integration. It does not supersede source acceptance criteria or required CI.

## Context

Team Teddy has one person. PR20's author is the only member of the required review team and the only configured GitHub identity. GitHub rejects self-approval, so the prior code-owner and last-push approval requirements cannot complete. A second model using the same account does not provide a separate GitHub reviewer.

The owner explicitly approved replacing mandatory human approval with recorded Jev review and required CI, followed by normal protected integration when the remaining checks pass.

## Decision

Change only repository ruleset26702417's human-approval requirements and descriptive name. Set the approving-review count to zero, remove mandatory team reviewers, and disable code-owner, last-push, and extra unattributed-change approvals.

Preserve required pull requests, review-thread resolution, stale-review handling, allowed merge methods, branch targeting, deletion protection, force-push protection, bypass actors, and required CI contexts with their integration IDs and strict latest-base policy. Leave the separate story-mandate ruleset27056058 unchanged. Do not use an administrator merge bypass.

Before integration, record the exact PR head, actual typed Jev review and its evidence, and passing checks. Jev's review is an operational record, not a GitHub human approval or an automatically enforced check. A probability is not a substitute for passing tests.

Use a fixed-repository Rust operational MCP tool for the policy mutation. Save the full previous ruleset, the proposed writable payload, and the other ruleset before writing. Reject observed policy drift. Read back the result and compare every unaffected clause. The API does not provide an atomic compare-and-swap guarantee; retain the before/after snapshots for recovery.

## Consequences

The sole owner can integrate reviewed work after CI without an impossible second-person identity. GitHub still enforces the unchanged required checks and pull-request protections. The operator must retain the Jev review record because GitHub does not enforce that record itself.

Restore the saved ruleset through its normal API if this policy must be reverted. Check for intervening changes first. Restoring the former review requirements reinstates the one-person review blocker. The source change can be reverted through another tested PR without force-pushing main.
