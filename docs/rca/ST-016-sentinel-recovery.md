# Six-fingerprint recovery

## Scope and decisions

The owner requested corrections for six specific error-sentinel records. These records contain three causes: a real command-help defect, an operator-selected nonexistent ID, and four messages from one stale title assertion.

Fix Root Causes led to intercepting native help before legacy parsing, rather than suppressing its failure. Model the Domain kept the help request as the exact argument pair `new --help` or `new -h`; ordinary creation remains unchanged. Laziness Protocol ruled out rewriting story allocation for an operator mistake. Prove It Works required real binary execution and actual returned-ID selection.

## Source and runtime evidence

ST-015 source commit `17ccee6` corrected the intended tracker-title contract and Gitea payload assertions. Evidence commit `ff397a7addc311e068c09ca3d9c4c1cf2d957085` passed all observed CI checks. PR #40 merged as `01d56defc7ff102dc52069ed50a11c6fd91005a9`.

ST-016 test-only commit `709dc5696aad59cf48952f119609280df7650d03` reproduces `Flag --help needs a value.` through the actual native binary with a valid retained runtime. The separate source-only commit `3c50bc1feeed68a69572f6239fdf2ee8be8fb001` prints native help before any legacy lookup. All three Rust recovery tests pass after that fix. The full native suite passes 44 tests, including the changed-title contract tests.

The returned-ID test creates a story after fixture `REC-067`, parses the actual JSON `id`, and passes that value to native `push --dry-run`. It confirms lookup succeeds and writes no remote linkage. The original guessed-ID invocation remains invalid; this is operational recovery, not a lookup implementation fix.

## Exact fingerprint dispositions

All six `fixed` submissions were rejected with `verification test is not present in the original absolute failing command`. The installed replay code recognizes selected Node/Vitest/compiler invocations, not these original Cargo and CLI commands. No ledger was edited, no Node proxy was added, and no historical evidence was replaced.

Each fingerprint then received a confirmed filed-gap resolution from `error_sentinel_resolve`. These replace the session follow-up-task clearances. They are not verified-source-fix dispositions.

| Fingerprint | Cause and correction | Verified gap signal |
| --- | --- | --- |
| `es_20de177785ff` | Old-title assertion; intended-title contract corrected in `17ccee6`. | `signal_39d75dd8-2476-4e46-a241-47671d54f823` |
| `es_21f28dc6f4ac` | Assumed ST-018; corrected invocation uses returned ST-015. Rust integration coverage is in `709dc56`. | `signal_0fb5eec9-1a7b-4e55-bb6a-3013c3546794` |
| `es_4f19cee7e7f7` | Named Gitea test from the same old-title assertion; corrected create/update checks in `17ccee6`. | `signal_af18f58c-b5bb-4d0d-82b0-4ff49885a8dc` |
| `es_972e3c265700` | Cargo failure footer from that same run; corrected native suite passes. | `signal_5124be92-1e47-4943-8734-3e7d9fac4ce6` |
| `es_a8170328dbb7` | Native new-help dispatch defect; exact RED at `709dc56`, source GREEN at `3c50bc1`. | `signal_3546d28f-3a8d-45fa-b3c7-6e117de4e063` |
| `es_d5c94fdf2b7b` | 19-pass/1-fail summary from the same Gitea run; corrected suite passes. | `signal_4411d630-38f5-48c3-90f9-e98c8afa16b2` |

Project: `project:Team-Teddy-Development/story-time`.
All six signals attach to the existing prerequisite `goal_bacc9b7c-255d-4008-b0b8-b561b47fb6e1:task:historical-evidence`, which remains Blocked. Those earlier typed gap filings have `resolveKind: filed` and that exact task reference. The later original-host bounded acceptances below preserve them. Source corrections and passing tests are linked in each signal excerpt.

## Delivered source and installed path

PR #42 merged at `6dfbd92a85b546824342b75435a562d926f51db4` after all nine observed CI checks passed. `cargo install --path native/story-push --locked --root /home/ejernigan/.local --force` exited zero and replaced the installed native CLI. Both `env -i PATH=/nonexistent /home/ejernigan/.local/bin/story new --help` and `new -h` print usage and exit zero. The 3 Rust recovery tests pass after installation. The previous executable and Cargo install metadata are preserved in `/tmp/story-native-before-ST016.tar.gz`.

Initial delivery recorded G1 through G5 met and G6 unmet. Original-host persistence now satisfies G6; all six gates have evidence. Runtime gates carry the unlazy checker's output digests and definition-bound evidence in ST-016-gates.md. The initial G4/G5 reviews did not establish the then-unmet G6. Current G6 acceptance has separate original-journal evidence.

## Original-host bounded acceptance

Dotfiles PR101 delivered the Rust verifier and preserved installed cursor/ST-002 behavior. The current original host executes that installed implementation. Its registered resolver accepts each exact request with `kind=historical_regression` and pinned ref `3c50bc1feeed68a69572f6239fdf2ee8be8fb001`. Enforced status confirms the same persisted records.

| Fingerprint | Verification kind and outcome | Original sequence | Independent confidence |
| --- | --- | --- | --- |
| `es_20de177785ff` | superseded_test; contract-correction | 636 | .96 |
| `es_21f28dc6f4ac` | operational_disposition; invocation-recovery | 638 | .90 |
| `es_4f19cee7e7f7` | superseded_test; contract-correction | 639 | .94 |
| `es_972e3c265700` | superseded_test; contract-correction | 640 | .96 |
| `es_a8170328dbb7` | historical_regression; help-source-repair-revalidated | 641 | .88 |
| `es_d5c94fdf2b7b` | superseded_test; contract-correction | 642 | .94 |

`docs/rca/ST016-original-host-persistence.json` contains the bounded receipt. Four Rust tests read the original journal without writing it. They verify all six current dispositions, prior filing sequences 185–190, and the unchanged sealed-prefix SHA-256 `3875c96d2f5861a95b5bc94154f8bddfbc94bee78c42d7a54a8d7b8c57d4e309`. Negative controls reject filed-only context, a missing latest disposition, confidence .79, and a false source-fix claim. The independent floor remains .80.

G6's independent Jev citation is supported with probability .85 and verdict confidence .80. This accepts the named bounded classifications, not historical source-fix identity. No tool-driven reload execution or second journal writer is claimed. No unchanged semantic refusal was retried.

## Historical source-fix limit

All six new `sourceFixAccepted` and `historicalPreimageProved` fields remain false. Historical dirty source, binary identity, HEAD, and missing numeric exits remain unknown. Original mutating pushes were not replayed. The obsolete title expectations and guessed-ID invocation remain invalid.

The older five-fingerprint Taskgraph prerequisite remains Blocked with AC0 and AC2 unmet. Its original-source and completion-contract obligations are not replaced by these six ST-016 dispositions. Preserve its history and the separate ST-002 criteria and state. Do not retry these six accepted requests or infer historical source identity to close that separate prerequisite.
