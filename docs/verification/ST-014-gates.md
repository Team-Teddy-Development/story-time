# ST-014 verification gates

Scope is native init and its Rust CLI regressions. Push and retained config remain unchanged compatibility interfaces.

| Gate | Required evidence | State before implementation |
| --- | --- | --- |
| AC1 | External path with Node available fails for missing metadata on original source; canonical control succeeds; fixed CLI writes normalized settings and retained config reads the same coordinates | UNMET |
| AC2 | Defaults, canonical/custom roots, flags, environment precedence, relative paths, overwrite refusal/force, malformed existing settings, ignore preservation, tracker normalization | UNMET |
| AC3 | Invalid JSON/settings return diagnostics without changing new/existing targets or metadata, including force | UNMET |
| AC4 | All init dispatch precedes legacy discovery; CLI runs with PATH=/nonexistent; installed-binary contracts await root delivery | UNMET |
| Rust conversion | Native init implementation and new regression tests are Rust; no Node init dispatch | UNMET |

## Execution chain and languages

- `native/story-push/src/main.rs`: Rust executable dispatch; init conversion pending.
- `native/story-push/src/init.rs`: planned Rust init argument boundary, normalization, and filesystem writes.
- `native/story-push/tests/init_contract.rs`: planned Rust real-process regressions; supports `STORY_TIME_TEST_BIN`.
- Retained `story.mjs` init/config and `config.mjs`: unchanged JavaScript history and config compatibility interface. Init must no longer call them. Other commands remain out of scope.
- README, AGENTS, story, and this file are documentation/data, not executable implementations.

## Test sequence

Commit test-only RED before source edits. Use `/home/ejernigan/.cache/story-issue37-target` for Cargo output. Retain the original runtime failure here. Run Cargo tests, fmt, clippy, npm tests, Rust policy tests, and Rust policy contracts after implementation.

Throughput checkpoint: one delegate owns this scope. No implementation fan-out or alternate designs are needed for the confirmed directory-creation defect. The request model carries source, target, force, and output mode. Normalization precedes filesystem effects. Generalized transactions are excluded.

## Original RED runtime

`CARGO_TARGET_DIR=/home/ejernigan/.cache/story-issue37-target cargo test --manifest-path native/story-push/Cargo.toml --test init_contract -- --nocapture` compiled and ran eight Rust CLI tests. Result: `1 passed; 7 failed`. Full original output is `/tmp/story-issue37-red.log`.

The canonical control passed. External settings with Node available failed with `exit Some(1): ENOENT: no such file or directory, open '/tmp/story-init-3556283-6/.story-time/.gitignore'`. Node-free tests separately failed with `Non-push commands require STORY_TIME_LEGACY_CLI or ~/.config/story-time/runtime.json with legacyCli.` These are the intended RED assertions, not compilation failures.

## GREEN verification

The current source passes all 39 Cargo tests: 19 library tests, 9 existing push CLI tests, 8 init CLI contracts, and 3 existing setup contracts. `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` pass with the same manifest and external target directory. `npm test` passes all 408 tests. The Rust policy and policy-contract binaries pass 12 and 11 tests respectively. No Node source or tests changed.

Current gate states are AC1 MET, AC2 MET, AC3 MET, AC4 UNMET pending installed-command verification, and Rust conversion MET for the approved init implementation scope. Acceptance criteria remain unticked pending root acceptance. All init arguments, including help, route through Rust before legacy discovery. `normalize` fills setup defaults and removes unused nested fields before shared validation. Only after valid settings exist does init create both the settings parent and root metadata directory.

Final language audit: `main.rs`, `init.rs`, shared validation in `lib.rs`, and new process-level regressions in `init_contract.rs` are Rust. Legacy init remains untouched history and a normalization oracle in compatibility tests only, not an active product dispatch. Config is the expressly excluded Node compatibility reader. Push is unchanged Rust. Other Node commands, existing Node tests, and shell command invocations are unchanged external verification interfaces, not replacement init implementations. No in-scope implementation remains unclassified or non-Rust.

An intermediate GREEN run exposed a test assertion that tried to stat an unnormalized `custom/../settings.json` through a nonexistent `custom` directory. The test now checks the lexically resolved settings file and its JSON contents, matching retained path resolution. A subsequent fmt check detected that test edit before formatting. Both were corrected, and the full Cargo suite, fmt, and clippy passed afterward. The original RED evidence remains unchanged above.

## Completion-check limitations

Source commits are RED `90b8140c77892b4477ded7a5cd434df83c975b29` and GREEN `feb1e060deb05f34ff1683db13fc1f1a0e702091`. The later evidence-only commit records this handoff.

The deterministic `confidence_check` returned 0, reporting no paired recognized test run, no paired error status, and no matched constrained Git commit. Actual Cargo and standalone Rust test output and normal Git commits are recorded above; constrained Pi Git commits are excluded by the delegated GitHub workflow. This result is not a pass. Jev completeness returned 37/100 with 0.90 confidence and done probability 0.10 for the explicitly incomplete story/delivery handoff. No story completion is claimed and these scores do not replace the actual suite results.

