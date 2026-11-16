# Hosted cross-repo story-time MCP

*   **Author:** Eric Jernigan
*   **Status:** Proposed
*   **Date:** 2026-11-15
*   **Story:** ST-128 ([#289](https://github.com/Team-Teddy-Development/story-time/issues/289)). Its Notes section records every owner and Jev decision in this document.

--------------------------------------------------------------------------------

## Overview

story-time keeps agile stories as markdown files in each repository, at `backlog/<PREFIX>-###.md`. The Rust `story` CLI (`native/story-push`) edits these files and publishes them one way to GitHub or Gitea issues.

[ADR 0006](../adr/0006-story-mcp-in-native-cli.md) and ST-011 added `story mcp` to the same binary. It is an MCP server that runs over stdio, and it lists, shows, creates, updates, notes, and lints stories in local checkouts. With `--graph`, it also copies the stories and `depends_on` links of one repository into Neo4j.

This design makes that server usable from anywhere, across all the repositories of one org. It adds four capabilities:

1. **HTTP access.** An HTTP endpoint at `storytime.teamteddy.net/mcp` on the coffee cluster, behind the team's Cloudflare tunnel. Each client gets its own token.
2. **Cross-repo links.** Links between stories in different repositories of the same org, for example from varsity-web (VE) to varsity-core (VC). The links are written in frontmatter and stored in Neo4j, which is the core graph store.
3. **Comment tools.** Tools that list the comments on a story's tracker issue and mark them resolved.
4. **Edits on the server.** Each edit lands as a branch and a pull request, so story changes still go through review.

**The proposed solution.** Run the existing `story mcp` on the cluster as one shared service. The service holds a clone of every registered repository, a Neo4j graph of every story and link, and a GitHub adapter that reads and answers comments. The markdown files stay the source of truth. Neo4j, the clones, and the tracker issues are copies that the server can rebuild.

**Terms used in this document:**

*   **Coffee cluster:** The team's Kubernetes cluster. Argo CD deploys it from the Gitea repository `team-teddy/coffee-apps`.
*   **`cc_deploy` and `cc_rollback`:** Tools of the `coffee-cluster-mcp` server. They pin an image tag in coffee-apps through a PR and wait for Argo CD to report the app as healthy.
*   **Registry org:** The set of repositories in one registry file.
*   **Forge org:** A GitHub owner. Both Varsity repositories are in one forge org, `Varsity-Engine`. A registry org can span several forge orgs, so the server gets a token for each one.
*   **Jev:** TypeSafe Jev, a typed-judgment service. A Jev score is its probability (0 to 1) that a choice is the best one, given the evidence it received.

## Problem Statement

**Agents cannot see across repositories.** Varsity work is split between varsity-core and varsity-web.

*   64 of the 313 VC story files mention VE stories, 157 times in total.
*   The mentions are only prose, such as "Paired varsity-web story for VC-109: [VE-046]".
*   No VE file links back.

No tool can answer "which web stories wait on this core story?" To answer that today, an agent clones both repositories and searches the prose with grep.

**Story tools work only where a checkout exists.** `story mcp` runs over stdio against local directories. An agent on another machine, a cloud session, or a teammate needs a checkout of every repository and a configured binary before it can read one story.

**Agents cannot read or resolve tracker comments.** Discussion about a story happens in the comments of its tracker issue. Neither the CLI nor the MCP can read those comments, and nothing records that a comment was handled. GitHub issue comments have no resolved state.

## Goals

*   **G1. Hosted access.** An authorized MCP client calls `https://storytime.teamteddy.net/mcp`. It lists, shows, and lints stories in any registered repository without a local checkout. Each response names the repository, the commit SHA, and the time of the last fetch it used.
*   **G2. Cross-repo links.** A story can name a story in another repository of the same registry org. It does this in `depends_on` or `relates_to`, with a bare ID such as `VC-109`. Lint accepts the link when exactly one repository declares that prefix. Lint rejects the link when the ID does not exist or the prefix is ambiguous.
*   **G3. Graph queries across the org.** A client asks Neo4j, through `story_graph`, which stories depend on a story or which stories it depends on, across repositories. The answer names the commit SHAs it was synced from.
*   **G4. Comments.** A client can list the comments on a story's GitHub issue, each marked resolved or unresolved. The client can resolve a comment by posting a reply that says what changed. Only the server's bot account can create the resolved state.
*   **G5. Edits through review.** A client can create or change a story. The server writes the change to one branch and one PR for each story. It applies the same lint and `set` rules as the CLI, and it returns the PR URL with `state: "proposed"`. A stale `expected_sha256` is refused, so the server never overwrites another writer's change.
*   **G6. Accountability.** Each write and each comment reply records which client made it.

## Non-Goals

*   Neo4j or the tracker as the source of truth.
*   Direct commits to a default branch.
*   Resolving PR review threads. v1 covers issue comments only.
*   **Gitea in v1.** The registry refuses `forge = "gitea"`. Gitea support is a Future Improvement.
*   Links across registry orgs.
*   Publishing tracker issues from the server. A story created on the server stays unpublished until someone runs `story push` after the merge.
*   An OAuth authorization server, a web UI, or human logins.
*   Cypher or other Neo4j concepts in tool contracts.
*   Replacing the stdio `story mcp`. It keeps working for local checkouts.
*   Automatic rewriting of prose mentions. Each repository gets a reviewed backfill PR.
*   Atomic edits across repositories. Each repository write is a separate call that opens a separate PR.

## Open Questions

*   **Freshness bound.** The working assumption is a 30-second fetch TTL as a config value, with `fetched_at` on every response. The owner can change it after S1.
*   **GitHub egress from coffee pods.** The S1 cluster check proves it. No other slice depends on it before S1.

Decisions taken while drafting this design:

*   Writes use server-held clones and push branches. This rejects Alternatives G, H, and I.
*   HTTP is served by the same `story` binary, with a synchronous HTTP library. This rejects Alternative N.
*   GitHub access uses a GitHub App.
*   Neo4j is the core graph store.

The decision record is in ST-128.

## Critical User Journeys (CUJs)

### CUJ 1: Read a story from anywhere

*   **Given**: A client holds a token with the `read` scope, and varsity-core is registered.
*   **When**: The client calls `story_show` with `repository: "varsity-core"` and `id: "VC-109"`.
*   **Then**: The response contains:
    *   the frontmatter and the body;
    *   `content_sha256`;
    *   the repository, the commit `sha`, and `fetched_at`.

    A second call within the TTL does not fetch again.

### CUJ 2: Find cross-repo blockers

*   **Given**: On varsity-web's default branch, `VE-046` has `depends_on: [VC-109]` and `relates_to: [VC-110]`.
*   **When**: A client calls `story_graph` for `VC-109` with `direction: "dependents"`.
*   **Then**:
    *   The result includes `VE-046`, with its repository.
    *   The result lists the `{repository, sha}` pairs that the graph was synced from.
    *   `story_lint` on `VE-046` raises no error for `VC-109` or `VC-110`.

### CUJ 3: Answer review feedback

*   **Given**: The GitHub issue of `VC-109` has two comments that no one has handled.
*   **When**: The client takes these steps in order:
    1.  It lists the comments.
    2.  It edits the story through the server.
    3.  It reads the story with `ref: "story-time/VC-109"`.
    4.  It resolves comment 1 with the note "Added a criterion for the empty roster case".
*   **Then**:
    *   The list shows two unresolved comments. Each comment carries `author_association`.
    *   The edit returns `state: "proposed"` and a PR URL.
    *   The `ref` read shows the edited story.
    *   A reply from the bot account appears on the issue.
    *   The next list shows comment 1 as resolved and comment 2 as unresolved.

### CUJ 4: Concurrent edits do not overwrite

*   **Given**: Two clients read `VE-046` and get `content_sha256` `A`.
*   **When**: Client 1 updates the story with `expected_sha256: A`. Then client 2 updates it with `expected_sha256: A`.
*   **Then**:
    *   Client 1's change lands on `story-time/VE-046`.
    *   Client 2 gets `conflict` with `{ref: "story-time/VE-046", current_sha256}`.
    *   Client 2's change is not written.

### CUJ 5: Access control

*   **Given**: Requests in three groups:
    *   requests with no token or an unknown token;
    *   requests with a valid token that lacks the scope a tool needs, or that falls outside the token's `prefixes`;
    *   requests from a read-only token that calls a read tool.
*   **When**: Each request calls a tool.
*   **Then**: Each group gets its own result:
    *   **No token or an unknown token:** HTTP 401 on every tool.
    *   **Missing scope or outside `prefixes`:** `scope_refused`.
    *   **Read-only token on a read tool:** the call succeeds.

    A refused request changes nothing in git or the tracker.

## Technical Plan (Architecture)

### Shape

This design adds one process: `story mcp --http` in the existing `story` binary. The process runs in one pod on the coffee cluster with a Neo4j sidecar.

stdio and HTTP share one dispatch function and one tool table, so no tool can exist on only one transport. The Ingress and ExternalSecret layout follows `coffee-cluster-mcp`. The Neo4j sidecar follows taskgraph.

```text
 MCP client ──HTTPS──> Cloudflare tunnel ──> nginx Ingress (storytime.teamteddy.net)
                                                   │  /mcp, /healthz, /readyz
                                                   ▼
 ┌──────────────────────── pod: story-time (replicas: 1, Recreate) ─────────────┐
 │  story mcp --http --registry /etc/story-time/registry.toml                   │
 │   ├─ http front: bearer → Caller{client, scopes, prefixes}; header checks    │
 │   ├─ dispatch(&self, caller, legacy) — shared with stdio                     │
 │   ├─ workspace: bare clones + per-write detached worktrees (/data, emptyDir) │
 │   ├─ OrgIndex (in memory, per SHA set) → lint                                │
 │   ├─ graph sync ──HTTP──► neo4j sidecar 127.0.0.1:7474 → story_graph         │
 │   └─ Forge (GitHub REST, App installation token)                             │
 └──────────────────────────────────────────────────────────────────────────────┘
        │ git fetch/push, REST
        ▼
  github.com (Varsity-Engine)
```

### Components

#### 1. HTTP front (same binary, synchronous)

A synchronous HTTP library (`tiny_http`) runs a fixed worker pool, with 8 workers by default. Each request has a timeout of 30 s by default. The queue is bounded, and when it is full the server returns a retryable `busy`.

The request contract:

| Request | Response |
| --- | --- |
| `POST /mcp`, JSON-RPC request | 200, `application/json`. JSON-RPC errors also return HTTP 200. |
| `POST /mcp`, notification (no `id`) | 202, empty body. |
| `MCP-Protocol-Version`, `Mcp-Method`, or `Mcp-Name` does not match the body | JSON-RPC error `-32020` (ST-006 contract). |
| Missing or unknown bearer token | 401. |
| `Origin` present and not on the allow list | 403. |
| Body over 1 MiB (the existing `LINE_LIMIT`) | 413. |
| `GET` or `DELETE /mcp` | 405. The server offers no SSE stream. |
| `GET /healthz` | 200 while the process is alive. |
| `GET /readyz` | 200 after the registry loads, otherwise 503. |

**Protocol era for each request.** Today `Server::handle` and `dispatch` take `&mut self`, and `initialize` sets a `legacy` flag for the whole process (`mcp.rs:205, 227, 233`). Both functions change to `&self` and take the era as a parameter for each request:

*   A body with modern `_meta` uses the 2026-07-28 era.
*   A body without `_meta` is legacy only when its `MCP-Protocol-Version` header names a legacy revision. Otherwise the server returns `-32022`.
*   In HTTP mode, `Server` stores no protocol state.
*   The stdio loop keeps its own `legacy` flag.
*   Shared caches each sit behind their own `Mutex`.

After the socket binds, the server prints `listening on <addr>` to stderr.

#### 2. Registry

A TOML file defines the registry: the repositories of one or more orgs. The file is mounted from a ConfigMap and parsed with the existing `toml_subset::parse`. Prefixes are declared, so a reviewer sees every prefix in the ConfigMap:

```toml
org = "varsity"
fetch_ttl_seconds = 30
[[repo]]
key = "varsity-core"
clone = "https://github.com/Varsity-Engine/varsity-core.git"
forge = "github"
default_branch = "main"        # optional; default is the remote HEAD
prefixes = ["VC", "ST"]        # varsity-core holds ST-001 and ST-002 from before its VC prefix
[[repo]]
key = "varsity-web"
clone = "https://github.com/Varsity-Engine/varsity-web.git"
forge = "github"
prefixes = ["VE"]
```

The registry file has three load-time checks:

*   It refuses `forge = "gitea"`.
*   It refuses a prefix that two repositories of one org declare.
*   Lint fails for a backlog file whose prefix is not declared, under the rule `links.undeclared_prefix`.

**Several orgs (amendment of 2026-11-16, ST-141).** v1 served one org for each process. One process now serves several:

*   A `[[repo]]` table can name its `org`. A repository with no `org` takes the top-level one, and `default` when the file has none. An org name has the form of a repository key.
*   The registry stays flat. `toml_subset` has no nested table arrays, and a flat file keeps the one-org format valid with no change.
*   A repository key is unique in the whole file. Each tool still takes `repository`, and no tool takes an org.
*   An org is the boundary of every link. The org index that lint and `story_update` use holds only the repositories of the org of the target repository. The Neo4j projection is made and read for one org at a time, and `sync` in a `story_graph` result names only the repositories of that org.
*   Two orgs can declare the same prefix. The check for a prefix that two repositories declare applies inside one org.
*   A graph node is keyed by the tracker target of its repository. When two orgs register one tracker target, the projection of the second org fails with `config_invalid`, because a shared node would join the two orgs.
*   `story_repos` gives the `org` of each repository.

Not part of this amendment: a link between two orgs, and a read limit for each client. Each client token still reads every registered repository; ST-142 adds that limit, and it must be in place before a second org with other readers goes live.

#### 3. Workspace: bare clones and freshness

**Clones.** On start, the server clones each repository with `git clone --bare` into `/data/repos/<key>.git`. The fetch refspec is `+refs/heads/*:refs/heads/*`. One mutex for each repository serializes every git operation on that repository: fetch, worktree add and remove, commit, push, and prune.

**Reads.** A read uses the default-branch commit, or the commit of a named `ref`. When the last fetch is older than `fetch_ttl_seconds`, the server fetches first. To read, the server takes three steps:

1.  It runs `git worktree add --detach --no-checkout` at that commit.
2.  It runs a sparse checkout of `backlog/` and `.story-time/` only.
3.  It parses the files and removes the worktree.

Parsed backlogs are cached by `(repository, sha)` in a bounded LRU of 64 entries.

**Responses.** Every read response carries `{repository, sha, fetched_at}`. A cross-repo answer reads each repository at its own SHA, and it lists the pair for each repository. A cross-repo answer is not one atomic snapshot. A repository that has not been cloned yet, or that the server cannot reach, returns the retryable error `repository_unavailable`.

#### 4. Writes: one branch and one PR for each story

Write tools keep the existing argument `expected_sha256` (`mcp.rs:1299-1302`).

**Branch lifecycle:**

*   **Open PR:** While story `<ID>` has an open PR, reads of that story also return `proposed: {ref, content_sha256, pr_url}`. Writes then compare `expected_sha256` against the file at `story-time/<ID>`.
*   **No open PR:** When story `<ID>` has no open PR, the next write starts from the default branch. Its one leased push replaces any leftover `story-time/<ID>` branch. (As built in ST-133: the push replaces the branch instead of a separate delete, so the remote never has a moment with no branch and no lease.)
*   **Default branch moves:** When the default branch moves under an open PR, the server does nothing. GitHub shows that PR as conflicted.

**Write steps.** For each write, the server takes these steps under the repository's mutex:

1.  It fetches.
2.  It adds a detached worktree at the head of `story-time/<ID>`, or at the default branch.
3.  It runs the existing `set`, `note`, or `new_story` code with the explicit root and the existing SHA-256 check. A mismatch returns `conflict` with `{ref, current_sha256}`.

    The lint gate stays: `status=ready` is lint-gated, and `status=done` still needs ticked criteria and evidence. `set::apply` takes the org index (Component 5), so a cross-repo dependency does not fail the gate.
4.  It commits as the App's bot user, with a `Requested-by: <client_id>` trailer.
5.  It pushes:

    ```text
    git push origin HEAD:refs/heads/story-time/<ID> --force-with-lease=refs/heads/story-time/<ID>:<expected-head-or-empty>
    ```

    A rejected push returns `conflict` and changes nothing.
6.  It opens a PR to the default branch, or updates the open PR. The PR body starts with `Stories: <ID> (#<issue>)` when the story has an issue, and with `Stories: <ID>` when it has none.
7.  It removes the worktree. On start, the server prunes worktrees that a crash left behind.

**Result.** A write result contains these fields:

*   `state: "proposed"`;
*   `pr_url`, `branch`, and `head_sha`;
*   the hashes before and after the write;
*   the lint result.

**New stories.** `story_create` uses the existing `next_id`. It already reserves IDs across every branch, and every `story-time/*` branch is a local head in the bare clone. If two creates pick the same ID, one push fails. That call fetches, allocates a new ID, and retries once. It returns `conflict` if the retry also fails.

#### 5. Cross-repo links and lint

**Schema.** The schema gains `relates_to`, a list handled like `depends_on`. A new crate-level module `src/org_index.rs` holds `OrgIndex`, which maps each story to its repository through the declared prefixes. It provides `resolve(id)`, which returns resolved, missing, or ambiguous, and `dependents_of(id)`.

**Index sources:**

*   **The CLI** passes `org: None`.
*   **Local `story mcp`** builds an index from every `--root`, so `story mcp --root varsity-core --root varsity-web` checks links without the server.
*   **The hosted server** builds the index from the registry.

**Rules:**

| Rule | `org` present | `org: None` |
| --- | --- | --- |
| `independent.deps` (`depends_on` and `relates_to`) | Missing ID is an error. A dependency that is not done keeps the existing warning. | Local behavior is unchanged for local IDs. |
| `links.ambiguous` (new) | Error for an ID whose prefix two repositories declare. | Does not run. |
| `independent.foreign_unresolved` (new) | Does not run. | Warning for an ID whose prefix the local repository does not own. |
| `spike.unblocks` | Passes when the local scan or `org.dependents_of(id)` finds a dependent. | Unchanged. |
| `links.prose_mention` (new) | Warning for a body ID with a declared prefix that is absent from `depends_on` and `relates_to`. | Does not run. |
| `links.undeclared_prefix` (new) | Error for a backlog file whose prefix its repository does not declare. | Does not run. |

The CLI output stays unchanged for a repository with no foreign IDs. The existing contract tests show this for the output they cover.

#### 6. Neo4j as the core graph store

Neo4j answers `story_graph`. It is the owner's chosen store for story relationships, because Cypher expresses directed traversals of variable depth across `DEPENDS_ON`, `RELATES_TO`, and `IN_EPIC`. It also lets people run dependency queries that the tools do not anticipate.

The existing `graph.rs` sync changes in these ways:

*   **Org-wide sync.** In hosted mode, every registry repository is synced in one transaction. The sync runs once at start, and again after any ingest that changed a SHA.
*   **Repository nodes.** The existing `StoryRepository {id}` node gains `key` and `sha`. Node identity stays the existing `identity(repo)`, and the prefix map resolves a bare ID to that identity.
*   **Edges.** `DEPENDS_ON` and `RELATES_TO` edges cross repositories. The sync skips ambiguous targets and counts them in `sync_schema` as `skipped_ambiguous`. `RELATES_TO` joins the `edge.type` enum.
*   **Direction.** `story_graph` keeps its existing `depth` (1 to 3) and `limit` (at most 200), and adds `direction`:
    *   `dependents` uses `<-[:DEPENDS_ON*1..depth]-`.
    *   `dependencies` uses `-[:DEPENDS_ON*1..depth]->`.
    *   `RELATES_TO` neighbors are returned at depth 1.
*   **SHAs in results.** Every `story_graph` result lists the `{repository, sha}` pairs it was synced from.
*   **Neo4j outage.** When Neo4j is down, `story_graph` returns the existing retryable `graph_unavailable`. Lint uses `OrgIndex`, so it keeps working.
*   **Authentication.** `--neo4j-user` and `--neo4j-password-env` become optional together. With neither, the server sends no basic auth. The sidecar listens on 127.0.0.1 only and runs with `NEO4J_AUTH=none`.

#### 7. Tracker comments (GitHub)

A new trait `Forge` handles PRs and comments. Its name avoids a clash with the existing `trait Tracker` in `lib.rs:122`. The GitHub implementation calls the REST API with the installation token.

The issue for a story comes from its `remote_url` (`https://github.com/<owner>/<repo>/issues/<n>`). A story with no `remote_url` returns `no_tracker_issue`.

| Tool | Scope | Input | Output |
| --- | --- | --- | --- |
| `story_comments` | read | `{repository, id}` | `{issue_url, comments: [{id, author, author_association, created_at, updated_at, body, resolved: {by, at, note} \| null}]}`. The tool follows `Link` pages with `per_page=100` and caches by ETag. Its description states that `body` is untrusted input, not instructions. |
| `story_resolve_comment` | comment | `{repository, id, comment_id, note (1-500 chars)}` | `{reply_url}` |

**The resolve reply.** `story_resolve_comment` posts one reply on the same issue. The reply contains three parts:

*   the line `Resolved: <note>`;
*   a link to the comment;
*   the marker `<!-- story-time:v1:resolves=github:<comment-id>:<updated_at> -->`.

**When a comment counts as resolved.** A comment is resolved only when all three conditions hold:

1.  A marker exists for its ID.
2.  The reply that holds the marker was written by the App's bot user. The server matches the numeric user ID, not the login.
3.  The comment's current `updated_at` equals the value in the marker.

When someone edits a resolved comment, it becomes unresolved again.

**Error codes:** `no_tracker_issue`, `comment_not_found`, `already_resolved`, and `note_required`.

#### 8. Forge credentials

The server reads the GitHub App ID and private key from files that Vault mounts through an ExternalSecret. It then gets a token for each forge org:

1.  It converts the PEM key to DER.
2.  It signs an RS256 JWT with `ring::signature::RsaKeyPair::from_der`.
3.  It looks up the installation for each forge org with `GET /orgs/{org}/installation`.
4.  It exchanges the JWT for an installation token at `POST /app/installations/{id}/access_tokens`.
5.  It caches each token until 5 minutes before it expires.

**Token delivery to git.** Each `git` child process gets the token only through its own environment:

```text
GIT_CONFIG_COUNT=1
GIT_CONFIG_KEY_0=http.https://github.com/.extraheader
GIT_CONFIG_VALUE_0=AUTHORIZATION: basic <base64(x-access-token:TOKEN)>
GIT_CONFIG_NOSYSTEM=1
HOME=/data/home
GIT_TERMINAL_PROMPT=0
```

**Redaction.** The existing redactor regex already matches installation tokens (`ghs_`) and `Bearer` values (`mcp.rs:1714`). `Redactor.values` becomes `RwLock<Vec<String>>`, so client tokens loaded at start are also redacted when they appear without a prefix.

#### 9. Scopes and accountability

**Client tokens.** Clients are listed in a TOML file from Vault, one entry per client: `[[client]] id, token_sha256, scopes, prefixes`. Each token is 32 random bytes in base64url form. The server hashes a presented token with `adoption::sha` and compares it with `ring::constant_time::verify_slices_are_equal`.

**Revocation.** To revoke a token, remove its hash in Vault and restart the pod. A restart is safe because the pod holds only caches.

**Tool visibility.** In hosted mode, the server registers every tool. `tools/list` returns only the tools that the caller's scopes allow. A call outside the caller's scopes returns `scope_refused`.

| Tools | Scope | Prefix check |
| --- | --- | --- |
| `story_config`, `story_list`, `story_show`, `story_lint`, `story_repos`, `story_graph`, `story_comments` | read | None; reads cover the whole org. |
| `story_update`, `story_note`, `story_create` | write | Target ID, or the prefix for `story_create`. |
| `story_graph_sync` | write | None. The tool triggers a manual rebuild. |
| `story_resolve_comment` | comment | Target ID. |

**Audit line.** The existing audit line for each call (`mcp.rs:340-384`) gains `client_id` and `sha` in hosted mode. It still holds no field values.

### Tool table, hosted mode

Hosted-only fields live in separate hosted schemas that `Backend::Hosted` selects, so local `tools/list` output does not change. The hosted-only fields are `ref`, `proposed`, `repository`, `sha`, `fetched_at`, `relates_to` in `summary`, `state`, `pr_url`, `branch`, and `head_sha`.

| Tool | Exists today | Hosted change |
| --- | --- | --- |
| `story_config`, `story_list`, `story_show`, `story_lint` | yes | Optional `ref`. Adds `{repository, sha, fetched_at}`, `proposed`, and org-aware lint. |
| `story_update`, `story_note`, `story_create` | yes | Branch-and-PR write (Component 4). Adds `state`, `pr_url`, `branch`, and `head_sha`. |
| `story_graph`, `story_graph_sync` | yes | Org-wide sync. Adds `direction` and the synced SHAs. |
| `story_repos` | new | Returns `[{key, prefixes, sha, fetched_at, state: ready\|cloning\|unavailable, last_error}]`. |
| `story_comments`, `story_resolve_comment` | new | Component 7. |

### Deployment

*   **Image.** The image is `192.168.1.51:30500/story-time:<short-sha>-amd64`. It is built on Spark, the aarch64 development machine, with this command:

    ```text
    docker buildx build --platform linux/amd64 -f native/story-push/Dockerfile .
    ```

    The command then pushes the image to the plain-HTTP registry on the LAN. The runbook is `docs/runbooks/story-time-image.md`. GitHub CI only builds the image to check the Dockerfile.
*   **Deploy and rollback.** `cc_deploy(app: "story-time", file: "k8s/story-time/30-deployment.yaml", image: "192.168.1.51:30500/story-time", tag)` pins the tag. `cc_rollback` restores the previous tag. A rollback loses nothing, because the pod holds only caches.
*   **Pod.** The pod runs one replica with `strategy: Recreate`. It has two containers:
    *   `story`, with `readOnlyRootFilesystem` and `emptyDir` volumes for `/data` (`sizeLimit: 2Gi`), `/data/home`, and `/tmp`;
    *   a `neo4j` sidecar, added in S3.
*   **Secrets.** ExternalSecrets come from `ClusterSecretStore vault-taskgraph`.
*   **Network.** The Ingress serves `storytime.teamteddy.net` (`/mcp`, `/healthz`, `/readyz`). An ingress-only NetworkPolicy admits the `ingress-nginx` namespace. Probes are `exec` probes, so the policy does not block kubelet.

### Architecture decisions to record

*   **ADR 0007 (new):** Serve `story mcp` over Streamable HTTP for hosted use across repositories. ADR 0007 supersedes ADR 0006's "stdio only" and "no network" points in hosted mode only. It covers these points:
    *   the protocol era for each request;
    *   the auth and scope model;
    *   the clone-and-PR write path and branch lifecycle;
    *   the GitHub App and git token delivery;
    *   bot-only resolve markers;
    *   Neo4j as the core graph store.
*   **ADR 0006:** Add a note that links to ADR 0007. The stdio decision still applies to local use.

## Alternatives Considered

Each reason states the main trade-off. A Jev score, where one is given, supports the reason and does not replace it. ST-128 ([#289](https://github.com/Team-Teddy-Development/story-time/issues/289)) records each decision.

### Alternative A: Neo4j or the tracker as the source of truth

*   **Pros**: One store would hold content and links. Graph queries would need no ingest step.
*   **Cons**: People edit backlog files by hand. Story changes must land through PRs. Two sources of truth would need a merge rule for every conflict.
*   **Reason for Rejection**: Losing the database would lose data. Every hand edit would also need a conflict policy. Files stay the source of truth (Jev 0.99).

### Alternative B: No database; the in-memory graph answers `story_graph`

*   **Pros**: No sidecar and no sync step. The answers would come from the same snapshot that lint uses. A full parse of about 355 stories is fast, though not measured on the cluster.
*   **Cons**: Traversals across several relationship types, and ad-hoc dependency queries, would need custom code. The approach would not grow into a general relationship store as orgs are added.
*   **Reason for Rejection**: This is an owner constraint (2026-11-15): Neo4j is the core store for story relationships. A graph database expresses directed traversals of variable depth across several relationship types in one Cypher query. It also supports ad-hoc dependency queries, and it scales as registry orgs grow.

    The accepted costs at today's size (about 355 stories, depth at most 3) are a sidecar, a sync step, and a graph snapshot that can trail lint's SHAs. To make that lag visible, every `story_graph` result returns the SHAs it was synced from. Lint uses `OrgIndex`, so a Neo4j outage affects only `story_graph`. Jev scored this alternative 0.73 against 0.25 for Neo4j. The owner overrode that score.

### Alternative C: Links stored only in Neo4j

*   **Pros**: A link could be added without a PR.
*   **Cons**: The files would no longer describe the backlog completely.
*   **Reason for Rejection**: A rebuild of the index would erase every link that is not in a file (Jev 0.02).

### Alternative D: Comments in a `## Comments` section of the file

*   **Pros**: Comments would live in git, and resolution would be a file edit.
*   **Cons**: Discussion already happens on tracker issues, and only 1 of about 355 Varsity files has such a section.
*   **Reason for Rejection**: Every comment would need a PR, and existing issue discussion would be lost (Jev 0.02).

### Alternative E: PR review threads as comments

*   **Pros**: The forge has a native "resolve" for review threads.
*   **Cons**: A thread exists only while a PR changes the story.
*   **Reason for Rejection**: A story that no one is editing would have no place for comments (Jev 0.18).

### Alternative F: Edit only the caller's local worktree

*   **Pros**: The server would hold no file-write credentials.
*   **Cons**: A remote server cannot reach local checkouts.
*   **Reason for Rejection**: Remote and cloud callers could not edit at all, so G5 would fail for them.

### Alternative G: The forge contents API for writes

*   **Pros**: The write path would hold no git state, and the forge would enforce the blob-SHA check.
*   **Cons**: Today's write code works on file paths. A write that changes several files, such as `story split`, would not be atomic.
*   **Reason for Rejection**: Server-held clones reuse the existing locked, hash-checked write code unchanged, and they keep multi-file writes in one commit.

### Alternative H: Read-only hosted server; edits stay in local stdio

*   **Pros**: The least new code, and no file-write credentials.
*   **Cons**: Cloud sessions and teammates without a checkout could not edit.
*   **Reason for Rejection**: G5 would not be met for remote callers.

### Alternative I: Return a patch for the caller to apply

*   **Pros**: The server would never write.
*   **Cons**: The caller needs a checkout.
*   **Reason for Rejection**: A remote caller often has no checkout to apply the patch to, so G5 fails for cloud sessions (Jev 0.09).

### Alternative J: Machine-user tokens, or the owner's personal tokens

*   **Pros**: Quick to set up.
*   **Cons**: Machine-user tokens need manual rotation and reach more repositories than the server needs.
*   **Reason for Rejection**: With the owner's tokens, bot replies and human comments look the same. That breaks the bot-only resolve rule. A GitHub App gives short-lived tokens, limited to the installed repositories, under a separate bot identity.

### Alternative K: Cloudflare Access alone, or an OAuth server

*   **Pros**: Cloudflare Access needs no app code. OAuth gives an identity for each user.
*   **Cons**: Cloudflare Access alone has no scopes in the app, and no team-teddy identity provider exists for OAuth.
*   **Reason for Rejection**: Cloudflare Access cannot express per-tool scopes. An OAuth server is a large new component with no identity provider behind it. Per-client hashed bearer tokens extend the single shared secret that coffee-cluster-mcp uses (Jev 0.98).

### Alternative L: Share taskgraph's Neo4j

*   **Pros**: One fewer process.
*   **Cons**: Both services would share one blast radius and one upgrade schedule.
*   **Reason for Rejection**: taskgraph's Neo4j listens only on its own pod's localhost, so another pod cannot reach it without exposing it (Jev 0.14).

### Alternative M: Fully qualified link IDs (`varsity-web#VE-046`)

*   **Pros**: An ID can never be ambiguous.
*   **Cons**: Existing prose uses bare IDs, and a qualified link breaks when a repository is renamed.
*   **Reason for Rejection**: Declared prefixes, with a load-time collision check, already make bare IDs unambiguous (Jev 0.96).

### Alternative N: A separate async server with an MCP SDK, or async HTTP in the same binary

*   **Pros**: The SDK would track new MCP revisions, and async would avoid a fixed worker pool.
*   **Cons**: A second protocol layer and version skew with the local CLI, or more build time and binary size for every laptop user.
*   **Reason for Rejection**: One binary with one dispatch keeps a single tool table for stdio and HTTP. The 8-worker pool, with timeouts and a bounded queue, serves the expected handful of agent clients.

### Alternative O: Webhooks to keep the index fresh

*   **Pros**: Reads would never wait for a fetch.
*   **Cons**: Each repository needs a webhook and a public receiver that checks signatures. A reconciler is still needed for missed events.
*   **Reason for Rejection**: Fetch on read with a TTL bounds staleness with no extra public endpoint (Jev 0.80).

--------------------------------------------------------------------------------

## Risks & Mitigations

| Risk | Impact | Mitigation | Residual |
| --- | --- | --- | --- |
| Single replica | Each deploy causes a short outage. | `Recreate`. Readiness depends only on the registry load. A repository that is still cloning returns `repository_unavailable`. | Seconds of downtime per deploy. |
| Worker pool saturation | Slow forge calls delay reads. | A timeout on each request, a bounded queue, and a retryable `busy`. | Slow replies when more than 8 forge calls run at once. |
| Cold start on `emptyDir` | Clones and Neo4j are rebuilt after each restart. | Sparse worktrees, sync once at start, and `repository_unavailable` until each repository is ready. | Graph answers are unavailable until the first sync. |
| GitHub App blast radius | A leaked client token with `write` scope can open PRs and post replies. | The App is installed on registered repositories only. Clients have their own scopes and `prefixes`. Writes carry a `Requested-by` trailer. Revocation is a hash removal plus a restart. | Edits stay possible until the token is revoked. |
| Untrusted comment text | Comment text can steer an agent that holds write scope. | `body` is marked untrusted, and `author_association` is returned. | Low: both repositories are private, so only members can comment. |
| Duplicate resolve | Two concurrent resolves post two markers. | None needed. Both markers are valid bot markers, and either one resolves the comment. | A redundant reply. |
| Graph lag | `story_graph` can trail the SHAs that lint uses. | Every result returns the SHAs it was synced from. | A short lag after each fetch. |
| GitHub egress | Coffee pods might not reach GitHub. | The S1 cluster check proves it before S2 to S5 build on it. | None after S1. |

## Non-Functional Requirements

*   **Security & Privacy**:
    *   Client tokens are stored only as SHA-256 hashes and compared in constant time.
    *   The App key comes from Vault through an ExternalSecret.
    *   Tokens never appear in argv, on disk, or in clones. They appear only in the environment of `git` child processes.
    *   The redactor covers `ghs_` tokens, `Bearer` values, and the client tokens that the server loaded.
    *   Neo4j listens on 127.0.0.1 only.
*   **Performance & Latency**:
    *   **Reads within the TTL:** answered from the SHA cache. Target: below 200 ms at p95.
    *   **Reads that fetch:** add the time of one `git fetch`. That time will be measured in S1.
    *   **Writes:** one fetch, one commit, one push, and one PR call. Target: below 5 s at p95.
    *   **Container limits:** `story` gets 768 Mi, which matches taskgraph core. `neo4j` gets 1.5 Gi, with a 512m heap and a 256m pagecache, which matches taskgraph's sidecar.
    *   **Comment listing:** uses ETags against the GitHub limit of 5,000 requests per hour for each installation.
*   **Rollout & Rollback**:
    *   Hosted mode needs `--http`, so local stdio users see no change.
    *   Each slice deploys on its own through `cc_deploy`. `cc_rollback` restores the previous tag.

## Future Improvements

*   Gitea support through the existing `gateway::Gitea::request` client, with a Gitea bot user.
*   Push webhooks with a stale-while-revalidate cache.
*   Cloudflare Access in front of the hostname, rate limiting, and client tokens that expire.
*   Egress pinning through an FQDN-capable policy or an egress proxy.
*   Atomic edits across repositories, run as a saga under `skills/story-time/references/backends.md`.
*   Several registry orgs in one process.
*   Publishing tracker issues from the server.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Each slice below is its own story with Given/When/Then criteria and its own PR. All executable code is Rust (Constitution Article VIII). story-time changes go to `Team-Teddy-Development/story-time`, and deployment changes go to Gitea `team-teddy/coffee-apps`.

### Slices

| Slice | Delivers | Depends on |
| --- | --- | --- |
| S0 | ADR 0007; `&self` dispatch with the protocol era taken from each request; `--http` with the header checks; hashed client tokens and scopes. CI only; no cluster deploy. | none |
| S1 | Registry with declared prefixes; bare clones; TTL fetch; SHA cache; `ref`; `story_repos`; read-only GitHub App token and git token delivery; image runbook; first cluster deploy. | S0 |
| S2 | `OrgIndex`; `relates_to`; org-aware lint; `set::apply` with `org`; backfill PRs in varsity-core and varsity-web. | S0 |
| S3 | Org-wide Neo4j sync with cross-repo edges; `direction`; the Neo4j sidecar. | S1, S2 |
| S4 | App write permissions; branch-and-PR writes with the branch lifecycle; `story_create` retry. | S1, S2 |
| S5 | `story_comments` and `story_resolve_comment` on GitHub. | S4 |

### Target Files to Create/Modify

#### story-time repository

##### 1. `docs/adr/0007-hosted-story-mcp.md` (Create, S0)

*   **Type / Scope**: Architecture decision record.
*   **Proposed Changes**: Records the decisions listed under "Architecture decisions to record".

##### 2. `docs/adr/0006-story-mcp-in-native-cli.md` (Modify, S0)

*   **Type / Scope**: ADR status note.
*   **Proposed Changes**: Add: "Hosted mode is governed by ADR 0007; the stdio decision is unchanged."

##### 3. `docs/designs/hosted-story-mcp.md` and `docs/designs/hosted-story-mcp-review/` (Modify; lands under ST-128)

*   **Type / Scope**: This design and its review reports.

##### 4. `native/story-push/Cargo.toml` and `native/story-push/Cargo.lock` (Modify, S0 and S1)

*   **Type / Scope**: Build configuration.
*   **Proposed Changes**:
    *   S0 adds a pinned `tiny_http`.
    *   S1 adds a pinned `base64`.
    *   The existing `ring` and blocking `reqwest` are reused.
    *   No async runtime is added.

##### 5. `native/story-push/src/mcp.rs` (Modify, S0, S1, S4, S5)

*   **Type / Scope**: MCP server core.
*   **Proposed Changes**:
    *   **Dispatch:** Change `handle(&mut self, line)` and `dispatch(&mut self, …)` to `&self`, with parameters `caller: &Caller, legacy: bool`. The stdio `serve` loop keeps its own `legacy`.
    *   **Backend:** Replace `Vec<Root>` with a `Backend` enum: `Local(Vec<Root>)` or `Hosted(Workspace)`. A hosted read builds `Root { key, path }` from a parsed tree, so the existing `root()`, `open()`, `audit()`, and `story_config` code is reused.
    *   **Flags:** `run` and `roots()` accept the new flags: `--http`, `--registry`, `--clients`, `--allowed-origin`, `--workers`, `--request-timeout-seconds`, and `--github-api-url`.
    *   **Tool visibility:** In hosted mode, every tool is registered, and `tools/list` is filtered by scope. A call outside the scope returns `scope_refused`.
    *   **Schemas:** Hosted schemas are separate from local schemas.
    *   **New tools:** Register `story_repos` (S1), plus `story_comments` and `story_resolve_comment` (S5).
    *   **Redactor:** `Redactor.values` becomes `RwLock<Vec<String>>`.
    *   **Audit line:** Gains `client_id` and `sha` in hosted mode.
    *   **Regression check:** `tests/mcp_protocol.rs` passes unchanged.

##### 6. `native/story-push/src/mcp/http.rs` (Create, S0)

*   **Type / Scope**: HTTP front.
*   **Proposed Changes**:
    *   Implements the request contract table in Component 1.
    *   Uses the worker pool with a bounded queue and a timeout for each request.
    *   Prints `listening on <addr>` after the socket binds.

##### 7. `native/story-push/src/mcp/caller.rs` (Create, S0)

*   **Type / Scope**: Client identity and scopes.
*   **Proposed Changes**:
    *   Defines `Caller { client_id, scopes, prefixes }`.
    *   Loads the clients TOML with `toml_subset::parse`. S0 confirms that `toml_subset` supports arrays of tables, or adds that support.
    *   Hashes tokens with `adoption::sha` and compares them with `ring::constant_time::verify_slices_are_equal`.
    *   Provides `permits(scope, target: Option<&str>)`.

##### 8. `native/story-push/src/mcp/registry.rs` (Create, S1)

*   **Type / Scope**: Registry.
*   **Proposed Changes**:
    *   Parses the registry TOML from Component 2.
    *   Refuses `forge = "gitea"` and duplicate prefixes.
    *   Builds a map from each prefix to its repository.

##### 9. `native/story-push/src/mcp/workspace.rs` (Create, S1 and S4)

*   **Type / Scope**: git clones, freshness, and writes.
*   **Proposed Changes**:
    *   Runs the system `git` with the environment from Component 8.
    *   **S1:**
        *   `ensure_clone` runs `clone --bare` with the heads-only refspec.
        *   `fresh(key)` fetches the repository when its TTL has expired.
        *   `parse_at(key, sha)` adds a detached sparse worktree, parses it, and removes the worktree. Results go into a 64-entry LRU.
        *   One mutex for each repository.
        *   `prune()` runs on start.
    *   **S4:**
        *   `write(key, story_id, expected_sha256, op)` follows Component 4, including the branch lifecycle.
        *   It also checks for an open PR for each story, and replaces the branch when no PR is open.

##### 10. `native/story-push/src/org_index.rs` (Create, S2)

*   **Type / Scope**: The crate-level org index (`pub(crate)`).
*   **Proposed Changes**:
    *   Defines `OrgIndex { stories, prefix_map }`.
    *   Provides `resolve(id) -> Resolved | Missing | Ambiguous` and `dependents_of(id)`.
    *   Builds from the registry, or from every local `--root`.
    *   Holds unit tests in a `#[cfg(test)]` module: resolved, missing, and ambiguous IDs; a VE story that depends on a VC spike; duplicate declared prefixes.

##### 11. `native/story-push/src/mcp/forge_auth.rs` (Create, S1)

*   **Type / Scope**: GitHub App tokens.
*   **Proposed Changes**:
    *   Converts the PEM key to DER, signs the RS256 JWT, looks up each installation, exchanges the JWT for a token, and caches the token (Component 8).
    *   Builds the `git` child environment.
    *   Holds `#[cfg(test)]` checks:
        *   The JWT claims are `iat` 60 s in the past, `exp` 9 minutes ahead, and `iss` set to the App ID, and the signature verifies against a test key.
        *   A cached token is reused before expiry and refreshed within 5 minutes of it.
        *   The built environment holds the token only in `GIT_CONFIG_VALUE_0`.

##### 12. `native/story-push/src/mcp/forge.rs` (Create, S4 and S5)

*   **Type / Scope**: The PR and comment adapter.
*   **Proposed Changes**:
    *   **Trait:** `Forge` with `open_or_update_pr`, `open_pr_for(branch)`, `list_comments(issue)`, and `post_comment(issue, body)`.
    *   **GitHub:** REST v3 with the installation token, and `--github-api-url` for tests.
    *   **Issue coordinates:** parsed from `remote_url`.
    *   **Markers:** formatting and parsing for Component 7, matching the bot by its numeric user ID.

##### 13. `native/story-push/src/mcp/graph.rs` (Modify, S3)

*   **Type / Scope**: The Neo4j projection.
*   **Proposed Changes**: Every change in Component 6:
    *   the optional `--neo4j-user` and `--neo4j-password-env` pair;
    *   org-wide sync, with `StoryRepository` gaining `key` and `sha`;
    *   cross-repo `DEPENDS_ON` and `RELATES_TO` edges;
    *   `direction` with the directed Cypher queries;
    *   `RELATES_TO` in the `edge.type` enum, and `skipped_ambiguous` in `sync_schema`;
    *   sync at start and after any SHA change;
    *   the reuse of `graph_unavailable`.

##### 14. `native/story-push/src/lint.rs` (Modify, S2)

*   **Type / Scope**: Lint rules.
*   **Proposed Changes**:
    *   `Context` gains `org: Option<&OrgIndex>`.
    *   The rules change as the table in Component 5 shows.
    *   `RULES` grows from 16 to 20 entries: `links.ambiguous`, `independent.foreign_unresolved`, `links.prose_mention`, and `links.undeclared_prefix`.
    *   The `Context` built at `lint.rs:531` passes `org: None`.

##### 15. `native/story-push/src/set.rs`, `native/story-push/src/saga_tests.rs`, `native/story-push/src/story.rs`, `native/story-push/src/lenient_story.rs` (Modify, S2)

*   **Type / Scope**: The story schema and `set`.
*   **Proposed Changes**:
    *   `set::apply` gains `org: Option<&OrgIndex>`. The CLI caller passes `None` (`set.rs:50`), and the hosted `story_update` passes the index (`mcp.rs:1040`). The `Context` built in `set.rs:133` and `saga_tests.rs:66` passes it through.
    *   `relates_to` is added after `depends_on` in both `KEY_ORDER` lists:
        *   `story.rs`, the push renderer;
        *   `lenient_story.rs`, the file writer used by `list::save`.

        In both, `relates_to` is handled as a list.

##### 16. `skills/story-time/scripts/src/domain/story.mjs` and `skills/story-time/scripts/src/application/update-story.mjs` (Modify, S2)

*   **Type / Scope**: Node parity.
*   **Proposed Changes**:
    *   Add `relates_to` to the key order (`story.mjs:11`).
    *   Add `relates_to` to the list fields (`update-story.mjs:6`).

##### 17. `skills/story-time/scripts/usage.txt`, `skills/story-time/SKILL.md`, `README.md` (Modify, S0)

*   **Type / Scope**: Help and documentation. `help.rs` only includes `usage.txt`.
*   **Proposed Changes**:
    *   Document `story mcp --http` and its flags.
    *   Add a "Hosted MCP" section to the README, with the client registration (URL plus bearer header) for Claude Code and Codex.

##### 18. `native/story-push/Dockerfile` (Create, S1)

*   **Type / Scope**: The container image.
*   **Proposed Changes**:
    *   The build context is the repository root, because `help.rs` includes a file outside the crate.
    *   The builder stage runs `cargo build --release --locked --manifest-path native/story-push/Cargo.toml --bin story`.
    *   The runtime base is `debian:bookworm-slim` plus `git` and `ca-certificates`.
    *   The image runs as a non-root user, with the entrypoint `story mcp --http 0.0.0.0:8080`.

##### 19. `docs/runbooks/story-time-image.md` (Create, S1) and `.github/workflows/test.yml` (Modify, S1)

*   **Type / Scope**: Image build path.
*   **Proposed Changes**:
    *   **Runbook:** The commands run on Spark:

        ```text
        docker buildx build --platform linux/amd64 -f native/story-push/Dockerfile -t 192.168.1.51:30500/story-time:<short-sha>-amd64 .
        docker push 192.168.1.51:30500/story-time:<short-sha>-amd64
        ```

        The runbook also covers the insecure-registry setting and the `cc_deploy` call.
    *   **CI:** Adds a job that runs `docker build -f native/story-push/Dockerfile .` with no push.

##### 20. `native/story-push/tests/mcp_http.rs` (Create, S0)

*   **Type / Scope**: Subprocess tests.
*   **Proposed Changes**: The test starts `story mcp --http 127.0.0.1:0 --root <fixture> --clients <fixture>` and reads the bound address from stderr. It asserts these cases:
    *   **Status codes:** 401 with no token, 403 for a disallowed origin, 413 for a large body, 405 for `GET /mcp`, and 202 for a notification.
    *   **Header checks:** `-32020` when a header does not match the body.
    *   **Scopes:** A read-only token gets `scope_refused` on `story_update`, and `tools/list` hides `story_update` from that token.
    *   **Transport parity:** `story_show` returns the same `structuredContent` over HTTP as over stdio.
    *   **Protocol era:** A legacy client and a modern client interleave without changing each other's era.

##### 21. `native/story-push/tests/mcp_hosted_workspace.rs` (Create, S1 and S4)

*   **Type / Scope**: Subprocess tests with `file://` bare-repository fixtures and a local HTTP GitHub stub, selected with `--github-api-url`.
*   **Proposed Changes**:
    *   **S1:** A fetch is skipped when the TTL holds, and runs when it expires. The SHA changes after an upstream commit. `ref` reads a branch. `story_repos` reports `state` for each repository. A repository that cannot be reached returns `repository_unavailable` while `/readyz` stays 200.
    *   **S4:**
        *   A write creates `story-time/<ID>` with one commit, the trailer, and the bot author, and the stub records one PR call.
        *   A second write after an open PR compares against the branch file.
        *   A stale `expected_sha256` returns `conflict` with the ref.
        *   An upstream push between fetch and push gives a lease rejection, which returns `conflict`.
        *   With no open PR, the next write starts from the default branch.
        *   Two concurrent `story_create` calls get different IDs.
        *   `prune` removes worktrees that a crash left behind.

##### 22. `native/story-push/tests/lint_org.rs` (Create, S2)

*   **Type / Scope**: Subprocess tests through `story mcp --root A --root B` and `story lint`.
*   **Proposed Changes**:
    *   `depends_on: [VC-109]` in a VE story passes, and `[VC-999]` fails as missing.
    *   A third root that also declares VE fails with `links.ambiguous`.
    *   A body mention of `VC-109` that is not linked warns with `links.prose_mention`.
    *   Local `story lint` in the VE repository alone warns with `independent.foreign_unresolved`, and no error appears.
    *   The existing `tests/lint_contract.rs` passes unchanged.

##### 23. `native/story-push/tests/mcp_graph.rs` (Modify, S3)

*   **Type / Scope**: Graph contract tests on the existing Docker-backed Neo4j harness.
*   **Proposed Changes**: Add cases for:
    *   org-wide sync with cross-repo `DEPENDS_ON` and `RELATES_TO`;
    *   `skipped_ambiguous`;
    *   `direction: dependents` and `direction: dependencies`;
    *   synced SHAs in results;
    *   `graph_unavailable` after `Neo4j::stop()`;
    *   `--graph` with no credentials against a Neo4j started with `NEO4J_AUTH=none`.

##### 24. `native/story-push/tests/mcp_comments.rs` (Create, S5)

*   **Type / Scope**: Subprocess tests against the local GitHub stub.
*   **Proposed Changes**:
    *   **Listing:** A story with no `remote_url` returns `no_tracker_issue`. Comments list with `resolved: null` and `author_association`. A `304` response is served from the ETag cache.
    *   **Resolving:** A resolve posts exactly one reply with the marker and the note. An empty note returns `note_required`. A second resolve returns `already_resolved`.
    *   **Resolved state:** A marker written by a user ID other than the bot is ignored. An edited original, with a new `updated_at`, reads as unresolved.
    *   **Secrets:** No token value appears in any output.

##### 25. `backlog/ST-###.md` (Create, six files)

*   **Type / Scope**: Stories, one for each slice from S0 to S5.
*   **Proposed Changes**:
    *   Each story has `depends_on` as in the slice table, and criteria taken from the CUJs and from items 20 to 24.
    *   The coordinator allocates the IDs after `git fetch --prune` and a check of open PRs, once ST-128 merges.
    *   Each story is pushed under Rule 4: publish to the tracker immediately after each story change.

#### coffee-apps repository (Gitea `team-teddy/coffee-apps`)

##### 26. `argocd/story-time.yaml` (Create, S1)

*   **Type / Scope**: Argo CD Application. It follows `argocd/coffee-cluster-mcp.yaml` and points at `k8s/story-time`.

##### 27. `k8s/story-time/` manifests (Create in S1; Modify in S3 and S4)

*   **Type / Scope**: Kubernetes manifests, laid out like `k8s/coffee-cluster-mcp/`.
*   **Proposed Changes**:
    *   **S1:**
        *   `kustomization.yaml` and `00-namespace.yaml` (`story-time`).
        *   `10-serviceaccount.yaml`, with `automountServiceAccountToken: false`.
        *   `20-externalsecret.yaml`, from `ClusterSecretStore vault-taskgraph`. Keys: `global/infra.story-time.clients`, `global/infra.story-time.github-app-id`, and `global/infra.story-time.github-app-key` (`property: value`). Mount: `/etc/story-time/secrets/`.
        *   `25-configmap.yaml`, holding the registry TOML.
        *   `30-deployment.yaml`. Settings:
            *   `replicas: 1` and `strategy: Recreate`.
            *   Image `192.168.1.51:30500/story-time:<tag>`.
            *   `args: ["mcp", "--http", "0.0.0.0:8080", "--registry", "/etc/story-time/registry.toml", "--clients", "/etc/story-time/secrets/clients", "--github-app-id-file", "/etc/story-time/secrets/github-app-id", "--github-app-key-file", "/etc/story-time/secrets/github-app-key"]`.
            *   `readOnlyRootFilesystem: true`, non-root.
            *   `emptyDir` volumes for `/data` (`sizeLimit: 2Gi`), `/data/home`, and `/tmp`.
            *   Requests of 100m CPU and 256Mi memory, and a 768Mi limit.
            *   `exec` probes: `story mcp probe --url http://127.0.0.1:8080/healthz` for liveness, and `/readyz` for readiness.
        *   `40-service.yaml`, mapping port 80 to 8080.
        *   `50-ingress.yaml`, for host `storytime.teamteddy.net`, with `/mcp`, `/healthz`, and `/readyz` and the annotations from coffee-cluster-mcp.
        *   `60-networkpolicy.yaml`, ingress only, from the `ingress-nginx` namespace to port 8080.
    *   **S3:**
        *   Adds the `neo4j` sidecar container, with taskgraph's settings:
            *   image `neo4j:5.26-community`;
            *   `NEO4J_AUTH=none`;
            *   `NEO4J_server_default__listen__address=127.0.0.1`;
            *   heap of 512m and pagecache of 256m;
            *   `runAsUser: 7474`;
            *   a 1536Mi limit;
            *   an `emptyDir` store.
        *   The sidecar has no `readOnlyRootFilesystem`.
        *   Adds `--graph --neo4j-url http://127.0.0.1:7474` to the `story` args.
    *   **S4:** No manifest change. The App's write permissions are an owner handoff.

`--github-app-id-file` and `--github-app-key-file` are added to the flag list in item 5.

#### Owner handoffs (no repository file)

##### 28. GitHub App and Vault (S1 and S4)

*   **Type / Scope**: Secrets and forge setup. Agents have no access to these values.
*   **Proposed Changes**:
    *   **S1:**
        *   Create the GitHub App `storytime`, with Contents: read and Metadata: read.
        *   Install it on `Varsity-Engine/varsity-core` and `Varsity-Engine/varsity-web`.
        *   Store `global/infra.story-time.github-app-id`, `global/infra.story-time.github-app-key`, and `global/infra.story-time.clients` in Vault. `clients` is the clients TOML with token hashes.
    *   **S4:**
        *   Add Contents: write, Pull requests: write, and Issues: write.
        *   Create `Varsity-Engine/story-time-sandbox` and install the App on it, for the S4 and S5 cluster checks.

### Verification Plan

*   **Build**:
    *   `cargo build --release --locked --manifest-path native/story-push/Cargo.toml`
    *   `docker build -f native/story-push/Dockerfile .` (CI)
*   **Tests**: `cargo test --locked --manifest-path native/story-push/Cargo.toml`. The existing contract tests pass unchanged, which shows that the local CLI and stdio output they cover is unchanged. The new suites are items 10, 11, and 20 to 24.
*   **Static analysis**:
    *   `cargo clippy --manifest-path native/story-push/Cargo.toml --locked --all-targets -- -D warnings`
    *   `cargo fmt --manifest-path native/story-push/Cargo.toml --check`
*   **Story gates**: `story lint` passes on each slice story, and each PR body starts with its `Stories:` line.
*   **Cluster checks.** Run them after `cc_deploy` reports Synced and Healthy:
    *   **S0:** None. S0 is checked in CI only.
    *   **S1:**
        *   `curl -s https://storytime.teamteddy.net/healthz` returns 200.
        *   `story_repos` lists both repositories, each with a SHA and `fetched_at`.
        *   `story_show VC-109` with a read token returns the story and `content_sha256`, and returns 401 without a token.
        *   After an upstream commit, `story_show` returns the new `sha` within the TTL.
        *   After `kubectl -n story-time delete pod`, the same `sha` returns, if no upstream commit landed in between.
        *   `kubectl exec … -- curl -sI https://api.github.com` returns 200, which shows REST egress.
    *   **S2:**
        *   The backfill PRs in varsity-core and varsity-web pass `story lint`.
        *   Hosted `story_lint` on the linked VE stories reports no `independent.deps` errors.
        *   A hosted `story_update status=ready` on a VE story with a VC dependency succeeds.
    *   **S3:**
        *   `story_graph` for `VC-109` with `direction: "dependents"` returns the linked VE stories and their synced SHAs.
        *   Stopping the Neo4j container makes only `story_graph` return `graph_unavailable`, while `/readyz` stays 200.
    *   **S4:**
        *   A write against `Varsity-Engine/story-time-sandbox` returns a PR URL. The PR shows the bot author and the `Requested-by` trailer.
        *   Repeating the write with the old `expected_sha256` returns `conflict`.
    *   **S5:**
        *   On a sandbox issue, a resolve posts one bot reply, and `story_comments` then shows the comment as resolved.
        *   A manual edit of the comment shows it as unresolved again.

--------------------------------------------------------------------------------

*Drafted with EGM*
