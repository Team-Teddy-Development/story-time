# Cartographer production preflight

`cartographer-preflight` reports deployment replica counts and four selected literal environment values. It performs no cluster writes.

## Fixed access boundary

Every Kubernetes request uses context `admin@coffee-cluster-1`, namespace `cartographer`, and `--request-timeout=10s`. Callers cannot change targets or supply commands, file paths, environment names, or projections.

The initial `get deployments` request projects only deployment names, desired replicas, and ready replicas. The tool selects these workloads from that metadata:

| Role | Deployment |
| --- | --- |
| Panel | `cartographer-callback-probe` |
| Authority v1 | `cartographer-authority` |
| Authority v2 | `cartographer-authority-v2` |
| Bridge | `cartographer-bridge` |

The tool does not substitute `cartographer-ui` for a missing panel. A missing workload is a failed prerequisite.

For the discovered panel and bridge, separate requests project container names and these four literal `.env[].value` fields:

- `CARTOGRAPHER_AUTHORITY_URL`
- `CARTOGRAPHER_AUTHORITY_V2_URL`
- `CARTOGRAPHER_ORIGIN`
- `CARTOGRAPHER_ORGANIZATION`

The projections never access `valueFrom`, secrets, ConfigMaps, logs, annotations, general environment data, or authority configuration. Secret-backed values remain absent. The tool does not resolve them.

## Reports

Each prerequisite has `pass`, `observed`, and `expected` fields. Overall `pass` requires every prerequisite to pass.

Replica checks require desired replicas greater than zero and ready replicas equal to desired replicas. Authority v2 scaled to zero fails, including when both counts are zero. An absent ready count means zero. Invalid numeric output is an inspection error.

The panel requires all four selected literals. The bridge requires the two authority URLs; browser origin and organization are optional because it is not the panel. Multiple containers can supply the same value. Conflicting values fail. Containers with none of the selected values do not add values.

URL values must start with `http://` or `https://`. User information, query strings, fragments, backslashes, whitespace, control characters, non-ASCII text, and values longer than 512 bytes fail. Organization values accept ASCII letters, numbers, colons, periods, underscores, and hyphens, including canonical Cartographer organization IDs. Unsafe nonempty values appear as `[redacted: unsafe value]`, never as raw values.

These checks establish metadata prerequisites, not production health. They do not test DNS, HTTP reachability, authorization, registry admission, callback behavior, rollout generation freshness, or whether URLs identify the intended endpoints. Successful replica counts do not prove any of those properties.

## CLI

From the repository root, the offline test command is:

```sh
cargo test --offline --manifest-path tools/cartographer-preflight/Cargo.toml
```

The build command is:

```sh
cargo build --offline --manifest-path tools/cartographer-preflight/Cargo.toml
```

Run the live read-only inspection with:

```sh
tools/cartographer-preflight/target/debug/cartographer-preflight inspect
```

`inspect` prints one JSON document. Exit code `0` means all prerequisites pass. Exit code `1` means inspection succeeds but production prerequisites fail. Exit code `2` means transport, projection parsing, or invocation fails. Transport failures never become absent-workload reports.

## Stdio MCP

With no arguments or with `stdio`, the executable accepts newline-delimited JSON-RPC 2.0 on stdin. It writes only responses to stdout. Notifications receive no response.

Supported methods are `initialize`, `ping`, `tools/list`, and `tools/call`. The sole tool is `cartographer_project_preflight`. Its input schema is closed and empty:

```json
{"type":"object","properties":{},"additionalProperties":false}
```

An example request is:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"cartographer_project_preflight","arguments":{}}}
```

Unknown tools and nonempty arguments return JSON-RPC error `-32602` before Kubernetes access. Production blockers remain report data with MCP `isError: false`. Inspection failures return `isError: true` with `kind: inspection_error`.

## Process and output limits

Rust starts `kubectl` with argument arrays, never a shell. Requests use `get deployments` and explicit JSONPath projections. No port forwarding, pod execution, deployment operation, or write command exists.

Each subprocess has a 15-second wait deadline. Captured stdout is limited to 65,536 bytes. Invalid UTF-8 and oversized projections are errors. The tool suppresses subprocess stderr because authentication plugins can print credentials. Errors report fixed descriptions without raw subprocess output. Stdio requests have a 65,536-byte limit. An oversized request produces an error and ends the stdio session.

`kubectl` remains an external platform interface. Its existing authentication and kubeconfig behavior are outside this implementation. This tool neither reads nor prints those files. The operator must trust the installed `kubectl` and its configured authentication mechanism.

## Rust acceptance gate

Gate status is MET for the delegated scope `tools/cartographer-preflight/**`.

| Artifact | Language or format | Role | Conversion status |
| --- | --- | --- | --- |
| `src/main.rs` | Rust | CLI, MCP, projections, parsing, evaluation, process bounds, unit tests | Active Rust implementation |
| `tests/stdio.rs` | Rust | Real executable protocol and CLI boundary tests | Active Rust verification |
| `Cargo.toml`, `Cargo.lock` | Cargo configuration | Package and locked third-party dependency metadata | Not executable implementation |
| `.gitignore` | Git configuration | Excludes local build output | Not executable implementation |
| `docs/README.md` | Markdown | Tool contract and verification boundary | Documentation |
| `kubectl` | External platform interface | Kubernetes transport and JSONPath evaluation | Not agent-authored implementation |
| `serde_json` and transitive crates | Third-party Rust dependencies | JSON encoding and decoding | External dependencies |

No non-Rust implementation, configuration credentials, or embedded executable scripts are added. Parent-owned ST-001 changes and other repository implementations are outside this audit.

Unit tests use synthetic projected data. Integration tests execute the compiled CLI and stdio server with an empty `PATH`, preventing live Kubernetes access. They assert expected transport failures without printing them. Live preflight verification remains parent-owned.
