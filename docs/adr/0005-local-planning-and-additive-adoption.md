# Local planning artifacts and additive backlog adoption

## Status

Accepted for ST-018 and ST-021 implementation planning, 2026-11-12. This decision does not mean either feature is implemented.

## Context

The accepted ST-019 research maps a complete Scrumban lifecycle. Current native push mirrors issues but does not provide a sprint service or general tracker backlog import. Existing story parsing retains extra fields but rendering normalizes source bytes. Rewriting a brownfield backlog through that renderer cannot prove byte preservation.

The owner requires fixed sprints with continuous prioritization and a safe adoption path for existing projects. Missing goals, capacity, and history must remain explicit. Existing settings, tracker identities, and closed work must survive adoption.

## Decision

ST-018 produces version-1 local planning artifacts. Native Rust owns deterministic inventory checks and capacity arithmetic. Skills orchestrate existing story owners. A plan does not start a sprint or update tracker membership. Refinement replaces backlog-groom orchestration while preserving its alias.

ST-021 targets additive local lifecycle metadata version1. Existing stories and settings retain their bytes and authority. Preview is read-only. Apply requires reviewed preimages, a verified non-secret backup, an exclusive lock, a write-ahead journal, and divergence-safe recovery. Adoption v1 does not write tracker issues, labels, or sprint fields. Unsupported tracker-only imports require a concrete handoff instead of a false success.

The implementation contract is [Planning and brownfield implementation contract](../spikes/ST-018-ST-021-implementation-contract.md). ST-021 is split by user outcome into ST-022 read-only preview and ST-023 complete safe apply/recovery. All original criteria remain on the parent and map to the children. Recovery ships with apply, not after it. ST-023 depends on ST-022; both depend on completed ST-019. ST-018 depends on ST-019 and is independent of unfinished adoption code.

## Consequences

Teams can review a useful plan before authoritative sprint state exists. Existing projects can adopt local metadata without a bulk rewrite. The first lifecycle release cannot claim brownfield readiness until planning and both adoption children are accepted.

Active-sprint transitions, observed event history, metrics, and tracker import/write contracts remain later implementation work. They do not inherit a fictional historical baseline from migration. The local-only design avoids cross-tracker rollback promises but does not eliminate local crash/recovery testing.

The aggregate ST-021 estimate13 remains historical after splitting. Current lint still treats that terminal parent as oversized; ST-024 (#56) records the defect. Do not lower the historical estimate to bypass it. Active implementation leaves receive their own readiness checks and retain warnings honestly.
