# Stage 7 Critic Review report: hosted-story-mcp.md (round 1)

* **Target:** `docs/designs/hosted-story-mcp.md`, FeatureRange lines 1 to 328
* **Reviewers:** `egm-design` (feature-spec-standard), `egm-structural` (feature-spec-standard), and `egm-style` (persona rules). All three completed.
* **Verdict:** FAIL. After deduplication there are **4 Blockers, 14 Majors, and 14 Nits**.
* **Clean round:** No.
* **Overlap:** Findings marked ↺ repeat a Stage 5 or Stage 6 finding, which raises their priority.

## 🔴 Blockers

| ID | Lines | Finding | Fix | Reviewers |
| --- | --- | --- | --- | --- |
| C-B1 | 213-297, 56 | **Rejections do not meet the Alternatives Rule.** Nine Reasons are only a Jev score (A, C, D, E, I, K, L, M, O). Six are only "author/owner chose" (B, F, G, H, J, N). Nothing is linked, and E is circular ("it is a Non-Goal"). B is the weakest: the text admits that Jev preferred no database at 0.73. "Jev" and the scale are never defined. | Lead each Reason with the concrete trade-off, most of which the Cons already state, and keep the score only as support. Define Jev and the 0 to 1 scale once. Link the owner decision records (B, F, H, line 56), or remove the attribution. For B, state which query in-memory traversal cannot serve, or record Neo4j openly as an owner constraint. ↺ Stage 6 M2 | design, structural, style |
| C-B2 | 94, 115, 121, 291, 310; `mcp.rs:227, 233, 265` | **`&mut self` dispatch and a process-wide `legacy` flag.** Sharing one `Server` across 8 workers needs a mutex, so requests run one at a time and the p95 below 200 ms is impossible. One client's `initialize` changes the protocol era for every client. | Required refactor: dispatch becomes `&self`, and each request carries its own era (from the `MCP-Protocol-Version` header or `_meta`). Give shared caches fine-grained locks. Add this to the ADR 0007 scope. ↺ Stage 5 Z1 | design |
| C-B3 | absent | **No Risks & Mitigations section.** Unstated risks: single-replica correctness, pool saturation, cold start on `emptyDir`, the App's blast radius, untrusted comment text, and unproven egress. | Add `## Risks & Mitigations` (Risk, Impact, Mitigation, Residual) between Alternatives and NFRs. Move the scattered mitigations into it. | structural, design |
| C-B4 | 170, G4 | **Prompt injection through comments.** `story_comments` returns comment bodies verbatim. On public repositories anyone can comment, and an agent with `write` and `comment` scopes then acts on that text. | Name it in Risks. Mark comment bodies as untrusted data in the result. Filter by author association or org membership, or expose comments only to read-only clients by default. | design |

C-B4 is raised from Major to Blocker by the author. It is a security path in a multi-tenant public service.

## 🟡 Majors

