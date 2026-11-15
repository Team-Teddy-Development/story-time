# Stage 9 Implementation Readiness report: hosted-story-mcp.md (round 1)

* **Target:** `docs/designs/hosted-story-mcp.md`, DetailedRange lines 329 to 609.
* **Reviewers:** All three completed.
  * `egm-implementation-readiness`, against the implementation-readiness-standard.
  * `egm-codebase-details`, against the same standard. It read 14 source files, 3 test files, CI, and the coffee-apps manifests.
  * `egm-style`.
* **Verdict:** FAIL. The plan is not ready for implementation. After deduplication there are **8 Blockers, 24 Majors, and 16 Nits**.
* **Fact added since the draft:** Both `Varsity-Engine/varsity-core` and `Team-Teddy-Development/varsity-engine` are **private**, as `gh api` reported.
* **Overlap:** Findings marked ↺ repeat an earlier stage.

## 🔴 Blockers

| ID | Lines | Finding | Evidence | Fix |
| --- | --- | --- | --- | --- |
| R-B1 | 337-341, 576, 594, 603 | **Private repositories cannot be read in S0 and S1.** Credentials arrive only in S4, and the public-clone fallback does not exist. | The repositories are private. | Move the read-only App path into S0: App creation, `github-app-id` and `github-app-key` in Vault, the JWT and installation token code, and the git credential channel. Split item 29 by slice. Delete the fallback in line 54. ↺ Z5 |
| R-B2 | 180, 305, 414, 434 | **The credential helper has no channel to the token.** | `mcp::run` has no IPC. `roots()` rejects unknown arguments (`mcp.rs:82-99`). | Name one channel. Either a Unix socket under `/data` with a stated request and response format, or `http.extraHeader` through `GIT_CONFIG_*` with the NFR relaxed. Name the exact `git -c` invocation and the `HOME` and `GIT_CONFIG_NOSYSTEM` settings that work under `readOnlyRootFilesystem`. ↺ Z2, C-M11 |
| R-B3 | 487 | **The image registry is left undecided, and GitHub-hosted CI cannot push to the LAN registry.** | `192.168.1.51:30500` (`coffee-cluster-mcp/30-deployment.yaml:24`, `taskgraph/20-statefulset.yaml:82`). `test.yml` runs on `ubuntu-latest`. | Pin `192.168.1.51:30500/story-time:<sha>-amd64`. Name who builds and pushes the image (a LAN runner, or a manual build on Spark) and how `cc_deploy` receives the tag. CI only builds the image. |
| R-B4 | 574-575 | **`--graph` conflicts with `NEO4J_AUTH=none`, and the arguments end with "…".** | `graph.rs:55-90` requires `--neo4j-user` and a non-empty password variable. | Change `graph.rs`: user and password become optional together, and no basic auth is sent when both are absent. Or give dummy credentials. Write the full `args:` list. |
| R-B5 | 145, 464-467 | **The hosted move to `ready` rejects cross-repo dependencies.** `set::apply` builds its own local-only lint `Context`. | `set.rs:130-145`; `lint.rs:386-389`. | Add `org: Option<&OrgIndex>` to `set::apply`. It has two call sites, `set.rs:50` and `mcp.rs:1040`. |
| R-B6 | 457-458 | **The plan describes `spike_unblocks` wrongly.** The rule does not resolve IDs. It scans local `depends_on` lists for the spike's own ID. | `lint.rs:419-442`. | `OrgIndex::dependents_of(id)`. The rule passes when the local scan or the org scan finds a dependent. Add a test: a VE story depends on a VC spike. |
| R-B7 | 528 | **The plan names a Neo4j "test double" that does not exist.** | `tests/mcp_graph.rs:12-76` runs a real `neo4j:5.26.30-community` container through Docker. | Use the Docker harness. Produce the unavailable case with `Neo4j::stop()` (`mcp_graph.rs:123`). |
| R-B8 | 582 | **The NetworkPolicy uses a hostname, and the Gitea NodePort is wrong.** | NetworkPolicy accepts only `ipBlock`. The repository notes this at `drip/60-network-policies.yaml:94,116`. Gitea is reachable in-cluster at `gitea.gitea.svc:3000`. | Use pinned GitHub `/meta` CIDRs on port 443, plus kube-dns. Or use an ingress-only policy, as Stage 8 proposed. Use the in-cluster Gitea service, and drop `:30022`. ↺ Z4, C-M8 |

## 🟡 Majors

### Protocol and transport

