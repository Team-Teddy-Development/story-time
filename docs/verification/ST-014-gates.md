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

## Handoff boundary

Do not install, open a PR, accept ST-014, close issue 37, or remove this root-owned worktree. Root reviews and delivers the implementation commits, then exercises installed-binary contracts and accepts the story.
