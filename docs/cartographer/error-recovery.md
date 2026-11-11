# Onboarding error recovery

This record separates corrected tool invocations, a tested story-data repair, and a remaining ledger API limitation. Session follow-up tasks are not code fixes.

## Missing-directory search

Cause: a relative-path `rg` command ran in the Story Time anchor, although its targets belong to Cartographer UI. The owning run then used a standalone search with absolute paths in its Cartographer UI worktree. That search exited zero and returned the registry documentation. The five corrected resolutions were accepted before the worktree was released.

| Fingerprint | Original missing target | Actual correction | Accepted evidence |
| --- | --- | --- | --- |
| `es_ff1169a62cbc` | `docs` | Search the absolute UI `docs` directory | Rerun sequence 56, resolution 57 |
| `es_2d130b9f576a` | `app-rust` | Search the absolute UI `app-rust` directory | Rerun sequence 56, resolution 59 |
| `es_82043d10eb6f` | `tools` | Search the absolute UI `tools` directory | Rerun sequence 56, resolution 60 |
| `es_6b0d7282ccc8` | `contracts` | Search the absolute UI `contracts` directory | Rerun sequence 56, resolution 61 |
| `es_4f60ab90f04a` | `deploy` | Search the absolute UI `deploy` directory | Rerun sequence 56, resolution 62 |

The corrected command starts with `rg -n`, not `cd`, and uses this pattern:

```text
onboard|register.*project|project.*register|CARTOGRAPHER_PROJECTS|project registry
```

Its absolute target prefix was `/home/ejernigan/source/team-teddy/worktrees/runs/55069cf7e69f0c1e4c6f71f8d009d982/`. Its targets were `docs`, `app-rust`, `tools`, `contracts`, and `deploy`. The exact full command is retained in each ledger resolution. The worktree has been released; this path describes historical evidence, not an instruction to access a current directory.

## Correction-recorder invocation

One failed correction-recorder call produced three separately fingerprinted output lines:

| Fingerprint | Symptom | Root cause and actual recovery |
| --- | --- | --- |
| `es_005acf2166ce` | `Script failed` | The correction reference was a compound shell command. The recorder required a directly observed standalone `rg` command. The subsequent absolute-path rerun and five successful resolution calls completed that operation. |
| `es_1685d7625846` | `Script error:` | Same failed recorder call, not another source defect. Recovery is the accepted resolution sequence 57 and 59 through 62. |
| `es_f9326708fe68` | `a later successful corrected rg command must be observed in this session` | The compound `cd ... && rg ... | head` reference did not match the recorder's recognized evidence. A standalone absolute-path `rg` invocation supplied valid evidence, and all five corrections were accepted. |

These are verified operational corrections, not a source-code repair. The old pstack entries only registered follow-up work. They did not supply this distinction or change the original failed invocation into a success.

### Ledger API gap

The installed `error_sentinel_resolve` interface has a `corrected` mode for original missing-path search errors. It has no general operational-recovery mode for a failed call to the correction recorder itself. A `fixed` claim would imply a tested source repair that did not occur. A `filed` call requires a Taskgraph signal, although project tracking has moved to story-time.

Retain the three original failures and their existing follow-up ledger records. Do not rewrite their classifications or claim a source fix. The concrete tool gap is to add a bounded, tested correction-recorder recovery type that witnesses the original failed call, exact later successful correction calls, and original incident fingerprints. That harness implementation is outside this project's onboarding scope. This file is the durable local gap record; it is not an API-accepted filed resolution.

## Missing story scores

`es_22067bfa3b92` came from `story lint ST-001`. The story had neither an estimate nor WSJF inputs, so the CLI could not rank it. Production uncertainty did not require leaving these fields absent: proposed scores can be explicit while the live criterion remains unmet.

The repair uses the CLI to set proposed `size=3`, `value=1`, `time_criticality=1`, and `risk_reduction=1`. The changelog marks them as agent proposals. It does not invent a deadline or claim owner-confirmed business priority. The story remains draft until actual readiness is reviewed.

The Rust regression `tests/cartographer_setup.rs::onboarding_story_has_estimate_and_ranking_inputs` runs the real repository CLI. At test-only commit `c0e2bb6`, it failed on `size.estimated` and `value.scored`. After the scalar-data change, both Rust tests pass and the actual CLI reports `READY (0 errors, 0 warnings)`. A lint readiness result is not a completed story or a live registry admission.

Run from the repository root:

```sh
rustc --edition=2021 -D warnings --test tests/cartographer_setup.rs -o /tmp/story-time-cartographer-setup-tests
/tmp/story-time-cartographer-setup-tests --test-threads=1
node skills/story-time/scripts/story.mjs lint ST-002
```

The maintained command uses the peer-assigned current ID ST-002. The original failed command named ST-001.

The first compile invocation omitted `--edition=2021` and produced format-string compatibility warnings under Rust's default edition. The corrected command explicitly selects Rust 2021 and denies warnings. No warning suppression or assertion weakening is used.

