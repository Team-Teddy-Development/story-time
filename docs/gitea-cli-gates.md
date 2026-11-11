# Gitea CLI bounded gates

## Contract

Build one Rust executable around the recovered Rust library. Commands are `create`, `update`, and `close`. Each command reads one bounded JSON object from stdin. `GITEA_TOKEN` is the only credential input. Responses and errors use typed JSON. Help is plain text.

## Workflow

Frame the existing library and callers. Define the CLI boundary. Implement and test the subprocess path against a local Rust HTTP fixture. Run offline tests and real help/error commands. Inspect and commit. Keep this isolated worktree for parent review. Do not push.

The data shape is a command enum containing command-specific deserialized requests, then a tagged output enum. Library types remain authoritative for HTTP operations. This applies Model the Domain by rejecting command-incompatible fields at parsing. Prove It Works requires real subprocess execution, not compilation alone.

The design is a direct adapter to three existing methods. A second architecture competition and delegate fan-out add no material information for this settled boundary. They are skipped. No JavaScript gate runner or implementation is added. Rust tests are the executable gates.

## Acceptance gates

- G1. Rust CLI commands send the expected HTTP method, path, authorization, and JSON body, then return typed issue JSON.
  CHECK: `CARGO_TARGET_DIR=/tmp/story-gitea-cli-target cargo test --offline --manifest-path gitea-gateway/Cargo.toml`
  EXPECT: all CLI HTTP subprocess tests pass.
- G2. Malformed, oversized, trailing, unknown-field, and command-incompatible stdin fails before a request. Credentials never appear in CLI stdout or stderr.
  CHECK: same Rust test command.
  EXPECT: CLI boundary and redaction tests pass.
- G3. Help and invalid command paths run on the compiled executable without credentials.
  CHECK: actual compiled binary invocations recorded below after verification.
  EXPECT: help exits zero; invalid commands return JSON and exit nonzero.
- G4. Approved CLI execution chain is Rust.
  CHECK: inspect CLI, library, and Rust tests.
  EXPECT: no executable JavaScript changes or subprocess delegation to non-Rust code.
- G5. Integration gaps remain explicit.
  CHECK: inspect `docs/gitea-cli.md` against repository entry points.
  EXPECT: no claim of full Story Time conversion, live Gitea verification, or full application integration.

## Verification record

G1, G2, and G4 are verified for the delegated CLI scope. The offline Rust run passed 5 CLI subprocess tests and 3 library HTTP tests, with zero failures. No JavaScript executable files changed. The source chain is CLI to library to reqwest, all Rust.

G3 is verified by direct executable runs. `/tmp/story-gitea-cli-target/debug/gitea-gateway --help` printed the documented usage and exited 0. `gitea-gateway unknown` returned `{"status":"error","code":"invalid_command"}` and exited 1. Piping `{"number":0}` into `gitea-gateway close` returned `{"status":"error","code":"invalid_input"}` and exited 1. Rust subprocess tests assert the same help and error behavior.

G5 is verified by inspecting the existing JavaScript configuration, secrets, tracker port, gateway selection, and npm entry points. `docs/gitea-cli.md` records their missing Gitea integration. The broad Story Time conversion gate stays UNMET. Live Gitea compatibility is not verified.

These are direct Rust test and command observations, not results from the unlazy JavaScript gate runner. The task requires Rust verification. No gate runner was introduced or ported as part of this task.

The first semantic completeness check ran before commit and returned 66/100 with done probability 0.55. It is preserved as incomplete pre-commit evidence. Final completion checks follow the local commit. No admission score is used to authorize implementation.

## Initial state

The recovered commit is `8f7932b`. It contains a Rust HTTP library with three operations and three HTTP integration tests. No binary exists. Story Time still uses JavaScript entry points. This task does not convert those entry points.

## Throughput checkpoint

One executable, one subprocess integration test module, and documentation form the bounded deliverable. No dependency additions are needed. Use the existing offline Cargo cache. A live Gitea endpoint is not required or authorized for this local task.
