# Planning and brownfield implementation contract

ST-018 (#48) and ST-021 (#52) share this target contract. This is design, not installed behavior. ST-019 research is the evidence base. The owner requested refinement, not product implementation in this run.

## Existing interfaces and ownership

`skills/story-time/references/story-format.md` defines flat Markdown stories with scalar and inline-list frontmatter. `native/story-push/src/story.rs::parse` supports additional valid keys and named sections, but normalizes whitespace and comments when rendering. Migration must retain original bytes instead of parse-and-render round trips.

`native/story-push/src/lib.rs::context` reads current or legacy settings and resolves the configured backlog. Its `Tracker` port provides create, update, and remove. It does not provide an authoritative sprint service or general tracker backlog import. Existing push supports issue mirroring, not Jira-like sprint operations. Configuration and the existing backlog remain authoritative for their current fields.

`story-map` owns epic outcomes and links. `story-edit` owns individual changes. `refinement` replaces backlog-groom orchestration and keeps backlog-groom as an alias. `sprint-planning` proposes feasible work. `story-setup` owns new/adopt/upgrade routing. No second setup or epic service is introduced.

## ST-018 delivery boundary

The first slice produces a read-only inventory and a planning artifact. It does not start sprints, change story status, assign tracker sprint fields, or write authoritative execution history. Active-sprint transitions, scope-change execution, carryover, release, and metrics remain later research slices.

New deterministic inventory, validation, capacity arithmetic, and artifact generation use Rust in the existing native CLI crate. Skills provide orchestration and invoke existing story owners. The retained Node CLI and installer are unchanged external compatibility interfaces. Updating static expected-skill names or package metadata is data, not replacement executable logic.

### Planning artifact version 1

Use JSON for the machine-readable artifact and Markdown for its human view, under an explicitly selected `docs/planning/` path. The proposed native entry point is `story plan --from <request.json> --output <plan.json>`; it is not available yet. With no output option, emit the result on stdout only. Never overwrite an existing plan without an exact reviewed preimage.

| Field | Contract |
| --- | --- |
| `schema_version` | Integer 1. Independent of package version 0.1.0 and local lifecycle metadata version. Unknown future versions are rejected before writes. |
| `plan_id` | Explicit nonempty identifier, scoped to the repository. It is not an active sprint ID. |
| `source` | Repository identity, configuration location and digest, backlog file digests, and capture time. Capture time describes observation, not past delivery. |
| `goal` | Supplied goal or null with a named missing-input reason. Do not infer a product goal from story titles. |
| `calendar` | Configured sprintDays plus supplied start/end/time zone, or null missing fields. Ten working days alone cannot establish dates or holidays. |
| `capacity` | Supplied point budget and reserve with their source, or null. Velocity, availability, and story points are not interchangeable units. |
| `dispositions` | Exactly one row for every input open-story ID, including selected, deferred, blocked, needs-refinement, justified-standalone, or unresolved-epic evidence. Selection and epic disposition are separate fields. |
| `selected` | Unique ready IDs whose required dependencies are done. Include total known points, dependency evidence, exclusions, and WIP policy. Unknown size excludes commitment. |
| `assessment` | `provisional` if goal, calendar, capacity, source freshness, or required size/permission evidence is missing; otherwise `planned`. Neither starts a sprint. |
| `forecasts` | Optional later groupings explicitly marked forecast, without promised dates or automatic assignments. |
| `publication` | Capability results and unsupported operations. Local plan generation never synchronizes a tracker. |

Capacity uses one explicit unit. For a supplied point budget, selected points plus reserve must not exceed that budget. Unknown capacity produces a provisional result with no commitment. Zero capacity selects no work. A negative budget, reserve above budget, duplicate ID, cycle, malformed input, or unknown schema produces a diagnostic with no output-file replacement. Empty backlogs produce an empty inventory, not a fabricated goal.

Refinement inventories all open stories but deepens near-term work only. Each input ID appears once. It preserves existing estimates unless re-estimation is explicitly requested. An unresolved epic is a visible disposition, not an invented epic. Readiness checks remain the existing story-ready responsibility. Sprint planning consumes the resulting ready, ordered candidates rather than recreating those decisions.

WIP remains the configured per-person pull limit. It is not a cap on total planned stories. Missing people or availability stays unknown. A plan cannot silently change the fixed-sprint goal or authorize active-sprint additions.

### ST-018 verification contract

Future Rust process tests live in `native/story-push/tests/planning_contract.rs`. They exercise the built binary, not only helper functions. The exact target does not exist before implementation.

| Criteria | Planned test cases and observable result |
| --- | --- |
| AC1 | Mixed ready/draft/blocked/standalone/unmapped fixture, empty backlog, duplicate ID. Inventory equals the input ID set exactly once; unresolved mappings remain visible. |
| AC2 | Budget 8, reserve 1, selected sizes 3 and 3 yields total 6 and remaining 1. Size 8 cannot join. Unfinished prerequisites are excluded with their IDs. |
| AC3 | Missing goal, calendar, size, capacity, or tracker capability gives provisional output and no assignment. Future groups remain forecasts. |
| AC4 | Installed temporary skill tree exposes refinement and sprint-planning, alias routes to refinement, metadata/router/README/package lists agree. No new runtime dependency is hidden in the installer. |
| AC5 | Malformed JSON, unknown version, negative or excessive capacity, cycle, stale preimage, and attempted overwrite leave source/config/tracker bytes unchanged and do not replace output. |
| AC6 | Both refinement modes and the old backlog-groom entry point produce one orchestration path, not independent ranking or mutation logic. |
| AC7 | Native contract tests, retained Node regressions, Rust policy suites, fmt and strict Clippy pass. Scoped Rust audit names every new executable and test. |

After implementation, run `cargo test --manifest-path native/story-push/Cargo.toml --test planning_contract`, the configured regression command, and installation checks in a temporary agent home. Do not modify global installed skills for a planning test.

## ST-021 source and target versions

Adoption targets local lifecycle metadata version 1 and planning artifact version 1. It does not change package version, reinterpret completed work, or enable later sprint execution features.

| Source | Preview | Apply |
| --- | --- | --- |
| Existing flat story files and current `.story-time/settings.json` | Read originals, custom keys/sections, configured prefix/backlog path, sync policy and links. Treat absent lifecycle metadata as version 0. | Add lifecycle metadata and migration receipt. Retain all existing story and settings bytes unless a reviewed mapping names an exact change. |
| Existing flat story files and legacy `.story-time.json` | Record the actual legacy config location. Do not create a competing settings file. | Preserve that location and its bytes; metadata references it. |
| Existing backlog without settings | Inventory without assuming tracker or prefix. Conflicting identity/config choices are explicit. | Requires reviewed settings inputs; initialize only missing settings through native init, never force-replace existing configuration. |
| Metadata version 1 with matching preserved preimages | Report already adopted and the verified version. | No changes and no duplicate receipt, issue, or label. |
| Future/unknown version, nested or malformed story format, duplicate identities, path escape, or local/remote conflict | Preserve input and emit a conflict with exact path/identity. | No apply until a supported, reviewed mapping exists. Do not stringify unknown nested data into the flat format. |
| Tracker-only backlog without supported read/import contract | Report tracker-only source and required export/import capability. Retain remote IDs in preview evidence. | Block with a concrete export/import handoff. Existing issue-write adapters do not count as import support. |

Tracker-only support in this first adoption contract is a diagnosed handoff, not a false successful migration. A future import capability requires its own acceptance evidence. A provided export remains immutable source evidence until a reviewed mapping can preserve its identities and custom data. Do not create parallel issues to demonstrate adoption.

### Preservation and capabilities

Write new metadata to `.story-time/lifecycle.json` containing `schema_version: 1`, the actual config path, the selected backlog path/prefix, planning schema version 1, explicit unknown-history fields, and a migration ID. Keep credentials, tokens, environment contents, and signing keys out of metadata, backups, and reports. Store no synthetic sprint dates, velocity, or state-transition history.

Use one canonical repository root. Reject paths outside the reviewed root and symlink traversal before writes. Preserve repository-specific paths that cannot safely be handled as an explicit conflict rather than silently relocating them.

| Tracker | Existing interface | Adoption v1 effect |
| --- | --- | --- |
| Files | Local Markdown and settings | Local metadata/backup/receipt only. Preserve off. |
| GitHub, Gitea, ADO | Existing issue create/update/remove via native push | Preserve links and sync auto/ask/off exactly. No issue, label, milestone, sprint, or iteration write during adoption. |
| Jira, Linear | Existing manual/off configuration | Preserve manual/off and report unsupported automated import/sprint publication. |

Do not test credentials by printing them. A missing service prevents only the operation requiring it. Local read-only preview and local migration do not require tracker availability. Source-versus-tracker conflicts block apply; neither side wins automatically.

### Reviewed plan, backup, and journal

The proposed native command family is `story adopt preview|apply|verify|rollback`; these commands are not installed yet. Preview writes nothing in project or tracker state and returns its JSON on stdout. A caller may save that output to an explicit file. Bind apply to a reviewed plan digest, repository identity, full preimage digests, supported source/target versions, and an explicit list of allowed writes.

Before the first mutation, store a mode-600 verified byte backup and manifest outside the repository and outside all mutation targets. Back up only reviewed non-secret inputs. If a credential-like or unknown sensitive file enters the write set, refuse it rather than copy it into evidence. Confirm backup bytes against source digests and test restore readability before apply.

Persist an ordered write-ahead journal outside the mutation set. A step records its path, expected preimage, planned postimage, completion state, and error. Use temporary files plus rename for local replacements. Acquire a repository-scoped exclusive lock and recheck every preimage under the lock. Interrupted or concurrent runs never infer completion from a marker alone.

Recovery uses the same plan and journal. If current bytes equal the recorded postimage, recognize the completed step. If they equal the preimage, the step is still pending. Otherwise stop with a conflict. Rollback restores only journaled paths whose current bytes match the known postimage, in reverse order; remove newly created paths only when the journal proves this run created them. Preserve unrelated edits and all historical failures. Failed restoration remains a failure with a ready-to-run recovery action.

Adoption v1 performs no external writes, so it promises no cross-tracker atomic rollback. A future tracker-writing version must declare compensation per adapter and cannot inherit this local guarantee. Verify preserved story/config hashes, identities, membership, and receipt consistency before reporting success. A repeated successful apply verifies and exits unchanged, including unchanged receipt identity.

### ST-021 verification contract

Future Rust process tests live in `native/story-push/tests/adoption_contract.rs`. Fault injection is confined to disposable fixture repositories and journals.

| Criteria | Planned test cases and observable result |
| --- | --- |
| AC1 | Unversioned/current/legacy config, custom prefix/path, unknown target version, duplicate IDs, files-only and tracker-only sources. Preview leaves all input/project/tracker bytes unchanged. |
| AC2 | Non-ASCII/custom fields/sections, closed/archived stories, explicit sync modes and custom Ready/Done rules. Apply preserves original bytes, links and history; backup readability and digest equality are asserted. |
| AC3 | Apply twice and verify twice. Second run changes no file, receipt identity, issue, or label; tracker write count is zero for both. |
| AC4 | Crash before/after each journaled write, stale preimage, concurrent invocation, symlink escape, lost lock and divergent rollback bytes. Assert bounded recovery, preserved unrelated edits, and truthful conflicts. |
| AC5 | Missing goals/capacity/epics/history stays null with reasons. Accepted and archived work stays terminal and is not forced through new readiness gates. |
| AC6 | Setup new/adopt/upgrade routing uses one native contract. Fresh init retains its existing behavior. Installed temporary skill and actual native executable pass; all new migration/test code is Rust. |

Run `cargo test --manifest-path native/story-push/Cargo.toml --test adoption_contract`, full native tests, fmt, strict Clippy, the configured repository tests, and temporary installation checks after implementation. The target does not exist yet. No test or future command in this document is reported as having run.

## Dependency and release order

Both stories depend on completed ST-019 research and this recorded contract. ST-021 does not depend on unfinished ST-018 code: adoption v1 adds independent local metadata and references the agreed planning schema. ST-018 can generate plan artifacts without adopting or rewriting existing projects. This avoids a dependency cycle and permits independent implementation.

The lifecycle version cannot be advertised as brownfield-ready until both stories are accepted. WIP remains 1. Ready stories can wait in the queue; readiness does not start either implementation. Later active-sprint/event/metrics work must consume explicit observed history, not reconstruct history during adoption.

## Decisions and alternatives

Choose local file artifacts for the planning slice rather than a sprint database or tracker assignments. That preserves the original skill outcome without inventing native sprint APIs. Choose additive metadata and byte-preserving originals for adoption rather than rewriting every story through a lossy renderer. Reject automatic tracker-only import where the read contract is absent. These are reversible implementation boundaries, not new product goals or delivery dates.
