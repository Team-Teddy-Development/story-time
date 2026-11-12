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

## Handoff boundary

Do not install, open a PR, accept ST-014, close issue 37, or remove this root-owned worktree. Root reviews and delivers the two commits, then exercises installed-binary contracts and accepts the story.