## Additional recovered policy condition

`es_f1c8734d4d56` records a policy condition thrown through a codemode call to `cleanup_worktrees`. The cleanup itself did not run in that failed batch. The agent then called the cleanup tool directly, inspected the dry-run target list, backed up worktree metadata as required, completed the actual cleanup, and released both owned worktrees. This was a recovered invocation, not a product-code fix. Keep the same ledger API limitation above explicit.

## Verification-run findings

The first Rust compile omitted its edition. Fingerprints `es_6d3ff9fe6278`, `es_89384d419d1f`, `es_327c3989cf70`, `es_b0e4e82ee354`, `es_77aae6c58579`, `es_42bef1287083`, and `es_edd61f14c974` capture the resulting format-compatibility warnings and compiler suggestions. The maintained command and CI select `--edition=2021 -D warnings`; the final compile emits no warnings. This is a corrected compiler invocation, not a language change or suppressed warning.

The test-first RED run deliberately exposed the missing story scores. Fingerprints `es_395b864c66ad`, `es_b0f11e7f79c0`, `es_e4f0d278e848`, and `es_57e025fda7d8` describe that same expected failing regression before the data repair. The preserved test-only commit and GREEN rerun above prove the repair; these are not independent unresolved product failures.

Clippy then rejected an explicit `if request.get("id").is_none() { return None; }` notification guard in the new MCP dispatcher. Fingerprints `es_2f2858fea169`, `es_eee138ef682e`, `es_95947c43607c`, and `es_6914e3154885` describe that one source issue and its build summaries. The source now uses `request.get("id")?;`. The existing real stdio regression asserts that the notification produces no response. The final Clippy run with all targets and warnings denied passes, and all 14 tool tests pass. No lint rule is disabled.

A later typed `fixed` submission for the historical CLI lint incident was refused because the original absolute command did not name the new regression test. The tested data repair remains valid; the recorder's restricted proof path cannot represent it. This is another instance of the ledger API gap above, not evidence that the story fields are still missing.

## Concurrent source reconciliation

Before this run pushed, PR #15 moved to `fa0f39f8b47d9ed579de9c6b6d89aea135c3fb68`. It had merged PR #14 from main and separated the conflicting story IDs: GitHub dogfooding is `ST-001`; Cartographer onboarding is `ST-002`. The merge of that exact head into this run found four conflicts in the CI file, ST-001, README, and admission evidence. The resolution preserves incoming ST-001 and GitHub settings byte-for-byte, keeps both Rust CI jobs, and transfers only onboarding notes and proposed scores to ST-002. Historical fingerprints, original names, and RED/GREEN commits remain intact.

The current real CLI regression asserted an expected RED result for unscored ST-002 while the GitHub configuration assertion passed. After transferring the proposed scores, both tests passed. This continues the same onboarding repair; the peer-assigned ID does not erase history.

The mutation experiment found a test gap: aggregate report status did not detect an inverted replica predicate because other prerequisites still failed. The test now asserts the v2 clause and all other clauses independently. The inverted predicate then produced the asserted exit 101, and restored source passes the full Rust suite.

Two exact-edit failures used an old-text typo and a sentence from a different document. Reading and matching the actual target text corrected both operations. A malformed tool-batch object also failed before any tool call; the corrected object executed the edits. These are recovered authoring invocations, not product-code fixes.

Current-run authoring fingerprints are `es_b176e4e83a8d`, `es_e7804d004982`, `es_7b97edcbdbdc`, and `es_d702fbba2273`. `es_2111284876f1` is the anticipated merge conflict, resolved in merge commit 013c81e with the incoming protected files unchanged and all CI contexts passing. `es_b67a62cdcf53` is the first mutation guard's empty-output failure when the mutant survived; the strengthened per-clause regression removes that coverage gap. `es_b13856c6bbd2` is the expected unscored draft CRT-052 intake warning; core grooming owns it, and it is not a passed readiness gate.

`es_238735685282` requested cursor zero for an oversized staged diff. Every chunk of staged tree c44f212811a23708c139333448fa980f1097b0d8 was then read through completion. The later typed resolution attempt returned `no matching completed staged-diff cursor chain in this branch`; it did not recognize the nested tool calls. This records the exact API limitation observed, not a claim that the staged content was unreviewed or that the global harness was fixed.

The installed GitHub CLI also exposed two compatibility limits. `es_69109bc96512` is its deprecated GraphQL projectCards query during `gh pr edit`; the operation was corrected with a scoped REST PATCH containing only the PR body. `es_fa5da842dc96` is the unsupported `gh pr checks --json` invocation; supported `gh pr view --json statusCheckRollup` returned the exact head and successful checks. These are corrected operations, not patches to the external GitHub CLI. Preserve the failed commands as history; no global tool installation was changed.

The remaining verification-API gap is also recorded in the available project tracker: https://github.com/Team-Teddy-Development/story-time/issues/19#issuecomment-6805784414. This is a durable gap handoff, not a source fix or an API-accepted sentinel resolution.