| ID | Lines | Finding | Fix | Reviewers |
| --- | --- | --- | --- | --- |
| C-M1 | 56 | "(Option A)" twice, which points at Alternative A, the wrong alternative. ↺ S6 M1 | Name each choice and the alternatives it rejects. | all three |
| C-M2 | 80-81, 142 | "blob SHA" and "SHA-256 of the file" are used for the same thing. The existing contract field is `expected_sha256`. ↺ S6 M6 | Use one term and one argument name: reuse `expected_sha256`. | all three |
| C-M3 | 140-150, CUJ 4 | The write base and the branch lifecycle are undefined: which file `base` compares against, what happens to a merged or closed PR branch, and what happens when the default branch moves under an open PR. ↺ S5 Z3 | `base` compares against the branch-head file while a PR is open. `story_show` reports the open PR ref. A branch whose PR is merged or closed is recreated from the default branch. A drifted PR is reported as conflicted. | design |
| C-M4 | 140, 143-148 | Bare mirror, worktrees, and fetch collide. A fetch is refused while a worktree has the branch checked out. Ref locks race. The file lock in a fresh worktree excludes nothing. | Add a mirror lock for each repository and single-flight fetches. Use detached-HEAD worktrees and push `HEAD:refs/heads/story-time/<ID>`. Drop the file-lock claim. | design |
| C-M5 | 115-119 | The Streamable HTTP checks leave out `Mcp-Method` and `Mcp-Name` against the body (`-32020`, required by ST-006). GET and DELETE on `/mcp` are undefined. There is no error status for an unsupported version. | List every header-to-body check and its code. `GET /mcp` returns 405. | design, (S5 Z1) |
| C-M6 | 152-154 | After the backfill, local lint with a local-only index errors on `VC-109` in varsity-web, so local `status=ready` breaks. | An ID whose prefix the local repository does not own gives a warning locally. Only the org index raises "does not exist". Add a CUJ for this. ↺ S6 M11 | design |
| C-M7 | 147, 150, 170 | A new story has no tracker issue. The Rule 5 PR body `(#<issue>)` cannot be filled, no hosted step publishes the issue, and `story_comments` returns `no_tracker_issue`. | Define hosted publishing (on merge, a separate job, or a documented manual step) and the PR body when no issue exists. | design |
| C-M8 | 199, 308, 53 | An egress NetworkPolicy cannot name `api.github.com`. Neither sibling service pins egress. Gitea is reachable in-cluster. ↺ S5 Z4 | Name the CNI mechanism, or make egress best effort. Use `gitea.gitea.svc:3000`. Admit the ingress controller. | design |
| C-M9 | 100, 115, 171, 196-200 | Probes share the saturated pool, which can cause liveness kills during a write. A RollingUpdate overlap runs 2 pods. Resolve is check-then-post, so concurrent calls post twice. ↺ S5 Z12/Z13 | Serve probes on a separate thread. Use `strategy: Recreate`. Add a mutex for each comment and re-check after posting. Add a request timeout and a queue limit. | design |
| C-M10 | 116, 181, 304, 323 | The client-token lifecycle is unspecified: reload after an ExternalSecret refresh, revocation time, entropy, and rate limiting on a public hostname. | Hot-reload the token file. State a revocation SLA, a minimum entropy, and rate limits at nginx or Cloudflare. | design |
| C-M11 | 180, 305 | The credential helper has no channel to the in-memory token. ↺ S5 Z2 | Specify a Unix socket with peer-credential checks, or a GIT_CONFIG environment header, and relax the claim. Add a test. | design |
| C-M12 | 180, 306 | The design overstates the redactor. Today it matches fixed token prefixes and named environment variables. It does not cover the Gitea token from a file or client bearer tokens. | Write "the redactor is extended to …". | style |
| C-M13 | 166 | The intro says "two read and one write tool", but the table lists two tools. ↺ S6 M9 | Write "one read tool and one comment-scoped tool". | structural, style |
| C-M14 | 313, 273, 196 | Claims about existing infrastructure are wrong. Taskgraph's core limit is 768 Mi, not 512 Mi. coffee-cluster-mcp uses one shared `MCP_SECRET`. The Ingress pattern comes from coffee-cluster-mcp, not taskgraph. ↺ S5 Z15 | Correct each claim and give a reason for 512 Mi. | structural, style, design |

## 🟢 Nits

| ID | Lines | Finding |
| --- | --- | --- |
| C-N1 | 138, 325 | "One org per process" reads backwards (↺ S6 M4). |
| C-N2 | 44, 126-135 | "org" means two things. Define "registry org" (↺ S6 M3). |
| C-N3 | 138 | The claim that varsity-core owns ST needs evidence, and a stray file can make a prefix ambiguous (↺ S5 Z10). |
| C-N4 | 175 | Match the bot by numeric user ID, not by the `storytime[bot]` login. |
| C-N5 | 164, 190 | `depth` (1 to 3) and `limit` (at most 200) already exist. Only `direction` is new. |
| C-N6 | 164; `graph.rs:82-107` | `--graph` requires a Neo4j user and password today, which conflicts with `NEO4J_AUTH=none`. |
| C-N7 | 158-159 | Reuse the existing `StoryRepository` label and its tracker-identity key, or state the change. |
| C-N8 | 197 | Apply `readOnlyRootFilesystem` only to the `story` container. Give the `emptyDir` a `sizeLimit`. |
| C-N9 | 181 | State whether `prefixes` also limits reads, including the issue comments of private repositories. |
| C-N10 | 140, 311 | Fetches across repositories need parallel, single-flight execution and a worst-case latency. |
| C-N11 | 319 | Promote Future Improvements to H2. |
| C-N12 | 28 | Move "The proposed solution" out of The Problem. |
| C-N13 | 115-182 | Make components H4 headings. Add blank lines before lists. |
| C-N14 | 26, 217, 291, 311, 219, 237, 54, 147, 212 | Unsupported wording: "goes unanswered", "well under one second", and "about 1 s today" are not measured. "A few clients", the filler sentence, and the circular E reason need rewording. The decider for line 54 is not named. Rule 4 and Rule 5 have no links. |
