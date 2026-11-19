# Stage 5 Initial Zoom Out report: hosted-story-mcp.md

* **Target:** `docs/designs/hosted-story-mcp.md` (lines 1 to 609)
* **Reviewers:** `egm-zoom-out`, `egm-inventive-catalyst`, run in parallel with fresh context
* **Context:** ADR 0006, the ST-006 spike, `mcp.rs`, `mcp/graph.rs`, the coffee-cluster-mcp manifests, and the taskgraph `20-statefulset.yaml`
* **Verdict:** Context sufficiency passes. There are 0 Blockers, 15 Majors after deduplication, and 12 Nits.
* **Spot checks by the author:** These claims were confirmed in code:
  * `mcp.rs:233` sets `self.legacy = true` for the life of the process.
  * `gateway.rs:23` already has `enum Backend { Github, … }`.
  * `next_id.rs:64` has `reserved_ids`.
  * coffee-cluster-mcp pulls its image from `192.168.1.51:30500`.

## Major findings

### Contradictions (the design cannot work as written)

| ID | Lines | Finding | Proposed fix | Lenses |
| --- | --- | --- | --- | --- |
| Z1 | 121, 385; `mcp.rs:156, 233, 265` | **Process-wide `legacy` flag.** One HTTP client's `initialize` switches protocol handling for every other client. `&mut self` dispatch also forces a mutex, so 8 workers act as 1. The design claims the server is stateless per request. | Take the protocol era from each request: the `MCP-Protocol-Version` header or `_meta`. `Server` becomes `&self` with no mutable fields. stdio keeps its flag in the stdio loop. Also check `Mcp-Method` and `Mcp-Name` against the body, with error `-32020`, as ST-006 requires. | both |
| Z2 | 180, 305, 434 | **The git credential helper has no channel to the token.** The token is cached only in server memory. The design forbids passing it through argv, child-process environment, or disk. | Choose one channel: a Unix socket (mode 0600) that the server owns, OR git config through the `GIT_CONFIG_COUNT`/`KEY`/`VALUE` environment variables with `http.extraHeader`, which relaxes the environment rule. | both |
| Z3 | 64, 78-82, 140-150 | **Read and write use different bases.** Reads default to main, but writes compare `base` against the head of `story-time/<ID>`. After the first write, every later write returns `conflict`. A stale branch after a merge is also reused. | Reads return `proposed: {ref, sha256, pr_url}` when a story branch is open. `conflict` names the ref its hash belongs to. A write starts again from the default branch when the branch has no open PR. | catalyst |
| Z4 | 199, 308, 582 | **The NetworkPolicy cannot name `api.github.com`.** A Kubernetes policy supports only `ipBlock` CIDRs. GitHub's ranges are wide and they change. Neither sibling service pins egress, and coffee-cluster-mcp reaches Gitea in-cluster at `gitea.gitea.svc:3000`. | Use an ingress-only policy, like taskgraph's. Drop egress pinning in v1, or use an FQDN-capable CNI or an egress proxy, and state which one. Use the in-cluster Gitea URL, rewritten with `insteadOf`. | both |
| Z5 | 337-342, 54, 576 | **Credentials arrive after the slices that need them.** S0 to S3 clone and fetch, but the App credential arrives only in S4. Private repositories would fail. | Find out whether the repositories are private. If they are, move a read-only App credential and the credential channel to S0 or S1. | zoom-out |

### Scope and duplication (the design can be smaller)

| ID | Lines | Finding | Proposed fix | Lenses |
| --- | --- | --- | --- | --- |
| Z6 | 158-164, 215-219, 340, 443-451, 577 | **Neo4j duplicates the in-memory index.** `OrgIndex` already holds every story and every edge. A BFS to depth 3 over about 355 nodes is small. Neo4j brings a sync step, `index_unavailable`, a lint/graph SHA mismatch, and a 1.5 Gi sidecar on an `emptyDir` that empties on each restart. | Answer `story_graph` from `OrgIndex`. Keep Neo4j as a write-only projection of the cross-repo edges, which still meets the owner mandate. **Owner decision.** | both |
| Z7 | 152-154, 337-339 | **Cross-repo links are coupled to hosting.** Local stdio already accepts several `--root` options. S2 waits on S1 (mirrors) for no technical reason. | Build `OrgIndex` from all `--root` options in local mode. S2 then depends only on the lint and index work, so cross-repo value ships before the cluster. | zoom-out |
| Z8 | 15, 110, 166-180, 342, 441, 544, 582, 591 | **Gitea is speculative in v1.** The only registered org is Varsity, and both of its repositories are on GitHub. | Scope v1 to GitHub. Refuse `forge = "gitea"` when the registry loads. Move the Gitea bot, adapter, and egress to Future Improvements. | zoom-out |
| Z9 | item 12, line 441 | **`gateway.rs` is ignored.** It already has `Backend::{Github, Ado, Gitea}`, a Gitea REST client, pagination, and tests. GitHub there uses the `gh` CLI. | Extend `Gateway` with comment and PR methods and a token-injected GitHub HTTP backend, or state why a parallel adapter is needed. | zoom-out |
| Z10 | 138, 409 | **The prefix map is derived from the files and can become ambiguous without warning.** One copied VC file in varsity-web would make every `VC-*` link in the org ambiguous. | Declare `prefixes = [...]` on each registry entry, so the list is reviewed in the ConfigMap. Lint checks that each backlog file matches the declared prefixes. | zoom-out |

