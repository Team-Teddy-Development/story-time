# Gitea CLI reference

`gitea-gateway` creates, updates, and closes Gitea issues through Rust. Its `story-push` command also reads Story Time settings and Markdown, pushes issues, and saves local links. Use [Gitea Story Time](gitea-story-time.md) for that active flow. This reference describes the retained raw JSON commands.

## Build and command interface

From the repository root:

```sh
CARGO_TARGET_DIR=/tmp/story-gitea-cli-target cargo build --offline --manifest-path gitea-gateway/Cargo.toml
/tmp/story-gitea-cli-target/debug/gitea-gateway --help
```

The raw JSON interface accepts one command, `create`, `update`, or `close`. `--help` and `-h` print plain text without credentials or stdin. Other argument lists return `invalid_command` without reading stdin.

Each operation reads one JSON object from stdin through EOF. The maximum input size is 1 MiB, including whitespace. Unknown fields, duplicate fields, invalid UTF-8, multiple JSON values, and incorrect field types return `invalid_input`.

Common fields are `base_url`, `owner`, and `repo`, all strings. `base_url` is the Gitea server root, with an optional deployment path prefix. It is not the `/api/v1` URL. The library appends that API path. HTTPS is required except for literal loopback IP addresses over HTTP. URL credentials, queries, and fragments are rejected. Empty owner or repo values, `.` and `..`, are rejected.

| Command | Additional JSON fields | Behavior |
| --- | --- | --- |
| `create` | Required `title` string. Optional `body` string, default empty. | POST creates an issue. A whitespace-only title is rejected. |
| `update` | Required positive `number` integer. Optional `title` and `body` strings. | PATCH changes supplied fields. At least one non-null change is required. Null means omitted. An empty body clears the body. A whitespace-only title is rejected. |
| `close` | Required positive `number` integer. | PATCH sets `state` to `closed`. No title or body fields are accepted. |

Numbers use Rust `u64`. Zero, negative values, fractional values, and values above `18446744073709551615` are rejected.

Examples assume `GITEA_TOKEN` is already set in the process environment. Tokens do not belong in argv or JSON.

```sh
printf '%s' '{"base_url":"https://gitea.example.com","owner":"acme","repo":"shop","title":"Fix checkout","body":"Reproduce with an empty cart."}' |
  /tmp/story-gitea-cli-target/debug/gitea-gateway create

printf '%s' '{"base_url":"https://gitea.example.com","owner":"acme","repo":"shop","number":7,"body":"Updated reproduction."}' |
  /tmp/story-gitea-cli-target/debug/gitea-gateway update

printf '%s' '{"base_url":"https://gitea.example.com","owner":"acme","repo":"shop","number":7}' |
  /tmp/story-gitea-cli-target/debug/gitea-gateway close
```

## Output and exit status

Operations write one JSON object and a newline to stdout. Normal operation writes nothing to stderr. Successful commands exit `0`. Errors exit `1`. An output write failure also exits `1`, but a broken output stream cannot carry a JSON error.

Success example:

```json
{"status":"ok","issue":{"number":7,"title":"Fix checkout","body":null,"state":"closed"}}
```

The issue contains a numeric `number`, string `title`, nullable `body`, and string `state`. The CLI preserves the library response fields and replaces token occurrences in returned string values with `[REDACTED]`.

Error examples:

```json
{"status":"error","code":"invalid_input"}
```

```json
{"status":"error","code":"http_status","http_status":401}
```

| Code | Meaning |
| --- | --- |
| `invalid_command` | Missing, unknown, extra, or non-Unicode command arguments. |
| `invalid_input` | Stdin is not a valid command-specific JSON request. |
| `input_io` | Reading stdin fails. |
| `input_too_large` | Stdin exceeds 1 MiB. |
| `missing_token` | `GITEA_TOKEN` is missing, empty, or not Unicode. |
| `invalid_configuration` | URL, repository coordinates, token header, or HTTP client setup is invalid. |
| `transport` | Connection, TLS, timeout, or response read fails. |
| `http_status` | Server returns a non-success status, including redirects. Only this error includes numeric `http_status`. |
| `response_too_large` | Successful response exceeds 1 MiB. |
| `invalid_response` | Successful response does not deserialize into the library's issue type. |

Input is parsed before token lookup. The CLI never includes raw stdin, arguments, response bodies, URLs, or underlying transport errors in error output. It does not read token files. The token enters the HTTP authorization header only through `GITEA_TOKEN`. The library API itself still accepts a token argument for programmatic callers.

