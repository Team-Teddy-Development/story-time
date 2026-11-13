# ST-018 independent review

## Verdict

BLOCKED. One source-preservation defect requires a fix before acceptance.

This is an independent static review of the nine supplied files. I read planning.rs through its end, planning_contract.rs through its end, planning_install.rs through its end, the story, the shared implementation contract, both skill entry points, the planning reference, and the request schema. I ran no shell commands, searches, tests, installation, or source mutations. Reproduction instructions below are a proposed regression case, not an executed result. The parent owns runtime verification and completion gates.

## Required finding

### B1. A valid configured backlog can overlap the generated Markdown output

**Severity:** High. Blocks merge because a successful planning invocation can overwrite an authoritative story.

**Locations:** `native/story-push/src/planning.rs:330-380` honors the configured backlog directory and inventories its Markdown files. `planning.rs:732-746` accepts any safe JSON target under `docs/planning/` and derives its Markdown counterpart. `planning.rs:748-790` accepts exact reviewed output preimages. `planning.rs:792-879` publishes without excluding source paths.

The output guard checks containment, symlinks, file types, and reviewed preimages. It does not check whether either target is a source file. The source recheck occurs before publication. Therefore, matching preimages authorize replacement even when the Markdown target is an inventoried story.

**Deterministic reproduction for the parent, in a disposable fixture:**

1. Set configuration to `{"tracker":"files","sync":"off","prefix":"PAY","backlogDir":"docs/planning"}`. Create valid source story `docs/planning/PAY-001.md`, with ID PAY-001, status ready, and size 3.
2. Capture a request through `story plan --snapshot`. Keep the request outside the configured backlog. Preserve the captured source manifest.
3. Set `output_preimages` to `{"docs/planning/PAY-001.json":null,"docs/planning/PAY-001.md":"<exact SHA-256 of the source story>"}`. No capacity or goal is necessary; a provisional inventory is sufficient.
4. Invoke `story plan --from <request-path> --output docs/planning/PAY-001.json`. The current control flow accepts the preimages, passes source freshness, and renames generated Markdown over PAY-001.md.
5. Assert the required behavior instead: exit 2 with a source/output collision diagnostic, original story/config bytes unchanged, and no JSON output created.

A `.map.md` source can overlap in the same way, using an output name ending `.map.json`. The defect is not a path escape or a stale-preimage case. All paths can be contained, regular, current, and explicitly reviewed. Output replacement evidence must not authorize backlog mutation.

**Smallest fix:** Before creating directories, locks, or stage files, reject either output target if it lies within the configured backlog directory or equals a captured source/config path. Recheck that boundary under the publication lock. Keep reviewed output replacement available only for non-source artifacts. Add Rust process regression coverage for a story collision and a map-file collision, preserving all project bytes on rejection.

**Typed review:** `jev_cite` judged the source-preservation claim supported, with supported probability 0.86 and verdict confidence 0.88. This is a static control-flow conclusion, not runtime proof.

## Seven-criterion review

