# Live Gitea verification

## Result

The native Rust Story Time push command passed against a real, isolated Gitea process on `http://127.0.0.1:31987`. The installed binary reported `gitea version 28.3.0`. No production repository or credentials were used.

The verification used an isolated SQLite database, repository directory, administrator, and API token under `/tmp/story-gitea-live-20261111`. Credentials were never printed. The test server was bound to loopback with SSH disabled. The server and its child processes must be stopped after verification.

The committed Rust example `gitea-gateway/examples/live_verify.rs` creates a private test repository on that isolated server, invokes the compiled CLI, and independently reads issue state through Gitea's API. It asserts create, repeat update, saved local links, acceptance criteria, close, reopen, team-owned label preservation, stale managed-label removal, and exactly one issue after repeated pushes.

```sh
STORY_PROOF_TOKEN_FILE=/tmp/story-gitea-live-20261111/token \
STORY_PROOF_BINARY=/tmp/story-gitea-cli-target/debug/gitea-gateway \
cargo run --offline --manifest-path gitea-gateway/Cargo.toml \
  --target-dir /tmp/story-gitea-cli-target --example live_verify
```

Observed exit code: 0.

```text
LIVE_GITEA_VERIFIED create, repeat update, persisted links, criteria, close, reopen, team-label preservation, stale-label removal, one issue
```

## Other checks

The parent ran all 26 Rust tests with zero failures, format checking, and Clippy with warnings denied. After merging current upstream main and resolving the setup skill description, the existing `npm test` suite passed 408 tests, with zero failures, skips, or cancellations. Node tests are legacy regression evidence, not the Rust implementation of the new feature.

## Scope and history

Earlier documents record missing live verification as the state at their original checks. This record supersedes only that missing local-live-server evidence. Production Gitea version compatibility, production TLS, and production token policy remain unverified.

Upstream main already merged its separate Node Gitea integration through PR #10 while this recovery was running. That code was not authored by this recovery. This branch adds a fully native Rust path alongside it and updates the skills to select the native path. It does not claim that all existing Story Time executable implementations have been converted to Rust.