Redirects are disabled. Connection timeout is 5 seconds. Request timeout is 15 seconds. The CLI does not time out while waiting for stdin EOF.

## Verification

```sh
CARGO_TARGET_DIR=/tmp/story-gitea-cli-target cargo test --offline --manifest-path gitea-gateway/Cargo.toml
```

The Rust tests launch the actual compiled executable, pipe stdin, and inspect stdout, stderr, exit status, and HTTP requests. A local Rust TCP fixture verifies all three operations, authentication, path prefixes, response parsing, and redaction. This is not a live Gitea compatibility test. The existing library tests also cover unsafe configuration and bounded responses.

## Historical raw-command integration audit

The following audit records the earlier raw-command foundation. Its missing story rendering, settings, and link integration are now implemented by the direct Rust `story-push` path. See the [current scope audit](gitea-story-time.md#verification-and-rust-conversion-gate) for active status. The older Node entry point still does not support Gitea and is not used for this flow. Preserve the remaining record as historical evidence.

The scoped CLI execution chain is Rust throughout. The broader Story Time Rust conversion gate is **UNMET**.

| Path or component | Language or format | Role | Status |
| --- | --- | --- | --- |
| `gitea-gateway/src/main.rs` | Rust | CLI argument and stdin boundary, JSON output | Implemented here. |
| `gitea-gateway/src/lib.rs` | Rust | HTTP transport, URL validation, issue operations | Recovered foundation, now serializes issue responses. |
| `gitea-gateway/tests/cli.rs`, `tests/http.rs` within the crate | Rust | Executable integration verification | Passing offline with local HTTP fixtures. |
| `gitea-gateway/Cargo.toml`, `Cargo.lock` | TOML and lock data | Build configuration and dependency versions | Required configuration, not executable implementation. |
| serde, serde_json, reqwest, Rust toolchain, transitive dependencies | Third-party Rust dependencies and build tooling | Serialization, HTTP, TLS, compilation | Existing dependency chain, no new dependencies added. |
| Gitea REST API and JSON input/output | External interface and data | Remote service protocol | Live compatibility remains unverified. |
| `skills/story-time/scripts/story.mjs` and its `src/` tree | JavaScript | Active Story Time CLI, domain logic, adapters, and configuration | Out of delegated scope, not converted, not called by this Rust CLI. |
| `skills/story-time/scripts/test/`, root `test/`, `scripts/run-tests.mjs` | JavaScript | Existing Story Time test chain | Out of delegated scope, not converted, not run as proof for this CLI. |
| `scripts/install.mjs` | JavaScript | Existing skill installation entry point | Out of delegated scope, not converted, no Rust binary installation added. |
| `package.json`, skill markdown, JSON fixtures, and repository documentation | Configuration and data | Packaging, agent instructions, test inputs | Not Rust conversion targets by themselves. |

The missing application integration is concrete:

1. `skills/story-time/scripts/src/config/config.mjs` excludes `gitea` from `TRACKERS`, `PUSHABLE_TRACKERS`, and known settings keys. Setting `tracker` to `gitea` fails.
2. `skills/story-time/scripts/story.mjs` selects GitHub and Azure DevOps gateways. It does not invoke this binary.
3. `skills/story-time/scripts/src/config/secrets.mjs` defines GitHub, Azure DevOps, and Jev secrets, but no Gitea token entry. This CLI intentionally uses only its environment.
4. `skills/story-time/scripts/src/ports/tracker-gateway.mjs` expects `create`, `update`, and `remove`, with remote IDs and URLs. This CLI returns issue fields and exposes `close`. Story rendering, remote URL storage, and saga compensation are not connected.
5. Installation and CI for the complete Story Time application do not build, install, or test this standalone Rust binary through the current npm scripts.

Full Gitea support also needs explicit scope beyond these issue mutations. This CLI has no issue read/list/search, pagination, labels, milestones, assignees, comments, pull requests, repository management, or webhook operations. Permissions are enforced by Gitea, not by a new local policy layer.

No retry, idempotency key, deduplication, ambiguous-create reconciliation, or rate-limit policy is implemented. A timeout can occur after Gitea creates an issue. Repeating `create` can create a duplicate. Operators must reconcile that outcome before retrying. No live Gitea credentials or endpoint were used for verification.
