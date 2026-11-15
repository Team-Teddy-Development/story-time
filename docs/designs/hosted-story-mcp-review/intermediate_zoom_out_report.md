# Stage 8 Intermediate Zoom Out report: hosted-story-mcp.md

* **Target:** `docs/designs/hosted-story-mcp.md`, full file (lines 1 to 609). The file is unchanged since Stage 5.
* **Reviewer:** `egm-zoom-out`, run with fresh context. It also read the reports from Stages 5, 6, and 7.
* **Focus:** Which fixes proposed in Stages 6 and 7 the design needs, and which of them would make v1 too large.
* **Verdict:** Not ready. There are **3 Blockers, 7 Majors, and 6 Nits**. Almost all of them are fixed by making the design smaller or by adding one short rule.
* **Author check:** The redactor regex at `mcp.rs:1714` already matches `ghs_` and `Bearer <token>`. Critic finding C-M12 is therefore mostly wrong, and the only gap it names is the Gitea token.

## 🔴 Blockers

| ID | Lines | Finding | Smallest fix | Rejected expansion |
| --- | --- | --- | --- | --- |
| I-B1 | 94, 115, 121; `mcp.rs:227, 233, 265` | Dispatch shares state across requests, through `&mut self` and the process-wide `legacy` flag. This is confirmed in the code. | `dispatch(&self)`. Each HTTP request takes its protocol era from its own header or `_meta`, and stdio keeps its own flag. Add the `Mcp-Method`/`Mcp-Name` check (`-32020`). `GET /mcp` returns 405. | A new design topic for fine-grained cache locks. One `Mutex<HashMap>` is enough. |
| I-B2 | 140-150, CUJ 4 | Reads and writes compare against different bases, so G5 fails after the first write. | Add a branch lifecycle paragraph. While a PR is open, `story_show` returns `proposed: {ref, content_sha256, pr_url}`, and `base` is compared against that ref. After the PR merges or closes, the next write starts from the default branch. A `conflict` result names the ref and the hash it compared. | Drift detection and rebase. The forge already shows when a PR conflicts. |
| I-B3 | absent; 170 | There is no Risks section, and comments allow prompt injection. | Add a Risks table of about 6 rows that points to existing mitigations. `story_comments` marks `body` as untrusted and returns `author_association`. | Filtering comments by org membership. Showing comments only to read-only clients, which breaks CUJ 3. |

## 🟡 Majors

| ID | Lines | Finding | Smallest fix | Rejected expansion |
| --- | --- | --- | --- | --- |
| I-M1 | 15, 110, 166-180, 441, 582, 591 | **Defer Gitea.** It causes about 5 later findings and most of the confusion about the word "org". | The registry refuses `forge = "gitea"`. Move the Gitea bot, adapter, egress, and Vault item to Future Improvements. | none |
| I-M2 | 140-148, item 9 | **Git concurrency.** | One mutex per repository for every git operation. Detached worktrees and `push HEAD:refs/heads/story-time/<ID>`. Remove the `(repo, story)` mutex and the claim about the file lock. | Single-flight fetches, a separate mirror lock, and ref-lock handling. |
| I-M3 | 180, 305, 434 | **The credential channel.** | Pass the token in the child `git` environment through `GIT_CONFIG_COUNT` and `http.extraHeader`. Change the NFR to "never in argv or on disk". Remove `story mcp credential`. If the repositories are private, move a read-only App token to S1. | A Unix socket with peer-credential checks. |
| I-M4 | 152-154, 459-462 | **Local lint after the backfill.** | A foreign prefix gives a warning when there is no org index. Build the `OrgIndex` from every local `--root` (Z7), so S2 no longer depends on S1. Limit the byte-identical claim to repositories with no foreign IDs. | none |
| I-M5 | 147, 150, 170 | **Hosted tracker publishing.** | Add a Non-Goal: stories created on the server stay unpublished until the existing `story push` runs. The PR body is `Stories: <ID>` with no `(#issue)`. Check that Rule 5 allows this, or record it as an Open Question. | A publishing job. |
| I-M6 | 158-164, 215-219, 340, 443-451 | **Neo4j.** No query exists that an in-memory BFS could not serve. **Owner decision:** (a) record Neo4j as an owner constraint with a link, or (b) answer `story_graph` from `OrgIndex` and keep Neo4j only as a projection. | Either option drops the `MENTIONS` edges and `remote.stale`. | none |
| I-M7 | 115, 197-200, 573, 577 | **Operations.** | Accept: `Recreate`; a timeout on each request; a long liveness timeout or no liveness probe; readiness that does not wait for every repository (an unreachable one returns `repository_unavailable`); an ingress-only NetworkPolicy without the egress CIDRs. | A split worker pool, a background refresher, a PVC or 2 replicas, a probe thread, and a per-comment mutex. A double resolve posts two valid markers, which is harmless. |

## 🟢 Nits

| ID | Finding | Fix |
| --- | --- | --- |
| I-N1 | Token lifecycle | Revoke a token by removing its hash in Vault and restarting the pod, which is safe because the pod holds only caches. Tokens are 32 random bytes. Rate limiting goes to Future Improvements. Reject hot reload and a revocation SLA. |
| I-N2 | Redactor | Write "already covers `ghs_` and `Bearer`". Nothing needs to be added once Gitea is deferred. |
| I-N3 | Auth parity | The sibling services each use one shared secret. Say that per-client tokens are a new pattern. |
| I-N4 | Bot identity | Match the bot by its numeric user ID. |
| I-N5 | `relates_to` | Defer it, or use it in CUJ 2. |
| I-N6 | Alternatives (C-B1) | Move the existing Cons into each Reason and define Jev once. Add no new analysis. |

## Minimum v1

| Slice | Contents |
| --- | --- |
| S0 (CI only) | `--http` in the same binary. `&self` dispatch with the protocol era taken from each request, plus the header checks. Hashed client tokens with scopes. |
| S1 (first deploy, GitHub only) | Registry with declared `prefixes`. Mirrors and the TTL fetch. One git mutex per repository. A read-only App token through `GIT_CONFIG` if the repositories are private. An ingress-only NetworkPolicy. `Recreate`. Readiness that does not wait for every repository. |
| S2 (no server needed) | `OrgIndex` built from the registry or from local `--root` options. Cross-repo lint, with a local warning for a foreign prefix. Backfill PRs. |
| S3 | `story_graph` with `direction`. Neo4j as the owner decides in I-M6. |
| S4 | Branch-and-PR writes that follow the lifecycle rule. New stories stay unpublished until `story push`. |
| S5 | GitHub comments. Markers matched by bot ID. Bodies marked untrusted, with `author_association`. |

**Deferred:**

* Gitea.
* `MENTIONS` edges and `remote.stale`.
* Hosted publishing.
* Filtering comments by org membership.
* A split pool and a background refresher.
* A PVC or several replicas.
* A socket channel for credentials.
* Token hot reload and rate limiting.
* Egress pinning.
* Atomic edits across repositories.

## Open facts that change the plan

* **Are the Varsity repositories private?**
  * If they are private, a read-only App token and the credential channel move to S1.
  * If they are public, the residual risk from untrusted comments is higher.