| ID | Lines | Finding | Fix |
| --- | --- | --- | --- |
| R-M1 | 385; `mcp.rs:205, 227, 233` | `handle` already exists, and it takes `&mut self`. The problem is the `legacy` state, not a missing method. ↺ C-B2, I-B1 | Change `handle` and `dispatch` to `&self` with a `legacy: bool` for each request. A notification over HTTP returns 202 with an empty body. |
| R-M2 | 396-399 | The HTTP contract leaves out the `Mcp-Method` and `Mcp-Name` checks (`-32020`, ST-006:85). It does not cover GET or DELETE, Accept and Content-Type, or the HTTP status of a JSON-RPC error. The 503 response from `/readyz` appears only in a test. | Add a table: method, headers, status, body. |
| R-M3 | 386-387; `mcp.rs:294-306` | Write tools are hidden unless `--write` is set. A read-only token then gets `-32602`, not `scope_refused`. | Hosted mode turns on every tool. State whether `tools/list` filters by scope. |
| R-M4 | 182, 387, 404 | Scope semantics are incomplete. `permits(scope, story_id)` has no ID for `story_create`, `story_list`, or `story_graph_sync`. The plan does not say whether `prefixes` limits reads, or which scope `story_repos` and `story_graph` need. | Add a table of tools against scopes and prefixes. |
| R-M5 | 182, 399; `mcp.rs:340-384` | The audit lines are ambiguous. An audit line for each tool call already exists. | Add `client_id` and `sha` to the existing line, in hosted mode only. |

### Writes

| ID | Lines | Finding | Fix |
| --- | --- | --- | --- |
| R-M6 | 140, 146, 415 | `clone --mirror` plus a push with a refspec fails, and a mirror push deletes remote refs. GitHub mirrors also fetch `refs/pull/*`. | Use `clone --bare` with `+refs/heads/*:refs/heads/*`. Push with `HEAD:refs/heads/story-time/<ID>`. Use `--force-with-lease=<ref>:<sha-or-empty>`. |
| R-M7 | 142, 145, 189 | The hosted write schema is undefined. The existing argument is `expected_sha256` (`mcp.rs:1299-1302`). `write_schema` has `additionalProperties:false` and `publish.pushed: const false`. The `already_applied` case is not covered. ↺ C-M2 | Keep `expected_sha256`. Define a separate hosted `write_schema` and the hosted `publish.next`. |
| R-M8 | 150, 472; `new_story.rs:94`; `next_id.rs:64-103` | `next_id` already reserves IDs across every branch, and `create` calls it internally. `new_story.rs` is missing from the plan. Open PR heads need an endpoint. | Reuse `next_id` and add `refs/pull` to it. Otherwise list `new_story.rs` with the call-site change. Define the PR body for a story with no issue. |

### Lint and schema

| ID | Lines | Finding | Fix |
| --- | --- | --- | --- |
| R-M9 | 153, 457-462 | Line 153 says the CLI uses a local-only index, but item 14 says `org: None`. `remote.stale` needs settings that `Context` does not carry. The plan does not say whether the new rules are skipped when `org` is `None`. The warning for a dependency that is not done (`lint.rs:390-398`) has no stated foreign behavior. `RULES: [Rule; 16]` must grow. `relates_to` targets are never checked. No rule name exists for the ambiguous case. ↺ S6 M11, C-M6 | Settle each point in item 14. |
| R-M10 | 457, 464 | `lint::Context` is also built at `saga_tests.rs:66`, `lint.rs:531`, and `mcp.rs:860`. `OrgIndex` inside private `mcp` cannot be named from `lint`. | Add `saga_tests.rs`. Place `OrgIndex` at `src/org_index.rs` as `pub(crate)`. |
| R-M11 | 467 | Files are written through `lenient_story::serialize` (`list.rs:206`), which has its own `KEY_ORDER` (`lenient_story.rs:420-439`). The Node parity files `story.mjs:11` and `update-story.mjs:6` are missing. `summary_schema` has `additionalProperties:false`. | Add `lenient_story.rs` and the two `.mjs` files. Decide whether `summary` includes `relates_to`. |
| R-M12 | 188, 390, 462 | Adding `ref`, `relates_to`, or hosted fields to shared schemas changes local `tools/list`, which breaks the byte-identical claim. | Hosted fields go only under `Backend::Hosted`. |

### Graph and tracker

| ID | Lines | Finding | Fix |
| --- | --- | --- | --- |
| R-M13 | 164, 451; `graph.rs:124-129`; `mcp.rs:324` | The existing error is `graph_unavailable`, which is retryable and covered by tests. ↺ | Reuse `graph_unavailable`. |
| R-M14 | 447-449; `graph.rs:184-211, 271-274, 323, 479` | The design duplicates the `StoryRepository` node. The repository identity is undefined: tracker string or registry key. The `edge.type` enum is closed. The traversal is undirected. `depth` already exists. The source of `MENTIONS` is unspecified. The first sync after a start is not covered. ↺ C-N5, C-N7 | Extend `StoryRepository` with `key` and `sha`. Name one identity. Give the directed Cypher for `dependents` and `dependencies`. Widen the enums. Sync once at start. |
| R-M15 | 441; `lib.rs:122`; `gateway.rs:649-713` | A `trait Tracker` already exists. The Gitea request builder already exists. ↺ Z9 | Rename the new trait to `Forge`. Reuse `Gitea::request` through `pub(crate)`. Add `gateway.rs` to the file list. |
| R-M16 | 166-171, 389, 441 | No schemas exist for `story_repos`, `story_comments`, or `story_resolve_comment`. Error codes, pagination, and parsing of `remote` and `remote_url` are missing. | Add them in the `Kind` and `Field` style. |
| R-M17 | 431-436; `mcp.rs:60-63, 1711-1722` | The redactor's values are fixed at start, and `&self` makes them immutable. The regex already covers `ghs_` and `Bearer`. | Use `RwLock<Vec<String>>`, or name the tokens that rely on the regex alone. |