### Operations

| ID | Lines | Finding | Proposed fix | Lenses |
| --- | --- | --- | --- | --- |
| Z11 | 52, 127, 140, 310 | **The network fetch is on the read path.** A TTL miss makes the caller wait about 1 s. During a forge outage, reads fail even though the mirror holds a usable SHA. | A background refresher runs `ls-remote` and fetches only on change. Reads always serve the last good SHA with `fetched_at`. `story_repos` reports `last_error`. | catalyst |
| Z12 | 115, 143, 291, 312 | **One pool of 8 workers serves forge calls that take 1 to 5 s.** Writes or a slow GitHub API starve cached reads. | Use a read pool and a forge pool, each with a bounded queue. Return a retryable `busy` error when a queue is full. Use `try_lock` on the story mutex and return `lock_held` on timeout. | catalyst |
| Z13 | 100, 115, 197, 497, 573, 577 | **The single-replica choice is treated as fixed.** Each deploy causes downtime while every repository clones in full. One unreachable repository leaves the whole pod not ready. `--force-with-lease` and push-to-reserve already give remote compare-and-swap. | Readiness means the HTTP server and the registry are loaded. A missing repository returns retryable `repository_unavailable`. Put the mirrors on a small PVC, or run 2 replicas with RollingUpdate and the lease as the lock. | catalyst |
| Z14 | 53, 487, 571, 592 | **Known cluster facts appear as open items.** The image registry is `192.168.1.51:30500`. ExternalSecrets use `ClusterSecretStore vault-taskgraph` with `global/infra.*` keys. Gitea is reachable in-cluster. | Write these as facts. Reduce the egress question to GitHub only. | zoom-out |
| Z15 | 273 | **The auth parity claim is wrong.** coffee-cluster-mcp uses one shared `MCP_SECRET`, not hashed tokens for each client. | Change the text: per-client hashed tokens are a new pattern. | catalyst |

## Nits

| ID | Lines | Finding | Fix |
| --- | --- | --- | --- |
| N1 | 290, 376 | The laptop binary gets `tiny_http`, JWT code, and adapters it never uses. | Put them behind a Cargo feature `hosted`. |
| N2 | 164, 190, 449 | `story_graph` already has `depth`, with a maximum of 3 and 200 nodes. Only `direction` is new. | Correct the text. |
| N3 | 158, 447; `graph.rs:272` | The existing nodes are `StoryRepository` keyed by tracker identity. The design adds `Repository {key, sha}`. | Reuse `StoryRepository`, add `sha`, and name the key. |
| N4 | 164, 574; `graph.rs:55-107` | `--graph` requires a user and a password variable today, which conflicts with `NEO4J_AUTH=none`. | Make the credentials optional for a loopback URL, or pass a dummy variable. |
| N5 | 417, 421, 577 | Full worktrees of application repositories go into a 2 Gi `emptyDir`. | Use `--no-checkout` plus sparse checkout of `backlog/` and `.story-time/`. |
| N6 | 150, 469 | `next_id_reserved` duplicates `next_id::reserved_ids`, which already scans every branch. | Reuse `next_id` unchanged. |
| N7 | 143 | The file lock in a throwaway worktree never contends with anything. | Drop it, or explain what it protects. |
| N8 | 156, 161, 448, 460 | `MENTIONS` edges and `remote.stale` trace to no Goal or CUJ. Prose mentions are handled three ways. | Drop both from v1. |
| N9 | 33, 152, 467 | No CUJ uses `relates_to`. | Add a "paired story" CUJ, or defer the field. |
| N10 | 190 | `story_graph_sync` is redundant in hosted mode, because sync runs automatically. | Leave it out, or describe it as a manual rebuild. |
| N11 | 337, 576 | The S0 init-container path is removed in S1. | S0 is HTTP and auth in CI only. The first cluster deploy happens in S1. |
| N12 | 138, 367; altitude | The "one org per process" sentence reads backwards. Item 3 should say Modify, not Create. Step-level git commands, LRU size, JWT offsets, CIDRs, and Rust signatures are too detailed for this altitude. | Fix the sentence and the item. Move the details to the slice stories. |

## Not flagged

Both lenses accept these as real trade-offs:

* The non-atomic cross-repo snapshot, because responses disclose it.
* Files as the source of truth.
* Bot-only resolve markers keyed on `updated_at`.
