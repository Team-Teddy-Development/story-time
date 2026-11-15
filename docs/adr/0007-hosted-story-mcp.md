# ADR 0007: Serve `story mcp` over HTTP for hosted use across repositories

Status: Accepted, 2026-11-15. Design: [hosted-story-mcp.md](../designs/hosted-story-mcp.md), ST-128 ([issue 289](https://github.com/Team-Teddy-Development/story-time/issues/289)). ST-129 implements the HTTP transport, the client tokens, and the scopes. The other decisions below are implemented by ST-130 to ST-134, and this record does not mean they exist yet.

## Context

[ADR 0006](0006-story-mcp-in-native-cli.md) chose stdio only and rejected Streamable HTTP "until a remote client needs it". Remote clients now need it: agents on other machines, cloud sessions, and teammates must read and change stories in several repositories of one org without a checkout of each one. The design document records the review and each owner decision.

## Decision

**Transport.** `story mcp --http ADDR --clients FILE` serves the same tools over HTTP from the same binary, with a synchronous HTTP library and a fixed worker pool. stdio and HTTP share one dispatch function and one tool table. One `POST /mcp` carries one JSON-RPC message and gets one JSON response. The server offers no event stream: `GET /mcp` returns 405. A notification returns 202.

**Protocol era for each request.** The HTTP server holds no protocol state. A request with 2026-07-28 `_meta` is modern. A request without `_meta` is legacy only when its `MCP-Protocol-Version` header names a legacy revision; otherwise the server returns `-32022`. The stdio loop keeps its own era for its one connection. A present `MCP-Protocol-Version`, `Mcp-Method`, or `Mcp-Name` header must agree with the body, else the server returns `-32020`.

**Authentication and scopes.** Each client has its own bearer token of 32 random bytes. The clients file holds only the SHA-256 hash of each token, and the server compares hashes in constant time. `--http` refuses to start without `--clients`. A missing or unknown token gets HTTP 401. A client has the scopes `read`, `write`, and `comment`, and an optional list of story prefixes that limits `write` and `comment`. A hosted server registers every tool; `tools/list` shows a client only the tools its scopes allow, and a call outside its scopes returns the tool error `scope_refused`. To revoke a token, remove its hash and restart the server.

**Request limits.** An `Origin` header that is not on the allow list gets 403. A body over 1 MiB gets 413. Each request has a timeout, and a full request queue returns a retryable 503.

**Accountability.** The audit line of each hosted call names the client. It still holds no field values and no token.

**Decided here, implemented later.** These parts of the design are accepted with this record:

- Reads come from server-held bare clones of the repositories in one registry file, fetched after a TTL (ST-130).
- Links between repositories are bare IDs in `depends_on` and `relates_to`, resolved through declared prefixes (ST-131).
- Neo4j is the core graph store and answers `story_graph` (ST-132).
- Each story write lands on one branch and one pull request (ST-133).
- GitHub access uses a GitHub App, and its token reaches `git` only through the child environment (ST-130).
- A tracker comment is resolved by a marker reply that only the App's bot user can write (ST-134).

## Consequences

ADR 0006 still governs local use: the default server is stdio, read-only, and makes no network call. Its "stdio only" and "no token, no network" points do not apply under `--http`.

A hosted server is a network service. It needs a clients file, an Origin policy, and a deployment that ends TLS in front of it. The server speaks plain HTTP and must not face the internet directly.

Under `--http` with `--root`, a client with the `write` scope changes story files in that root directly, with the same lock and SHA-256 precondition as the stdio server. ST-133 replaces that path with branch-and-PR writes for registered repositories.

The request timeout ends the HTTP response, not the work: a tool call that runs past the timeout finishes in its own thread. Tool calls are bounded by their own input and file limits.

The hand-written protocol layer now has two transports to keep in step with MCP revisions. The shared dispatch keeps that cost to the HTTP front.