| Criterion | Static verdict | Evidence and limits |
| --- | --- | --- |
| AC1. Complete open-story dispositions and epic evidence | Supported | `Status::open` excludes done, archived, and split. Candidate uniqueness is validated. The planner appends omitted open stories and creates one row per open ID. Selection and epic disposition remain separate. Missing epic evidence produces an explicit question or a request to confirm the existing source link. `mixed_inventory_capacity_completed_dependencies_and_epics_are_separate` and `empty_and_legacy_sources_and_missing_configuration_remain_read_only` cover mixed and empty inventories. |
| AC2. Goal, capacity arithmetic, dependency order, and deferred work | Supported | Candidate order remains caller-supplied. Only ready stories with known positive size and already-done prerequisites are eligible. Reserve is deducted before selection. The budget8/reserve1/3+3 case asserts total6 and remaining1. Unfinished dependencies stay blocked even when another candidate is selected. `dependency_order` duplicates selected order; that is sufficient here because selected stories cannot depend on unfinished selected peers. Forecasts are explicitly noncommittal, undated, and disjoint from selected IDs. |
| AC3. Provisional missing-input and unsupported-publication behavior | Supported | Missing goal, capacity, calendar fields, configuration, required ready-story size, or supplied eligibility evidence creates named missing inputs. Missing capacity selects nothing; zero capacity is valid. Remote trackers always receive a named unsupported sprint-publication reason. Publication records local-only mode and an empty write list. Skills distinguish availability, velocity, duration, and point budgets and do not fabricate runtime inputs. |
| AC4. Installed discoverability and metadata | Supported by inspected test design; execution not verified here | Both supplied SKILL.md files have matching names and descriptions, reference the shipped planning contract, and document the same native snapshot/stdout/output-pair interface. `planning_install.rs:57-206` invokes the retained real installer in a temporary HOME, examines both agent trees, checks router/README/expected-skill entries and metadata, and exercises documented native commands. Router files, metadata files, README, and installer source were outside the permitted direct-read list; this review does not independently attest their contents or a passing installation run. |
| AC5. Fail-closed malformed input and unchanged state | BLOCKED by B1 | Typed request structs reject unknown fields. Validation handles versions, duplicates, cycles, invalid capacity/calendar values, and stale source identity/content/membership. Existing process cases compare complete project snapshots on failing paths and check both exact output preimages. However, a valid output pair can overlap source Markdown and succeed while replacing it. The planned source-preservation boundary is therefore incomplete. |
| AC6. One refinement owner, alias, and estimate preservation | Supported within inspected scope | Single-story and backlog modes share the same inventory and routing instructions in refinement/SKILL.md. Existing sizes are preserved unless re-estimation is requested. The planner reads sizes from hashed sources and has no estimate-write path. The installation test asserts the old alias routes to refinement and contains no second planner command. The alias itself was not supplied for direct inspection; its executed installation check remains parent-owned. |
| AC7. Rust implementation and real entry-path regressions | Supported for inspected executable language; completion evidence outstanding | New planning logic and both supplied test files are Rust. The installation test invokes the unchanged Node installer as an explicitly retained compatibility interface, not a new planning implementation. It copies the built native test binary into a temporary installation and drives documented CLI commands. I did not inspect dispatch or package files outside the read list, run Cargo, inspect regression logs, or establish full repository Rust conversion status. The parent must supply full native/Node/policy/fmt/Clippy and installation results before acceptance. |

## Output-safety review

**Implemented protections.** Output paths must be repository-relative JSON files under docs/planning. Existing symlink components and nonregular targets fail. Both output preimages are required when either output exists. A per-directory create-new lock serializes cooperating publishers. Publication rechecks source and output preimages under that lock and stages both payloads before renaming either. The origin URL is hashed before it enters artifacts. Settings contents are not emitted.

**Remaining blocker.** These protections do not separate output artifacts from authoritative backlog inputs. B1 defeats the stated no-backlog-writes property without bypassing any current guard.

**Declared limitation, not an additional blocker.** Two-file publication is not crash-atomic. A failed second rename attempts restoration of the first target; restoration can itself fail. The reference explicitly discloses this limitation and requires inspection after output I/O or cleanup failures. The error JSON does not falsely promise rollback after every publication error. The supplied contract tests cover predictable conflicts, but do not demonstrate a forced second-rename failure or crash recovery. Do not report those unexecuted paths as verified.

**Rendering boundary.** Markdown embeds caller-supplied goal, identifier, reasons, and evidence. It is a human view of reviewed planning inputs, not an HTML sanitizer or an executable file. No additional sanitization guarantee is established by this review.

## Review method and handoff

The review covers correctness against the seven criteria, source/output safety, orchestration ownership, language scope, and the supplied process/install tests. No cosmetic changes or unrelated architectural work are requested. An active mutation experiment would violate the delegated no-source-edit boundary, so none was attempted.

Fix B1 and add the two source-overlap process cases. Then run the parent-owned Rust process/install tests and required repository regressions. Recheck the output boundary against the unchanged story/config bytes before accepting ST-018. This review does not authorize tracker writes, source edits, a merge, or sprint execution.
