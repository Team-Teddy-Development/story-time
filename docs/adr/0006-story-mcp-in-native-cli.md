# ADR 0006: Serve story-time MCP tools from the native `story` binary over stdio

Status: Accepted for ST-007 refinement, 2026-11-14. Spike ST-006, [issue 26](https://github.com/Team-Teddy-Development/story-time/issues/26). This decision does not mean the server is implemented.

## Context

Agents use story-time through shell commands. ST-007 asks for MCP tools that read and update stories. ST-011 asks for a Neo4j graph through the same MCP boundary. Article VIII requires Rust, and a wrapper around the historical Node CLI does not qualify.

Every `story` command is native Rust in `native/story-push` (ADR 0003, superseded in part). The commands return rendered strings, select the repository from the current directory and environment variables, and report free-text errors. `story set` and `story note` overwrite files without a lock or a precondition. The repository already has one Rust MCP, `tools/cartographer-preflight` (ADR 0002): a synchronous, hand-written stdio JSON-RPC loop with fixed targets.

The target clients are Claude Code and Codex on the owner's machine. Both launch stdio servers from a command and arguments. The current MCP revision, 2026-07-28, is stateless with per-request metadata; revisions up to 2025-11-25 use an `initialize` handshake.

The evidence, inventory, and full contracts are in the [ST-006 spike record](../spikes/ST-006-mcp-server.md).

## Decision

**Boundary.** Add `story mcp` to the existing `story` binary. The server calls typed, explicit-root functions in the `story_time_native` library. The CLI commands render their output from the same functions. Do not add a second server binary, and do not call `story` as a subprocess. The alternative, a dedicated binary on the same core, needs the same core work and adds an install artifact and version skew.

**Transport.** stdio only. The server is dual-era: it serves 2026-07-28 requests statelessly, implements `server/discover`, and answers `initialize` with 2025-11-25 semantics. The protocol layer is hand-written on `serde_json`, following the preflight. Streamable HTTP is rejected until a remote client needs it, because it requires a listener, Origin validation, header checks, and authentication that no current client needs.

**Repository isolation.** The operator names repositories with `--root` at launch; the default is `$CLAUDE_PROJECT_DIR`, else the current directory. Tools take a repository key and story ids, never paths. The server ignores `STORY_TIME_DIR`, `STORY_TIME_CONFIG`, and `STORY_TIME_PREFIX` and refuses a backlog directory outside the root.

**Tool scope.** The default server is read-only: `story_config`, `story_list`, `story_show`, and `story_lint`. With `--write`, it adds `story_update`, `story_note`, and `story_create`. Writes take the repository lock that adoption already uses, require the file's SHA-256 from a prior read, and apply the CLI's lifecycle rules with no force option. No tool pushes to a tracker, reads a tracker token, or uses the network. Resources and prompts are not exposed; the installed skills already carry the workflow text.

**Errors and audit.** Protocol faults use JSON-RPC and MCP codes (`-32700`, `-32600`, `-32601`, `-32602`, `-32022`). Domain faults are tool results with `isError: true` and a stable `code`. Write results return the before and after hashes, the changelog line, and the next publish step for the repository's `sync` policy. Every result passes a token redactor. Each call writes one value-free audit line to stderr.

**Graph.** ST-011 extends this server behind its own opt-in flag. It depends on the read slice, not on the write slice.

## Consequences

Agents can read and change stories without building shell commands, and the CLI and server cannot disagree, because they share one build and one core. The core needs a refactor before the first tool: explicit-root loading, typed results, and typed errors for list, show, config, lint, set, note, and new. Existing CLI output must stay byte-identical; the existing contract tests cover it.

The default server holds no secret and makes no network call, so operators can enable it at user scope. Publishing remains an explicit `story push`, which keeps tracker writes, credentials, and remote rollback in the one command ADR 0003 governs.

A CLI write can still race an MCP write until the CLI takes the same lock and precondition. `story new` can already replace a story when two creates race; the spike files that as a separate bug.

Supporting two protocol eras costs a small amount of code and a test per era. The hand-written protocol layer must track future MCP revisions; re-evaluate an SDK if Streamable HTTP or subscriptions become in scope.