All four owned build watches ended with numeric exit codes and `termination.cleanup: succeeded`. Original RED and intermediate formatting failures remain historical evidence. There are no active owned build processes. Cleanup preview found no eligible merged worktrees. The mutating cleanup call requested a backup and did not execute; no worktree was removed.

## Init normalization follow-up

The independent review at `870f066` found two init-only compatibility defects. Shared validation accepted bare `jev-` because every character of an empty suffix passed the predicate. It rejected float-encoded integral team values because `as_u64` does not accept serde JSON floating-point values. The retained reader requires a nonempty suffix and accepts JavaScript integral numbers.

Test-only RED commit `cdd4e4f4aea38944eb0a6bfb118f296665a43af1` changes `native/story-push/tests/init_contract.rs`. The actual command was `CARGO_TARGET_DIR=/home/ejernigan/.cache/story-issue37-target cargo test --manifest-path native/story-push/Cargo.toml --test init_contract`. It compiled, exited 101, and reported `7 passed; 2 failed`. Full output is `/tmp/story-issue37-normalization-red.log`.

```text
literal_integral_team_numbers_match_retained_init_and_config
exit Some(1): team.sprintDays must be a positive integer.
invalid_input_has_diagnostics_and_no_writes_even_with_force
assertion `left == right` failed
  left: Some(0)
 right: Some(1)
test result: FAILED. 7 passed; 2 failed
```

Source GREEN commit `e997473ecc54ea159b51cb0f0c2d278ab0a7bb82` changes only `native/story-push/src/init.rs`. Init checks the empty Jev suffix before filesystem effects. Init converts finite positive integral floating-point team values within the existing u64 validator range to integer JSON values before shared validation. `native/story-push/src/lib.rs`, shared `validate_config`, and push semantics remain unchanged.

The new literal JSON test uses `{"team":{"sprintDays":10.0,"wipLimit":1e0}}` and `{"team":{"sprintDays":1e1,"wipLimit":2.0}}`. It checks native init without Node on PATH, saved canonical integers, retained config output, and retained init output parity. The invalid-input matrix checks bare `jev-`, fractional, zero, and negative team values on fresh and forced-existing targets. It asserts field diagnostics, absent fresh settings/metadata, and byte-preserved existing settings/metadata.

| Verification after source fix | Observed result |
| --- | --- |
| Full Cargo suite with existing external target directory | 40 passed, 0 failed: 19 library, 9 push CLI, 9 init, 3 setup |
| `cargo fmt --manifest-path native/story-push/Cargo.toml --check` | Exit 0 |
| `CARGO_TARGET_DIR=/home/ejernigan/.cache/story-issue37-target cargo clippy --manifest-path native/story-push/Cargo.toml --all-targets -- -D warnings` | Exit 0 |
| `npm test` | 408 passed, 0 failed; `/tmp/story-issue37-normalization-npm.log` |
| Rust `scripts/story-policy.rs` test binary | 12 passed, 0 failed |
| Rust `test/story-policy-contract.rs` test binary | 11 passed, 0 failed |
| `story lint ST-014` | 0 errors; existing missing-WSJF warning remains, with no re-estimate |

`prove_it_works` records the built native CLI process tests and observed persistence/parity/no-write results. All new executable changes are Rust. The prior language audit and conversion gate remain valid. Prior RED/GREEN history above remains unchanged. AC1, AC2, AC3, and the init Rust conversion gate remain MET; installed AC4 remains UNMET. ST-014 remains in-progress. Both story updates were noted, previewed as updates to issue 37, pushed, and confirmed at `https://github.com/Team-Teddy-Development/story-time/issues/37`.

The follow-up deterministic completion check reports 0, with no paired recognized tests/commit and unresolved sentinel records. It does not recognize the captured successful Cargo/Rust commands and ordinary Git commits as paired evidence. The sentinel fix-resolution tool rejected the replay selection as `selected test is not the original failing test command`; actual source fixes and green reruns are recorded above. Jev completeness for the delegated handoff reports 82/100, confidence 0.48, and done probability 0.78. Neither completion check is a pass; no full story completion is claimed. The earlier wrong-path config search was corrected to the discovered `scripts/src/config/config.mjs` and its sentinel record resolved as an invocation correction.

The owned `st014-normalization-npm` watch ended with exit 0 and `termination.cleanup: succeeded`. Its stop check confirms no active owned process. No new worktree was created. No installation, PR, acceptance, issue closure, or worktree removal occurred.

## Handoff boundary

Do not install, open a PR, accept ST-014, close issue 37, or remove this root-owned worktree. Root reviews and delivers the implementation commits, then exercises installed-binary contracts and accepts the story.