### Tests, build, and deploy

| ID | Lines | Finding | Fix |
| --- | --- | --- | --- |
| R-M18 | 515-552 | `tests/*.rs` drive the binary as a subprocess. `lint::Context` and `mcp` internals are `pub(crate)` or private, so `lint_org.rs` and `forge_auth.rs` cannot call them. A "fake Tracker" cannot be injected into a subprocess. | Use in-crate `#[cfg(test)]` modules for unit tests, as `saga_tests.rs` does. Subprocess tests use HTTP forge stubs, chosen with `--github-api-url` or `--gitea-url`. The server prints `listening on <addr>`. Define the state before the load completes. |
| R-M19 | 474-477; `help.rs:5` | `help.rs` only includes `skills/story-time/scripts/usage.txt`. `mcp_registration_docs.rs` also checks `skills/story-time/SKILL.md`. | Target `usage.txt` and `SKILL.md`. |
| R-M20 | 482 | The Docker build context must be the repository root, because `help.rs` includes a file outside the crate. | Build context is the root, with `-f native/story-push/Dockerfile`. Use `cargo build --release --locked`. Name the base image, for example `debian:bookworm-slim`. |
| R-M21 | 372-379 | `Cargo.lock` is missing. CI uses `--locked` (`test.yml:48-49`). | Add `Cargo.lock`. Pin `tiny_http` and `base64`. |
| R-M22 | 600 | The `clippy` and `fmt` commands have no manifest path, and the root has no Cargo.toml. | Use the CI forms with `--manifest-path native/story-push/Cargo.toml --locked`. |
| R-M23 | 566-582 | The deployment does not split by slice. Problems in it: `readOnlyRootFilesystem` applies to the whole pod and breaks Neo4j. The Neo4j image, env, and securityContext are not pinned (taskgraph lines 131-149). git has no writable `HOME` or `/tmp`. The ServiceAccount, `automountServiceAccountToken`, requests, CPU limits, and ingress annotations are missing. The ExternalSecret store, keys, and mount paths are not pinned (`ClusterSecretStore vault-taskgraph`, `global/infra.story-time.*`). An ingress-only policy can block kubelet `httpGet` probes. | List the manifest contents for each slice. Copy the Neo4j settings from taskgraph. Use a read-only root for `story` only. Pin the secret wiring. Add an `ipBlock` for the node CIDR, or use exec probes. |
| R-M24 | 342, 572, 602-607 | Gitea and the sandbox repositories are never registered, and a second org needs a second process. S1 has no cluster check. Several checks overstate what they prove: `ls-remote` shows only git egress; S0 returns a file `sha256`, not a commit SHA; passing contract tests show only the output they cover. ↺ Z8, I-M1 | Defer Gitea, or specify the second Deployment. Name the sandbox repositories. Add the S1 check. Reword the checks so they claim only what they show. |

## 🟢 Nits

* **Registry example:** It lacks `default_branch`, and the field name `fetch_ttl` does not match the key `fetch_ttl_seconds`. Reuse `mcp.rs settings()` to read prefixes.
* **Flags:** `--registry` is missing from item 6, and `roots()` must accept it.
* **Backend:** Hosted reads build `Root { key, path: tree_at(...) }`, so that `root()`, `open()`, `audit()`, and `identity()` can be reused.
* **Client tokens:** Parse the clients file with `toml_subset`. Hash with `adoption::sha`. Compare with `ring::constant_time::verify_slices_are_equal`. Confirm that `toml_subset` supports arrays of tables.
* **App tokens:** The installation lookup (`GET /orgs/{org}/installation`) happens before the token exchange. Add the PEM to DER conversion step.
* **Worktree cache:** Define what the 64-entry LRU holds, and how evicted worktrees are removed.
* **Item labels:** Item 3 should be Modify, and "(Step 6)" is not a slice.
* **CI:** `test.yml` needs no change to pick up new test files.
* **Story IDs:** State who allocates the six story IDs, and when.
* **Headings:** Use `#####` for the numbered items under each group heading.
* **Org name:** Write `team-teddy-development` with one consistent case throughout.
* **Item 12:** Split it into bullets.
* **Rule IDs:** Use `independent.deps` and `spike.unblocks` consistently.
* **Wording:** Remove "exactly like".
* **Test oracles:** Make vague test checks concrete, for example "apart from the envelope", "fire as specified", and "no full fetch".
* **Rule 4:** Link it.
* **Line 609:** Drop "Drafted with EGM", which has no technical content.
