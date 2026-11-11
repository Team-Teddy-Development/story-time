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
node skills/story-time/scripts/story.mjs lint ST-001
```

The first compile invocation omitted `--edition=2021` and produced format-string compatibility warnings under Rust's default edition. The corrected command explicitly selects Rust 2021 and denies warnings. No warning suppression or assertion weakening is used.

## Additional recovered policy condition

`es_f1c8734d4d56` records a policy condition thrown through a codemode call to `cleanup_worktrees`. The cleanup itself did not run in that failed batch. The agent then called the cleanup tool directly, inspected the dry-run target list, backed up worktree metadata as required, completed the actual cleanup, and released both owned worktrees. This was a recovered invocation, not a product-code fix. Keep the same ledger API limitation above explicit.
