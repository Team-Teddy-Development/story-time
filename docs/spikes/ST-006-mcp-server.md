# ST-006 spike: a story-time MCP server

Spike: ST-006, [issue 26](https://github.com/Team-Teddy-Development/story-time/issues/26). Consumers: ST-007, [issue 27](https://github.com/Team-Teddy-Development/story-time/issues/27), and ST-011, [issue 31](https://github.com/Team-Teddy-Development/story-time/issues/31). Decision record: [ADR 0006](../adr/0006-story-mcp-in-native-cli.md). Source revision read: `bd151a2`. Read date: 2026-11-14.

## Answer

Add a stdio MCP transport to the existing native Rust `story` binary as `story mcp`. The transport calls typed functions in the `story_time_native` library. The CLI commands render text and JSON from the same functions. Do not add a second server binary, a Streamable HTTP listener, or a subprocess call to `story`.

Ship two slices. The first slice is read-only and is the default. The second slice adds guarded local writes only when the operator starts the server with `--write`. Neither slice publishes to a tracker, reads a tracker token, or uses the network.

## Inventory

### CLI and application core

`native/story-push` is one crate, `story-time-native`. It builds the `story`, `story-install`, and `story-install-skills` binaries (`native/story-push/Cargo.toml`). `src/main.rs` dispatches through `NATIVE_COMMANDS` in `src/lib.rs`. Every command is native since ST-096; ADR 0003 records that no `story` command starts Node.

Each command function has the signature `fn(&[String]) -> Result<(String, u8), Failure>`. It parses flags, loads settings, and returns rendered output. This shape is the main obstacle for MCP:

| Concern | Current code | Effect on an MCP server |
| --- | --- | --- |
| Repository selection | `config::locate` reads `STORY_TIME_CONFIG`, else `.story-time/settings.json` under `current_dir()` (`src/config.rs`). | The server cannot select a repository by an explicit, validated root. |
| Backlog directory | `list::backlog_dir` takes `--dir`, else `STORY_TIME_DIR`, else `backlogDir` under the root (`src/list.rs`). | A launch environment can move the backlog outside the root. |
| Output | `show::run`, `list::run`, `set::run`, and `note::run` return formatted strings. | A server would parse text or JSON strings it just built. |
| Errors | `Failure { code, message }` with free text (`src/lib.rs`). | There is no stable error code for a client to branch on. |
| Story model | `lenient_story::Js` with `json()` and `serialize()` (`src/lenient_story.rs`). | Reusable as is. It is the one lenient parser. |
| Lifecycle rule | `set` gates a move to `ready` on `lint::lint_story` errors unless `--force` (`src/set.rs`). | Reusable. MCP must never expose `--force`. |
| Changelog | `lenient_story::append_changelog` (`src/set.rs`, `src/note.rs`). | Reusable for audit lines in the story. |
| Writes | `list::save` writes `<id>.md.<pid>.tmp` and renames over the target, without a lock or a precondition. `lib.rs::atomic_write` (push) uses `create_new`, `fsync`, and keeps the file mode. | Two write paths exist. Neither checks that the file is unchanged since it was read. |
| Locking | `adoption_apply::lock` holds an OS lock on the repository root that the OS releases on exit (`src/adoption_apply.rs`). | Reusable as the per-repository write lock. |
| Allocation | `next_id::next_id` takes one above the highest id in the backlog and on every git branch (`src/next_id.rs`). `new_story` then calls `list::save`. | Two concurrent creates can compute the same id. The second rename replaces the first story. See [Defect found](#defect-found). |
| Sync policy | `config::parse` accepts `sync` of `off`, `ask`, or `auto` (`SYNC_MODES`). Only `story push` reads it (`src/lib.rs`, "Pushing is off for this project"). `set`, `note`, and `new` never push. | MCP writes change local files only. Publishing stays a separate, explicit step. |
| Secrets | `secret.rs` stores one token per tracker in `.story-time/.<name>.env` (mode 0600). `gateway.rs` reads `GH_TOKEN`, `AZURE_DEVOPS_EXT_PAT`, or `GITEA_TOKEN`. | The read and write slices need neither module. |

The CLI therefore already holds the domain logic. The MCP work is to give that logic an explicit-root, typed entry point, not to write new rules.

### The Rust preflight MCP

`tools/cartographer-preflight` is the existing Rust MCP in this repository (ADR 0002, `tools/cartographer-preflight/src/main.rs`, `tools/cartographer-preflight/tests/stdio.rs`, `tools/cartographer-preflight/docs/README.md`). Facts that carry over:

- It is a hand-written, synchronous JSON-RPC loop over stdio with `serde_json` as its only dependency. It needs no async runtime.
- It caps input lines, answers `-32700` for parse errors, `-32600` for invalid requests, `-32601` for unknown methods, and `-32602` for an unknown tool or bad arguments. A tool failure returns `isError: true` with a typed `kind`.
- It fixes its targets. The caller cannot choose commands, files, or namespaces.
- It speaks protocol `2024-11-05` through `initialize`. It has no `server/discover`. It does not answer modern per-request `_meta`.
- CI runs its tests and Clippy (`.github/workflows/test.yml`, job `cartographer-preflight`). Its real stdio output is evidence in `docs/cartographer/live-preflight.jsonl`.

It is a separate crate with a fixed single purpose. Story-time tools need the story domain, so the new transport belongs in `native/story-push`, not in the preflight crate. The preflight framing and error discipline are the pattern to follow. Its protocol version is out of date; see [Protocol era](#protocol-era).

### Target clients on this machine

Installed versions: Claude Code `2.1.325`, Codex CLI `codex-cli 0.157.1`. The installed `story` binary is `~/.local/bin/story`, written by `story-install`.

| Client | Configuration observed | Launch facts from its documentation |
| --- | --- | --- |
| Claude Code | `~/.claude.json` user `mcpServers` hold two stdio servers (`command` set) and two `http` servers. No story-time project has an `.mcp.json`. | Scopes: local and user in `~/.claude.json`, project in `.mcp.json`. stdio launch: `claude mcp add --transport stdio <name> -- <command> [args...]`. It sets `CLAUDE_PROJECT_DIR` in the server environment. It prompts before it uses a project-scoped server in interactive sessions. Output limit `MAX_MCP_OUTPUT_TOKENS`, default 25,000 tokens, warning above 10,000. |
| Codex | `~/.codex/config.toml` holds two `[mcp_servers.*]` tables, both `url` (HTTP). | stdio fields `command`, `args`, `env`, `env_vars`, `cwd`. Project config `.codex/config.toml` applies to trusted projects only. `startup_timeout_sec` default 10, `tool_timeout_sec` default 60. `default_tools_approval_mode = "writes"` prompts for tools that are not marked read-only. |

Only key names and transport types were read from these files. No token, URL path, or header value is recorded here.

## Design comparison

### Boundary: dedicated binary or `story mcp`

Both designs need the same core change: typed, explicit-root functions in the library. The difference is packaging.

| Criterion | (a) Dedicated `story-mcp` binary on the shared core | (b) `story mcp` in the existing binary |
| --- | --- | --- |
| Install and discovery | New artifact. `story-install` must build, back up, and roll back a second binary. | `story-install` already installs and rolls back `~/.local/bin/story`. No new artifact. |
| Version skew | CLI and server can differ after a partial install. | Impossible. One build serves both. |
| Dependency surface | Same crate, same locked dependencies. A separate crate would duplicate `config`, `lenient_story`, and `lint`, which the shared brief forbids. | Same. |
| Process shape | Long-running loop in its own `main`. | Long-running loop. `main.rs` needs one branch before the table, because table commands return a whole string at exit. |
| Attack surface | Smaller binary does not reduce reachable code, because tools call the same core. | Same reachable code. The server does not dispatch to other CLI commands. |
| Rust conversion | Pure Rust. | Pure Rust. |

Subprocess coupling (an MCP server that runs `story ... --json` and parses its output) is rejected. It loses typed errors (the CLI prints free text to stderr), parses rendered output, adds a process per call, and inherits `STORY_TIME_DIR` and `current_dir()` from the launch environment. A wrapper around the historical Node CLI does not meet Article VIII and is not considered.

**Selected: (b).** It is the smaller change for the same core work, and it removes version skew. The server is a module (`src/mcp.rs` plus a protocol module) in `story_time_native`, reached from `main.rs` as `story mcp`.

### Transport: stdio or Streamable HTTP

| Criterion | stdio | Streamable HTTP |
| --- | --- | --- |
| Claude Code support | Yes, `--transport stdio`. | Yes, `--transport http`. |
| Codex support | Yes, `command`/`args`/`cwd`. | Yes, `url` with `bearer_token_env_var`. |
| Repository access | The client starts the server on the same host and user as the checkout. | Needs a host with the checkout mounted, or a daemon per user. |
| Required security | Spec: stdio servers SHOULD NOT use the HTTP authorization framework and take credentials from the environment. | Spec: servers MUST validate `Origin` (403 when invalid), SHOULD bind to 127.0.0.1 locally, SHOULD authenticate every connection, and MUST check `MCP-Protocol-Version`, `Mcp-Method`, and `Mcp-Name` against the body (`-32020`). |
| Lifecycle | The client owns the process. EOF on stdin is the shutdown signal. | A separate service to deploy, supervise, and secure. |
| Concurrency | One client per process. Requests are serial. Other processes still write the same files. | Many clients per process. Needs the same file-level controls plus request concurrency. |

**Selected: stdio only.** Every target client launches stdio servers, the data is a local checkout, and stdio needs no listener, token, or Origin policy. Streamable HTTP adds a network service with no current consumer. It stays out of scope until a remote client needs it.

### Protocol era

The current MCP revision is `2026-07-28`. It is stateless: every request carries `_meta.io.modelcontextprotocol/protocolVersion` and `clientCapabilities`, servers MUST implement `server/discover`, and every result carries `resultType`. Revisions `2025-11-25` and earlier use an `initialize` handshake. A legacy client cannot reach a modern-only server.

The installed client versions are recent, but this spike did not observe which era each one sends. That fact does not change the decision: the server is dual-era. It answers modern requests statelessly and answers `initialize` with legacy `2025-11-25` semantics for the life of the process, as the versioning page permits. The first delivery story records the era each real client used.

### SDK

Hand-write the protocol layer, as the preflight did. The story-time crate is synchronous (`reqwest` blocking), and stdio requests are serial, so cancellation needs no runtime. `rmcp` 3.10.0 supports both transports but requires `tokio`, and it was released 2026-11-12, two days before this spike. An async runtime is a large dependency for a serial stdio loop of fewer than 500 lines in the preflight precedent. Re-evaluate `rmcp` only if Streamable HTTP or subscriptions become in scope.

### Resources and prompts

Neither adds value for the first two slices.

- Prompts: the story-time skills already give both clients the workflow text, and `story-install` installs them for Claude and Codex. A second copy as MCP prompts would drift.
- Resources: a `story://` resource would duplicate `story_show`. The tools are model-controlled and both clients support them. Revisit resources only if a client needs to attach a story by reference in its UI.

Declare only the `tools` capability, with `listChanged: false`.

## Contracts

These contracts are the specification for ST-007's children. JSON Schemas use the 2020-12 dialect (the MCP default). Every input schema sets `"additionalProperties": false`.

### Launch and repository isolation

```
story mcp [--root DIR]... [--write]
```

- Each `--root` names one repository. Without `--root`, the root is `$CLAUDE_PROJECT_DIR` when set, else the current directory. The server canonicalizes each root once at start.
- A root is usable when `.story-time/settings.json` (or the legacy settings file `config::locate` accepts) exists in it and `config::parse` accepts it. An unusable root is reported in `story_config` results as `config_invalid` or `not_configured`. It does not stop the server.
- Each root gets a key: the final path component, lowercased, made unique with `-2`, `-3`. Keys match `^[a-z0-9][a-z0-9._-]{0,63}$`.
- The backlog directory is `backlogDir` under the root. In MCP mode the server ignores `STORY_TIME_DIR`, `STORY_TIME_CONFIG`, and `STORY_TIME_PREFIX`. After canonicalization the backlog directory must be inside the root. A symlink that leaves the root is refused with `path_outside_repository`.
- Tool arguments never take a path. They take a `repository` key and story ids. Ids must pass `valid_id` and the configured prefix.
- In Team Teddy shared anchors, the operator points `--root` at a run-owned worktree, not at the shared anchor. The server does not create worktrees.

### Authorization

The stdio server runs as the local user and has that user's file access. Authorization is therefore a set of narrowing controls, not identity checks:

1. Launch allowlist. Only `--root` repositories are reachable. `repository` must equal an allowlisted key; otherwise `repository_not_allowed`. When exactly one root is allowlisted, `repository` may be omitted.
2. Write gate. Without `--write`, `tools/list` returns only read tools and a write tool name is an unknown tool (`-32602`). The tool set is fixed for the life of the process, as the tools page requires.
3. Client approval. Read tools carry `readOnlyHint: true`. Write tools carry `readOnlyHint: false`, so Codex `writes` approval mode prompts for them, and Claude Code permission rules can allow or deny each `mcp__<server>__<tool>`.
4. Lifecycle rules. The core applies the same rules as the CLI. MCP has no `force` input.
5. No tracker authority. No tool pushes, reads `.story-time/.*.env`, reads tracker token variables, starts `gh`, `az`, or `tea`, or opens a network connection.

### Protocol behavior

| Request | Response |
| --- | --- |
| Modern request without required `_meta` fields | `-32602` Invalid params. |
| Modern request with an unsupported version | `-32022` with `data.supported: ["2026-07-28", "2025-11-25"]` and `data.requested`. |
| `server/discover` | Supported versions, `capabilities: {tools: {listChanged: false}}`, `serverInfo: {name: "story-time", version}`. |
| `initialize` | Legacy `2025-11-25` result. Later requests on this process without modern `_meta` use legacy semantics. |
| `ping` | Empty result. |
| `tools/list` | The fixed tool list in a stable order, no `nextCursor`. A `cursor` param returns `-32602`. |
| `tools/call` with unknown name | `-32602` "Unknown tool". |
| Malformed JSON | `-32700`, id null. |
| Not a JSON-RPC 2.0 request | `-32600`. |
| Unknown method | `-32601`. |
| Line longer than 1 MiB | `-32600` "Request exceeds input limit", then the server stops reading, as the preflight does. |
| `notifications/cancelled` | Ignored. Requests run serially, so the referenced request has already finished. |
| EOF on stdin | Exit 0. |

Every modern result has `resultType: "complete"` and `_meta.io.modelcontextprotocol/serverInfo`. The server writes only MCP messages to stdout. It writes audit lines to stderr.

### Tool results and errors

Every tool result carries `structuredContent` that matches the tool's `outputSchema`, and the same JSON serialized in one `text` content block, as the tools page recommends for compatibility.

Input that fails the tool's schema or a domain rule is a tool execution error, not a protocol error. The result has `isError: true` and this structure:

```json
{
  "error": {
    "code": "conflict",
    "message": "ST-012 changed since it was read.",
    "retryable": true,
    "details": { "id": "ST-012", "expected_sha256": "…", "current_sha256": "…" }
  }
}
```

| `code` | Meaning | `retryable` |
| --- | --- | --- |
| `invalid_argument` | Argument fails the input schema. `details.pointer` is the JSON Pointer. | false |
| `invalid_cursor` | Cursor is not one this server issued, or its filter differs. | false |
| `repository_not_allowed` | `repository` is missing with several roots, or is not allowlisted. | false |
| `not_configured` | The root has no settings file. | false |
| `config_invalid` | `config::parse` rejected the settings. `details.reason` is the parser message without absolute paths. | false |
| `path_outside_repository` | The backlog directory resolves outside the root. | false |
| `invalid_story_id` | Id fails `valid_id` or the prefix. | false |
| `story_not_found` | No `<id>.md`. | false |
| `story_malformed` | The lenient parser rejected the file. | false |
| `story_too_large` | Story file larger than 1 MiB. | false |
| `field_not_writable` | Field is immutable (`id`, `created`, `updated`, `sections`) or reserved for push (`remote`, `remote_url`, `pending_create`). | false |
| `invalid_value` | A value fails the CLI's coercion (for example a non-numeric `size`). | false |
| `lifecycle_refused` | A move to `ready` with lint errors. `details.findings` lists `{rule, message}`. | false |
| `conflict` | `expected_sha256` differs from the file. | true, after a new read |
| `lock_held` | Another process holds the repository write lock. | true |
| `duplicate_title` | A story with the same normalized title exists and `allow_duplicate_title` is false. `details.ids` lists them. | false |
| `id_taken` | Allocation found the id already written by another process twice in a row. | true |
| `io_error` | A file operation failed. `details.operation` names it. No OS path outside the root. | true |
| `internal` | Bug. The message is generic; the audit line has the fingerprint. | false |

Error messages never include absolute paths outside the root, environment values, or file contents other than the story's own fields.

### Read tools (slice 1)

All read tools: `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true`, `openWorldHint: false`.

**`story_config`.** Input `{ "repository"?: string }`. Output:

```json
{
  "repositories": [
    { "key": "story-time", "status": "ok", "prefix": "ST", "tracker": "github",
      "sync": "auto", "backlog_dir": "backlog", "wip_limit": 1,
      "definition_of_ready": ["…"], "definition_of_done": ["…"] }
  ]
}
```

With `repository`, the array has one entry. `status` is `ok`, `not_configured`, or `config_invalid`. Only these allowlisted fields appear. Tracker coordinates (`github.repo`, `gitea.baseUrl`, `gitea.owner`, `gitea.repo`, `ado.organization`, `ado.project`) appear under `tracker_target` because ST-011 needs them for repository identity. No other settings field is projected.

**`story_list`.** Input:

```json
{ "type": "object", "additionalProperties": false, "properties": {
  "repository": { "type": "string" },
  "status": { "type": "array", "items": { "enum": ["draft","refined","ready","in-progress","done","split","archived"] }, "maxItems": 7 },
  "type": { "enum": ["story","spike","bug","chore"] },
  "epic": { "type": "string", "maxLength": 64 },
  "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 50 },
  "cursor": { "type": "string", "maxLength": 512 } } }
```

Output `{ "repository": key, "stories": [Summary], "next_cursor": string | null, "total": integer }`. `Summary` is `{ id, title, status, type, size, epic, depends_on, remote_url, updated, sha256 }`; absent fields are `null`, and `depends_on` is always an array. Order is the CLI's id order (`list::stories`). The cursor is opaque base64 of `{after_id, filter_digest}`. It continues after the last id, so stories added or removed between pages do not repeat or skip earlier ids. A cursor with a different filter returns `invalid_cursor`. A page of 50 summaries stays well under Claude Code's 10,000-token warning.

**`story_show`.** Input `{ "repository"?: string, "id": string }`. Output `{ "repository": key, "id", "path": "backlog/ST-012.md", "sha256", "bytes", "frontmatter": object, "sections": [{ "heading", "body" }] }`. `path` is relative to the root. `sha256` is the hash of the file bytes and is the precondition token for writes.

**`story_lint`.** Input `{ "repository"?: string, "ids"?: [string] (maxItems 100) }`. Output `{ "repository": key, "results": [{ "id", "findings": [{ "rule", "severity", "message" }] }], "errors": integer, "warnings": integer }`. It calls `lint::lint_story`, which is local and has no network judge.

### Write tools (slice 2, `--write` only)

All write tools take the repository write lock (the same OS lock as `adoption_apply::lock`), re-read the target under the lock, write a temporary file with `create_new`, `fsync` it, keep the file mode, and then rename it (`lib.rs::atomic_write`). `openWorldHint: false` for all.

**`story_update`.** `readOnlyHint: false`, `destructiveHint: true`, `idempotentHint: true`. Input:

```json
{ "type": "object", "additionalProperties": false, "required": ["id","expected_sha256","set"],
  "properties": {
    "repository": { "type": "string" },
    "id": { "type": "string" },
    "expected_sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
    "set": { "type": "object", "minProperties": 1, "maxProperties": 20,
             "additionalProperties": { "type": ["string","number","array","null"] } },
    "note": { "type": "string", "maxLength": 500 } } }
```

Semantics match `story set`: the same coercion, the same `ready` gate, one changelog line `set k=v, … — note`. `null` removes a field. Under the lock:

1. If the file hash equals `expected_sha256`, apply, write, and return `status: "updated"`.
2. If the hash differs but every requested field already has the requested value, write nothing and return `status: "already_applied"` with the current hash. A retry after a lost response therefore converges and does not add a second changelog line.
3. Otherwise return `conflict` with both hashes.

**`story_note`.** `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: false`. Input `{ repository?, id, expected_sha256, note (1..500 chars) }`. It appends one changelog line, as `story note` does. It is not idempotent; the hash precondition makes a blind retry fail with `conflict` instead of adding a duplicate line.

**`story_create`.** `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: false`. Input `{ repository?, title (1..200), statement?, acceptance_criteria?: [string] (maxItems 20), type?: enum, epic?, allow_duplicate_title?: boolean }`. Semantics match `story new`. Under the lock it allocates with `next_id`, then writes with a no-replace link (a hard link from the temporary file, which fails if the target exists). If the id is taken, it allocates again once, then returns `id_taken`. Before allocation it refuses an existing story with the same trimmed, case-folded title unless `allow_duplicate_title` is true, so a blind retry after a lost response does not create a twin.

Every write result is:

```json
{
  "status": "updated",
  "story": { "...": "Summary" },
  "sha256_before": "…", "sha256_after": "…",
  "audit": { "operation": "story_update", "repository": "story-time", "id": "ST-012",
             "fields": ["status"], "changelog": "set status=refined", "at": "2026-11-14T12:00:00Z",
             "server_version": "0.1.0" },
  "publish": { "sync": "auto", "tracker": "github", "pushed": false, "next": "story push ST-012" }
}
```

### Sync policy

MCP writes never publish. This matches the CLI, where only `story push` reads `sync`.

| `sync` | `publish.next` | Notes |
| --- | --- | --- |
| `off` | `null` | Push is refused for this project. `story export` gives a card. |
| `ask` | `"story push <id>"` | The agent follows the approval policy before it runs the command. |
| `auto` | `"story push <id>"` | The repository policy says to push after each change. The server does not do it. |

A `story_push` tool is out of scope for both slices. It needs tracker tokens, network access, and remote rollback (ADR 0003), which would remove the "no secrets, no network" property that makes the server safe to enable by default. File it separately if agents need it.

### Secrets and redacted audit evidence

- The process never reads `.story-time/.*.env` and never reads `GH_TOKEN`, `AZURE_DEVOPS_EXT_PAT`, `GITEA_TOKEN`, or the Jev key. Tests assert this with fake values in these variables and files, and check that no output contains them.
- Every outgoing tool result passes one redactor. It replaces token-shaped strings (`ghp_`, `gho_`, `ghs_`, `github_pat_`, `glpat-`, `Bearer <value>`, and the value of any of the tracker variables above when it is set and at least 8 bytes long) with `[redacted]`, and adds `redactions: <count>` to the result. A story that contains a pasted token is shown redacted.
- After each `tools/call`, the server writes one JSON line to stderr: `{at, tool, repository, ids, outcome, error_code, sha256_before, sha256_after, duration_ms}`. It never includes field values, notes, titles, or story bodies. Clients may capture stderr; the spec says they must not treat it as an error signal.
- The story changelog line is the durable audit record in the repository. The server writes no separate audit file in the repository.

### Limits

Input line 1 MiB. Story file 1 MiB. `story_list` page 100. `story_lint` 100 ids. Serial request processing. A tool call that would exceed Codex's 60-second default timeout is not expected for these local operations; the delivery stories measure it on this repository's backlog.

## Proposed delivery stories

The report for this spike gives the full criteria for the coordinator to file. Summary:

| Proposed story | Depends on | Proposed size | Reference |
| --- | --- | --- | --- |
| S1. Read stories through a read-only `story mcp` server (protocol, isolation, `story_config`, `story_list`, `story_show`, `story_lint`) | ST-006 | 5 | ST-022 (read-only preview, 5) |
| B1. Never replace an existing story when two creates race (CLI `story new`) | none | 2 | ST-042 (2) |
| S2. Update and note stories through guarded MCP write tools | S1 | 5 | ST-050 (`story set` native, 3) plus lock and precondition work |
| S3. Create stories through MCP without duplicate ids or titles | S2, B1 | 3 | ST-039 (concurrent label lock, 3) |

ST-007 becomes the parent of S1, S2, and S3 (status `split`). Its three criteria map to S1 (read), S2 and S3 (write with audit), and S1 plus S2 (typed failures, isolation, conflicts).

## What ST-011 needs

- Depend on S1, not on ST-007 as a whole. A graph needs reads and repository identity, not story writes.
- Repository identity: the tracker target from `story_config` (`github:<owner>/<repo>`, `gitea:<host>/<owner>/<repo>`, `ado:<org>/<project>`), else `path:<sha256 of the canonical root>`. Node key `(repository_id, story_id)`.
- A new opt-in launch flag (for example `--graph`) that adds `story_graph_sync` (writes Neo4j only) and `story_graph` (bounded read: depth at most 3, nodes at most 200). It is the first part of the server that needs a credential and the network, so it must stay off unless enabled, and it must get its credential through the approved secret mechanism (`story secret` would need a `neo4j` provider).
- A missing `depends_on` target becomes a node with `missing: true`, not a dropped edge. Edges are `DEPENDS_ON` (story to dependency) and `IN_EPIC` (story to epic).
- Synchronization replaces one repository's projection in one transaction, so removed links leave no stale edges.
- Choose the Neo4j client: the Bolt protocol needs an async driver; the Neo4j HTTP query endpoint would fit the existing blocking `reqwest`. This spike did not read Neo4j documentation, so that choice is open.
- Owner inputs before readiness: the Neo4j instance, database, and secret binding.

ST-011 is two outcomes: a graph payload for an MCP client, and a Neo4j projection. The first could be computed from the Markdown files without Neo4j. Splitting it would deliver the display value before the Neo4j owner inputs exist.

## Unresolved questions

1. Which protocol era Claude Code 2.1.325 and Codex 0.157.1 send. The dual-era design covers both; S1 records the observed era.
2. Which working directory Codex gives a stdio server with no `cwd`. S1 documents explicit `--root` registration for Codex so the answer does not matter.
3. Whether MCP should refuse `status=done` without `story-accept` evidence. Default: mirror the CLI, which allows it. This is an owner policy decision.
4. Whether the CLI `set`, `note`, and `new` should take the same repository lock and hash check. Without it, a CLI write can still race an MCP write in the short window between the MCP hash check and its rename. Recommended as a follow-up to B1.
5. Whether a remote client will ever need Streamable HTTP. If so, it needs its own story with Origin validation, header checks, and authorization.
6. ST-011's Neo4j client and secret binding (above).

## Defect found

`story new` can lose a story. `new_story::run` takes `next_id(dir, prefix)` and then calls `list::save`, which writes a temporary file and renames it over `<id>.md` (`src/new_story.rs`, `src/list.rs`). Two processes that allocate at the same time get the same id, and the second rename silently replaces the first story. This is inferred from the code; no reproduction was run in this spike. B1 above is the proposed bug story.

## Exit criteria

| Exit criterion | Where it is met |
| --- | --- |
| A decision under `docs/adr/` compares the named designs and records boundary, transport, tool scope, and evidence. | [ADR 0006](../adr/0006-story-mcp-in-native-cli.md), with the comparison in [Design comparison](#design-comparison). |
| Typed request and response contracts define repository isolation, authorization, validation, idempotency, concurrency, errors, and redacted audit evidence. | [Contracts](#contracts). |
| Linked vertical delivery stories identify dependencies, Rust conversion scope, estimates, and criterion-level protocol and real-client verification plans, or document unresolved blockers. | Proposed in [Proposed delivery stories](#proposed-delivery-stories) and in the spike report with full criteria. Not met until the coordinator files them and links them from ST-006 Findings, because this spike may not edit `backlog/`. |

## Sources

Repository (revision `bd151a2`): `backlog/ST-006.md`, `backlog/ST-007.md`, `backlog/ST-011.md`, `docs/adr/0002-read-only-onboarding-preflight.md`, `docs/adr/0003-native-story-push.md`, `docs/adr/0005-local-planning-and-additive-adoption.md`, `native/story-push/Cargo.toml`, `native/story-push/src/{main,lib,config,list,show,set,note,new_story,next_id,secret,lint,adoption_apply}.rs`, `tools/cartographer-preflight/src/main.rs`, `tools/cartographer-preflight/docs/README.md`, `.github/workflows/test.yml`, `.story-time/settings.json`, `docs/planning/open-issues-2026-11-13.md`.

MCP specification, revision 2026-07-28, read 2026-11-14:

- [Versioning](https://modelcontextprotocol.io/specification/versioning) and [Versioning and Compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning)
- [Base protocol overview, `_meta`, error codes, statelessness](https://modelcontextprotocol.io/specification/2026-07-28/basic/index)
- [Transports overview](https://modelcontextprotocol.io/specification/latest/basic/transports)
- [stdio](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)
- [Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [Tools](https://modelcontextprotocol.io/specification/latest/server/tools)
- [Pagination](https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/pagination)

Clients and libraries, read 2026-11-14:

- [Claude Code MCP](https://code.claude.com/docs/en/mcp) (first 100,000 characters)
- [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)
- [rmcp on docs.rs](https://docs.rs/crate/rmcp/latest)
