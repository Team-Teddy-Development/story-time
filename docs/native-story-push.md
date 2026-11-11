# Native story push

## Build and verify

Run from the repository root:

```sh
cargo fmt --manifest-path native/story-push/Cargo.toml --check
cargo clippy --manifest-path native/story-push/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path native/story-push/Cargo.toml --locked
cargo build --manifest-path native/story-push/Cargo.toml --release --locked
```

The Rust integration tests invoke the compiled `story` executable. They check the original empty-backlog failure, sync off, missing selectors, read-only planning, and Gitea create/update/link behavior. The native-path tests remove Node from `PATH`. Gitea tests use a loopback server; GitHub and ADO tests use compiled Rust platform-CLI stubs. No test creates production issues.

The retained Node reference suite runs with `node scripts/run-tests.mjs`. It does not prove the native replacement. Both suites run in CI.

## Install

```sh
cargo install --path native/story-push --locked
export STORY_TIME_LEGACY_CLI="$PWD/skills/story-time/scripts/story.mjs"
story push --all --dry-run --json
```

Keep the legacy path in a persistent checkout, not a worktree scheduled for removal. For a persistent setting, put `{"legacyCli":"/absolute/path/to/story.mjs"}` in `~/.config/story-time/runtime.json`. This is a runtime path, not a credential. Push does not read it or invoke Node.

If `story` already exists, preserve that exact entry before replacing it. Record its symlink target or save the executable. Install the new binary at the same path, then run the native regression against that installed path:

```sh
STORY_TIME_TEST_BIN="$(command -v story)" cargo test --manifest-path native/story-push/Cargo.toml --locked --test cli
```

To recover the previous installation, restore the saved executable or symlink and its runtime setting. Do not remove a checkout used by other commands until its replacement is configured.

Sync policy still applies during dry-run. Projects with sync off reject push. For a project with sync enabled, an empty backlog produces `{"ok":true,"plan":[]}` without a token, tracker process, or network request. An empty non-dry push also performs no operations.

## Scope and language audit

| Implementation | Language | Role | Conversion state |
|---|---|---|---|
| `native/story-push/src/main.rs` | Rust | Installed command dispatch; push never enters Node | Converted |
| `native/story-push/src/lib.rs` | Rust | Arguments, settings, repository selection, planning, saga, atomic linking | Converted |
| `native/story-push/src/story.rs` | Rust | Story format, tracker cards, linking, changelog | Converted |
| `native/story-push/src/gateway.rs` | Rust | Tracker credentials, Gitea HTTP, GitHub/ADO platform interfaces | Converted |
| Native unit/integration tests and `tests/empty-push.rs` | Rust | Direct CLI regression, domain, credential, tracker, and rollback checks | Converted |
| Existing `story.mjs` push code and Node tests | JavaScript | Preserved reference; not the supported native push execution chain | Inactive for native push |
| Other Node commands, domain helpers, and skill installer | JavaScript | Non-push behavior retained behind the dispatcher | Out of scope |
| `gh`, `az`, `git`, Node for non-push commands | External programs | Existing platform interfaces | External interface, not replacement implementation |
| Cargo manifest/lockfile, workflow, settings, Markdown stories, fixtures | Configuration/data/documentation | Build, CI, and inputs | Not executable implementation |
| Cargo dependencies | Third-party Rust | HTTP, TLS, JSON, time | Third-party dependencies |

The Rust conversion gate applies to the active push chain, not the whole repository. The supported native entry point has no Node step. Direct `node story.mjs push` is preserved historical behavior and is not the migrated interface.

## Acceptance gates

| Gate | Evidence | State |
|---|---|---|
| Empty `--all --dry-run` succeeds without credentials or writes | Original three-test regression included in native CLI suite | PASS locally |
| Missing selector and disabled sync remain rejected | Native CLI control tests | PASS locally |
| Existing create/update/skip plan and links remain supported | Native CLI planning and loopback Gitea tests | PASS locally |
| Native tracker implementations and credential boundaries work | Rust Gitea mock servers, Rust `gh`/`az` stubs, local-secret tests | PASS locally |
| Failure rollback preserves local files and reports compensation failures | Rust saga regression with create failure and concurrent edits | PASS locally |
| Rust conversion audit has no active in-scope non-Rust implementation | Scope audit above; Node-free CLI tests | PASS locally |
| Native and retained reference suites pass | 28 Rust tests; 408 existing Node reference tests | PASS locally |
| Installed binary, source review, remote CI, and integration | Release handoff records exact commit, installation, PR, and outcomes | UNMET until release handoff records them |

## Root cause and lesson

`es_9c55035c0ff8` is a selector-validation error, not a tracker-authentication error. The old handler treats all empty target arrays as missing arguments. The replacement validates whether a selector exists separately from the number of selected stories. It completes the plan before constructing a gateway.

The regression was committed red before the replacement. Its assertions remain unchanged; its invocation now selects the compiled native executable when Cargo supplies it or `STORY_TIME_TEST_BIN` is set. Without either, the standalone regression can still reproduce the historical Node defect. Do not mark a session follow-up as a source fix. Attach the implementation commit and actual native CLI evidence when resolving the fingerprint.

Remote updates retain the previous contract: they are not automatically reversible. New remote creates and local links are compensated in reverse order. Report failed compensations instead of hiding them or treating a partial remote update as a successful transaction.
