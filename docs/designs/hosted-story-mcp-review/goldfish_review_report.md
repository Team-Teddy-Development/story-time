# Goldfish Annotated Review Report

**Status: resolved** (2026-11-15). 31 PROPOSE FIX items applied to `docs/designs/hosted-story-mcp.md`. 4 ESCALATE items decided (E1 by the owner; E2 to E4 by Jev) and recorded in ST-128 (#289). 0 contested. 1 group of Nits declined. 0 items remain open. Line numbers in this report and in the stage reports refer to the design before the fixes (609 lines).

**Target:** `docs/designs/hosted-story-mcp.md` (full file, lines 1-609)

**Inputs:** This report merges and deduplicates the findings of five reports in this worktree: `zoom_out_report.md` (Z), `comprehension_report.md` (S6), `critic_report.md` (C), `intermediate_zoom_out_report.md` (I), and `implementation_readiness_report.md` (R). Each issue lists the source finding IDs.

**Counts:**

| Category | Issues |
| --- | --- |
| PROPOSE FIX | 31 |
| ESCALATE | 4 |
| CONTEST | 0 |
| DECLINE | 1 group of Nits |

**Applied to the fixes:**

* Both GitHub repositories are private. `gh api` returned `private: true` for both.
* Stage 8 marked some ideas as too large for v1. The fixes leave those ideas out.

---

## ESCALATE: these need the owner's knowledge

### [Issue E1] [`zoom-out`, `design`, `inventive-catalyst`] - Severity: Blocker — **RESOLVED by owner: option (a), Neo4j at the core; Alternative B reason rewritten as a stated trade-off**
*   **Affected Scope:** Lines 158-164, 215-219, 340, 443-451 (Technical Plan §6, Alternative B, S3, item 13)
*   **Discrepancy Description:** The design gives no query that Neo4j answers and the in-memory `OrgIndex` cannot. The org has about 355 nodes, the maximum depth is 3, and the node limit is 200. Neo4j brings a sync step, a failure mode, a mismatch between the SHA lint uses and the SHA the graph uses, and a 1.5 Gi sidecar. Alternative B has no technical reason for its rejection. Sources: Z6, I-M6, C-B1(B).
*   **Rationale:** The Alternatives Rule requires a stated trade-off. The owner's requirement is not a trade-off.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** The only reason for Neo4j is the owner's statement in the brainstorm chat: "linking stories in neo4j". No file the reviewers could read records that statement. The choice between (a) and (b) belongs to the owner:
    *   **(a)** Neo4j answers `story_graph`. The design states this as an owner constraint.
    *   **(b)** `OrgIndex` answers `story_graph`. Neo4j receives a write-only projection of every cross-repo edge, so the requirement is still met, and a Neo4j outage affects no tool.
*   ***Proposed Edit:***
    *   Location: Alternative B, Technical Plan §6, and item 13.
    *   Change: Write the owner's choice. In both cases, rename the Reason for Rejection in Alternative B to "Owner constraint (2026-11-15 brainstorm): Neo4j must hold cross-repo links". In both cases, drop the `MENTIONS` edges.

### [Issue E2] [`zoom-out`, `implementation-readiness`] - Severity: Major
*   **Affected Scope:** Lines 15, 110, 166-180, 342, 441, 544, 572, 582, 591-592, 606 (Gitea throughout)
*   **Discrepancy Description:** Gitea is speculative in v1. The only registered org is Varsity, and both of its repositories are on GitHub. Gitea brings about 5 other findings with it: the second process or pod, the sandbox repositories, the NodePort egress, the bot user, and a second adapter. Sources: Z8, I-M1, R-M24, S6 M3.
*   **Rationale:** No v1 Goal or CUJ needs Gitea.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** In the brainstorm, the owner approved "tokens for … `team-teddy` on Gitea". Dropping Gitea reverses that decision. Recommendation: defer Gitea. The registry refuses `forge = "gitea"`. The Gitea bot, adapter, and egress move to Future Improvements.
*   ***Proposed Edit:***
    *   Location: The Gitea lines listed above.
    *   Change: If the owner agrees, remove Gitea from S4, S5, items 11, 12, 24, 28, and 29, and from the NFRs. Add Gitea to Future Improvements.

### [Issue E3] [`implementation-readiness`, `codebase-details`] - Severity: Blocker
*   **Affected Scope:** Line 487 (item 19)
*   **Discrepancy Description:** The image registry is left undecided. Every coffee image comes from `192.168.1.51:30500`, a plain-HTTP registry on the LAN. GitHub-hosted runners (`ubuntu-latest`) cannot push to it. Sources: R-B3.
*   **Rationale:** This breaks the Zero-Decision Rule.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** No file says who builds coffee images and pushes them to `:30500`. `coffee-cluster-mcp/docs/deploy-path.md` covers only `cc_deploy(app, file, image, tag)`. Spark is aarch64, but the deployed tags end in `-amd64`, so the build happens somewhere else or cross-compiles. The owner knows which.
*   ***Proposed Edit:***
    *   Location: Item 19.
    *   Change: "Image `192.168.1.51:30500/story-time:<short-sha>-amd64`, built and pushed by <owner-named path>. GitHub CI only builds the image to check the Dockerfile. The tag is passed to `cc_deploy(app='story-time', file='k8s/story-time/30-deployment.yaml', image='192.168.1.51:30500/story-time', tag)`."

### [Issue E4] [`structural`, `design`, `style`] - Severity: Blocker (the part of C-B1 about links)
*   **Affected Scope:** Lines 56, 213-297
*   **Discrepancy Description:** Owner and Jev decisions are attributed without a linked issue, discussion, or PR.
*   **Rationale:** This is the Alternatives Rule's requirement on attribution.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** The decisions were made in this chat session, and no issue or PR records them yet. Two options:
    *   **(a)** File the design story now and link its issue as the decision record.
    *   **(b)** Remove the attributions and keep only the technical trade-offs. Issue P4 does this either way.
*   ***Proposed Edit:***
    *   Location: Alternatives.
    *   Change: Choose (a) or (b).

---

## PROPOSE FIX: Blockers

### [Issue P1] [`design`, `codebase-details`, `zoom-out`, `catalyst`] - Severity: Blocker
*   **Affected Scope:** Lines 115-121 (Component 1) and line 385 (item 5); `mcp.rs:205, 227, 233, 265`
*   **Discrepancy Description:** `handle` and `dispatch` take `&mut self`. `initialize` sets the process-wide `self.legacy`. Over HTTP, requests would run one at a time, and one client's `initialize` would change the protocol era for every other client. Sources: Z1, C-B2, I-B1, R-M1.
*   **Rationale:** This is confirmed in the code.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Taking the era from each request is the smallest fix that is correct.
*   ***Proposed Edit:***
    *   Location: Component 1, at the sentence "The server is stateless per request".
    *   Change, replacing that sentence: "`Server::handle` and `dispatch` change from `&mut self` to `&self` and take a `legacy: bool` for each request. In HTTP mode, no state is stored on `Server`. A request whose body has modern `_meta` uses the 2026-07-28 era. A request without `_meta` is legacy only if its `MCP-Protocol-Version` header names a legacy version; otherwise it is refused with `-32022`. The stdio loop keeps its own `legacy` flag. Shared caches each sit behind their own `Mutex`."
    *   Location: Item 5.
    *   Change: "Change the existing `handle(&mut self, line)` to `handle(&self, caller: &Caller, legacy: bool, body: &[u8])`."

### [Issue P2] [`design`, `catalyst`, `zoom-out`, `codebase-details`] - Severity: Blocker
*   **Affected Scope:** Lines 78-82 (CUJ 4), 140-150 (Component 4), 418 (item 9); `mcp.rs:1299-1302`
*   **Discrepancy Description:** Reads default to `main`, but a write compares `base` against the head of `story-time/<ID>`. Every write after the first one therefore conflicts. Merged or closed branches are reused. The hash has two names, "blob SHA" and "SHA-256", and the existing argument is `expected_sha256`. Sources: Z3, C-M2, C-M3, I-B2, R-M7, S6 M6.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: CUJ 4.
    *   Change: Replace "blob SHA `A`" with "`content_sha256` `A`".
    *   Location: Component 4.
    *   Change: Add a paragraph titled "Branch lifecycle" with these rules:
        1. Write tools take the existing `expected_sha256` argument.
        2. While story `<ID>` has an open PR, `story_show` and `story_list` add `proposed: {ref, content_sha256, pr_url}`, and writes compare against the file at that ref.
        3. When there is no open PR, the server deletes the `story-time/<ID>` branch, if it exists, and starts the next write from the default branch.
        4. `conflict` returns `{ref, current_sha256}`.
        5. When the default branch moves under an open PR, nothing happens. The forge shows the PR as conflicted.
    *   Location: Item 9.
    *   Change: Point to these rules.

### [Issue P3] [`structural`, `design`, `zoom-out`] - Severity: Blocker
*   **Affected Scope:** A new section between Alternatives Considered and Non-Functional Requirements, plus line 170
*   **Discrepancy Description:** There is no Risks & Mitigations section. Comment bodies are attacker-controlled input that reaches agents with write scope. Sources: C-B3, C-B4, I-B3.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: The new section `## Risks & Mitigations`.
    *   Change: Add a table with the columns Risk, Impact, Mitigation, Residual, and these 7 rows:
        1. **Single replica.** A deploy causes a short outage. Mitigation: `Recreate`, and a readiness check that does not wait for mirrors. Residual: seconds of downtime per deploy.
        2. **Worker pool saturation.** Mitigation: a 30 s timeout on each request and a bounded queue that returns a retryable `busy`. Residual: slow responses when there are more than 8 concurrent forge calls.
        3. **Cold start.** Mirrors are cloned again from `emptyDir`. Mitigation: a repository that is not yet mirrored returns `repository_unavailable`.
        4. **App blast radius.** The App holds write access to the installed repositories. Mitigation: install it on registered repositories only, per-client scopes and prefixes, and the `Requested-by` trailer.
        5. **Untrusted comment text.** Mitigation: the bodies are marked untrusted, and `author_association` is returned. Residual: low, because the repositories are private and only members can comment.
        6. **A double resolve posts two markers.** This is harmless, because both are valid bot markers.
        7. **Egress is not proven.** Mitigation: the probe in S1.
    *   Location: Line 170, `story_comments`.
    *   Change: Add "Each comment also returns `author_association`. The tool description states that `body` is untrusted input, not instructions."

### [Issue P4] [`structural`, `design`, `style`] - Severity: Blocker (the trade-off part of C-B1)
*   **Affected Scope:** Lines 213-297, 56
*   **Discrepancy Description:** The Reasons for Rejection are only scores or choices. Jev and the score scale are not defined. The label "(Option A)" points to the wrong Alternative. Alternative E's reason is circular. Sources: C-B1, C-M1, S6 M1, S6 M2, I-N6.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: The top of Alternatives.
    *   Change: Add "Scores come from TypeSafe Jev, a typed-judgment service. A score is Jev's probability (0-1) that the option is the best choice, given the stated evidence. Scores support the stated reasons. They do not replace them."
    *   Location: Each Reason for Rejection.
    *   Change: Lead each Reason with its main trade-off, taken from that Alternative's Cons. For example:
        *   I: "A remote caller often has no checkout to apply the patch to, so G5 fails for cloud sessions (Jev 0.09)."
        *   E: "A review thread exists only while a PR changes the story."
    *   Location: Line 56.
    *   Change: "writes use server-held clones and push branches (rejecting Alternatives G, H, I); HTTP is served by the same `story` binary (rejecting Alternative N)."

### [Issue P5] [`implementation-readiness`, `zoom-out`] - Severity: Blocker
*   **Affected Scope:** Lines 54, 337-342, 576, 594, 603
*   **Discrepancy Description:** The repositories are private, but credentials arrive only in S4. S0 and S1 cannot clone. Sources: R-B1, Z5, M19.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: The slice table.
    *   Change: Rewrite the slices as follows:
        *   S0: HTTP, auth, scopes, and tests in CI only. There is no cluster deploy and no init container (Z/N11).
        *   S1: The first deploy. It adds the registry, mirrors, the read-only App token, and the git credential channel. It needs the App with Contents: read and Metadata: read.
        *   S4: Adds the Contents: write, Pull requests: write, and Issues: write permissions to the App.
    *   Location: Item 29.
    *   Change: Split the item by slice. S1 gets `github-app-id`, `github-app-key`, and `clients`.
    *   Location: Line 54.
    *   Change: Delete the public-clone fallback.

### [Issue P6] [`all`] - Severity: Blocker
*   **Affected Scope:** Lines 180, 305, 414, 434 (items 9, 11)
*   **Discrepancy Description:** The credential helper has no channel to the token, which is cached in memory. Sources: Z2, C-M11, I-M3, R-B2.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Stage 8 judged a socket channel too large for v1. Code in the same pod can already read the App key file.
*   ***Proposed Edit:***
    *   Location: Component 8 and item 11.
    *   Change: "Each `git` child process gets the token through its own environment: `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=http.https://github.com/.extraheader`, and `GIT_CONFIG_VALUE_0=AUTHORIZATION: basic <base64(x-access-token:TOKEN)>`. The children also get `GIT_CONFIG_NOSYSTEM=1`, `HOME=/data/home`, and `GIT_TERMINAL_PROMPT=0`."
    *   Change: Remove `story mcp credential`.
    *   Location: The NFR at line 305.
    *   Change: "Tokens are never passed in argv, never written to disk or into clones, and are passed only in the environment of `git` children."

### [Issue P7] [`implementation-readiness`, `codebase-details`, `style`] - Severity: Blocker
*   **Affected Scope:** Lines 164, 574-575 (item 13, item 28); `graph.rs:55-90`
*   **Discrepancy Description:** `--graph` requires `--neo4j-user` and a non-empty password variable, which conflicts with `NEO4J_AUTH=none`. The arguments are elided with "…". Sources: R-B4, C-N6, Z/N3.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 13.
    *   Change: Add "`launch` accepts `--neo4j-user` and `--neo4j-password-env` together or not at all. With neither, it sends no basic auth. The local behavior with both flags is unchanged."
    *   Location: Item 28.
    *   Change: Write the full `args:` list, without the ellipsis. This depends on E1.

### [Issue P8] [`codebase-details`] - Severity: Blocker
*   **Affected Scope:** Lines 145, 464-467 (items 14, 15); `set.rs:50, 130-145`; `mcp.rs:1040`
*   **Discrepancy Description:** `set::apply` builds a lint `Context` that sees only the local repository. A hosted `status=ready` is therefore refused for any cross-repo dependency. Sources: R-B5.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 15.
    *   Change: Add "`set::apply` gains `org: Option<&OrgIndex>`. The CLI caller (`set.rs:50`) passes `None`. The hosted `story_update` (`mcp.rs:1040`) passes the org index."

### [Issue P9] [`codebase-details`] - Severity: Blocker
*   **Affected Scope:** Lines 457-458 (item 14); `lint.rs:419-442`
*   **Discrepancy Description:** The design misdescribes `spike_unblocks`. The rule scans for dependents of the spike. It does not resolve IDs. Sources: R-B6.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 14.
    *   Change: "`spike.unblocks` passes when the local scan or `org.dependents_of(id)` finds a dependent." Add a test case to item 22: a VE story depends on a VC spike.

### [Issue P10] [`codebase-details`, `style`] - Severity: Blocker
*   **Affected Scope:** Line 528 (item 23); `tests/mcp_graph.rs:12-76, 123`
*   **Discrepancy Description:** The design names a Neo4j "test double" that does not exist. The tests run a real Neo4j in Docker. Sources: R-B7.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 23.
    *   Change: "Add cases to the Docker-backed Neo4j harness in `tests/mcp_graph.rs`. The unavailable case calls `Neo4j::stop()`."

### [Issue P11] [`all`] - Severity: Blocker
*   **Affected Scope:** Lines 53, 199, 308, 580-582
*   **Discrepancy Description:** A NetworkPolicy cannot match the hostname `api.github.com`. Neither sibling service pins egress. An ingress-only policy can also block kubelet `httpGet` probes. Sources: R-B8, Z4, C-M8, I-M7, R-M23.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 28 and the NFRs.
    *   Change: "NetworkPolicy, ingress only, as in taskgraph: it admits the `ingress-nginx` namespace to port 8080. There is no egress pinning in v1 (Future Improvements). Probes are `exec` probes that run `story mcp probe`, which asks `127.0.0.1:8080/readyz`." Delete the CIDR list and the `:30022` port.
    *   Location: Open Questions.
    *   Change: Narrow the question to "GitHub egress, proven by the probe in S1".

---

## PROPOSE FIX: Majors

### [Issue P12] [`comprehension`, `structural`, `style`] - Severity: Major
*   **Affected Scope:** Lines 44, 126-138, 166, 325
*   **Discrepancy Description:** Several statements are unclear or contradict each other. Sources: S6 M3, S6 M4, S6 M5, S6 M9, C-M13, C-N1, C-N2, C-N3.
    *   "org" has two meanings.
    *   The sentence about one org per process reads backwards.
    *   The design never explains why varsity-core owns the ST prefix.
    *   The design says "two read tools and one write tool", but lists two tools.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Component 2.
    *   Change: Define the two meanings: "A *registry org* is the set of repositories in one registry file. A *forge org* is a GitHub or Gitea owner. The Varsity registry org spans two GitHub forge orgs."
    *   Location: Line 138.
    *   Change: "v1 serves one registry org per process. Serving several orgs from one process is a Future Improvement."
    *   Location: The ST example.
    *   Change: Add "varsity-core holds ST-001 and ST-002, which predate its VC prefix."
    *   Location: Line 166.
    *   Change: "One new read tool and one new comment-scoped tool."

### [Issue P13] [`design`, `implementation-readiness`, `codebase-details`] - Severity: Major
*   **Affected Scope:** Lines 115-121, 396-399 (item 6)
*   **Discrepancy Description:** The HTTP contract is incomplete. Sources: C-M5, R-M2, S6 N6.
    *   The `Mcp-Method` and `Mcp-Name` checks are missing. ST-006:85 requires them with error `-32020`.
    *   The behavior of GET and DELETE is not specified.
    *   Notifications are not covered.
    *   The status codes are not specified.
    *   The `--registry` flag is missing.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 6.
    *   Change: Add a table:

        | Request | Response |
        | --- | --- |
        | `POST /mcp` with a JSON-RPC request | 200, `application/json`. A JSON-RPC error is also returned with 200. |
        | A notification | 202 with an empty body. |
        | Headers that do not match the body | `-32020`. |
        | No token or an unknown token | 401. |
        | Disallowed `Origin` | 403. |
        | Body larger than 1 MiB | 413. |
        | `GET` or `DELETE /mcp` | 405. |
        | `/healthz` | 200. |
        | `/readyz` | 200 when the registry is loaded, otherwise 503. |

        The server prints `listening on <addr>` to stderr.
    *   Change: Add `--registry FILE`, `--github-api-url URL` (default `https://api.github.com`, used by tests), and `--request-timeout-seconds` (default 30).

### [Issue P14] [`codebase-details`, `implementation-readiness`, `comprehension`] - Severity: Major
*   **Affected Scope:** Lines 86-88 (CUJ 5), 182, 386-387, 404; `mcp.rs:294-306`
*   **Discrepancy Description:** Scopes are underspecified. Sources: R-M3, R-M4, S6 M7, S6 M10, C-N9.
    *   Write tools are hidden without `--write`, so a scoped token gets `-32602`, not `scope_refused`.
    *   No table maps tools to scopes.
    *   CUJ 5 is ambiguous.
    *   Scopes are assigned to the wrong slice.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Component 9.
    *   Change: "Hosted mode registers every tool. `tools/list` returns only the tools the caller's scopes allow. A call outside the caller's scopes returns `scope_refused`." Add a table:

        | Tools | Scope | Prefix check |
        | --- | --- | --- |
        | `story_config`, `story_list`, `story_show`, `story_lint`, `story_repos`, `story_graph` | read | No. Reads are org-wide. |
        | `story_update`, `story_note`, `story_create` | write | Yes, on the target story ID. `story_create` checks its prefix. |
        | `story_comments` | read | No. |
        | `story_resolve_comment` | comment | Yes. |
        | `story_graph_sync` | write | No. |

    *   Location: CUJ 5.
    *   Change: Split it into the three cases from S6 M7.
    *   Location: The slice table.
    *   Change: Move scopes to S0.

### [Issue P15] [`design`, `implementation-readiness`, `zoom-out`] - Severity: Major
*   **Affected Scope:** Lines 140-148, 414-421 (item 9)
*   **Discrepancy Description:** The git mechanics fail. Sources: R-M6, C-M4, I-M2, Z/N5, Z/N7, R-nit LRU.
    *   A push with a refspec from a `--mirror` clone fails.
    *   Fetches collide with the worktrees.
    *   The file lock in each worktree locks nothing.
    *   Full worktrees overflow the 2 Gi volume.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Component 3, Component 4, and item 9.
    *   Change: Replace the mechanics with these rules:
        1. Clone with `git clone --bare`, using fetch refspec `+refs/heads/*:refs/heads/*`.
        2. One mutex per repository serializes every git operation on that repository: fetch, worktree add and remove, commit, push, and prune.
        3. Worktrees use `git worktree add --detach --no-checkout <dir> <sha>`, then a sparse checkout of `backlog/` and `.story-time/`.
        4. Push with `git push origin HEAD:refs/heads/story-time/<ID> --force-with-lease=refs/heads/story-time/<ID>:<expected-or-empty>`.
        5. The LRU holds up to 64 parsed backlogs. Read worktrees are removed right after parsing.
    *   Change: Delete the `(repository, story)` mutex and the file-lock claim.

### [Issue P16] [`codebase-details`, `implementation-readiness`] - Severity: Major
*   **Affected Scope:** Lines 150, 469-472 (item 16); `next_id.rs:64-103`; `new_story.rs:94`
*   **Discrepancy Description:** `next_id_reserved` duplicates `next_id`, which already reserves IDs across every branch. Sources: R-M8, Z/N6.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 16.
    *   Change: "Reuse `next_id` unchanged. Every `story-time/*` branch is a local head in the bare clone. If a push loses the race, `story_create` fetches, allocates again, and retries once. It then returns `conflict`." Delete `next_id_reserved`.

### [Issue P17] [`design`, `zoom-out`, `implementation-readiness`] - Severity: Major
*   **Affected Scope:** Lines 147, 150, 170 (Non-Goals)
*   **Discrepancy Description:** A story created on the server has no issue, so the Rule 5 PR body cannot name one. Nothing publishes the story in hosted mode. Sources: C-M7, I-M5, R-M5.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Non-Goals.
    *   Change: "Publishing tracker issues from the server. A story created on the server stays unpublished until someone runs `story push` after the merge."
    *   Location: Component 4.
    *   Change: "For a story with no issue, the PR body uses `Stories: <ID>`."

### [Issue P18] [`codebase-details`, `design`, `zoom-out`, `comprehension`] - Severity: Major
*   **Affected Scope:** Lines 152-156, 457-462 (item 14)
*   **Discrepancy Description:** The lint plan contradicts itself and is incomplete. Sources: R-M9, R-M10, C-M6, I-M4, Z7, S6 M11.
    *   Line 153 says one thing about the local index, and item 14 says another (`org: None`).
    *   Local lint breaks after the backfill.
    *   `relates_to` is never checked.
    *   The rule fixed-size array `[Rule; 16]` is not updated.
    *   `OrgIndex` is private to `mcp`.
    *   `saga_tests.rs` is missing from the plan.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Component 5 and item 14.
    *   Change: Use these rules:
        1. Move `OrgIndex` to the new file `src/org_index.rs` (`pub(crate)`).
        2. Local `story mcp` builds the index from every `--root` (Z7). The CLI passes `None`.
        3. With `None`, an ID whose prefix the local repository does not own gives a **warning**, `independent.foreign_unresolved`. With an index, a missing or ambiguous ID gives an error (`independent.deps` or `links.ambiguous`).
        4. `relates_to` targets are checked by the same rule.
        5. The new rules (`links.prose_mention`) run only when `org` is `Some`.
        6. A foreign dependency that is not done keeps the existing warning.
        7. `RULES` grows to 18.
    *   Change: Add `saga_tests.rs` and the `Context` call sites (`lint.rs:531`, `mcp.rs:860`) to the file list.
    *   Change: S2 depends only on S0.

### [Issue P19] [`codebase-details`, `zoom-out`] - Severity: Major
*   **Affected Scope:** Lines 33, 152, 467 (item 15)
*   **Discrepancy Description:** Three files are missing from the `relates_to` change: `lenient_story.rs` `KEY_ORDER` (the writer), and the Node parity files `story.mjs:11` and `update-story.mjs:6`. No CUJ uses `relates_to`. Sources: R-M11, Z/N9, I-N5.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 15.
    *   Change: Add `lenient_story.rs`, `skills/story-time/scripts/src/domain/story.mjs`, and `skills/story-time/scripts/src/application/update-story.mjs`.
    *   Location: CUJ 2.
    *   Change: Add "`VE-046` also has `relates_to: [VC-109]` (paired story)."
    *   Change: `summary` gains `relates_to` in hosted mode only.

### [Issue P20] [`implementation-readiness`] - Severity: Major
*   **Affected Scope:** Lines 188-193, 390, 462
*   **Discrepancy Description:** Adding hosted fields to the shared schemas changes the local `tools/list` output. That breaks the byte-identical claim. Sources: R-M12.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 5.
    *   Change: "Hosted-only fields (`ref`, `proposed`, `repository`, `sha`, `fetched_at`, `relates_to` in `summary`, `state`, `pr_url`, `branch`, `head_sha`) live in separate hosted schemas that are selected by `Backend::Hosted`. Local schemas do not change."

### [Issue P21] [`codebase-details`, `style`, `zoom-out`] - Severity: Major
*   **Affected Scope:** Lines 158-164, 190, 443-451 (item 13); `graph.rs:124-129, 184-211, 271-274, 323, 414-421, 479`
*   **Discrepancy Description:** The design misstates existing Neo4j code and leaves the new parts undefined. Sources: R-M13, R-M14, C-N5, C-N7, Z/N2.
    *   It invents `index_unavailable`, but `graph_unavailable` already exists.
    *   It adds a `Repository` node, but `StoryRepository` already exists.
    *   It presents `depth` as new, but `depth` already exists.
    *   The traversal is undirected, and the design gives no directed query.
    *   The edge enums are closed.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Component 6 and item 13.
    *   Change: Use these rules:
        1. Reuse `graph_unavailable`.
        2. Extend `StoryRepository` with `key` and `sha`. Node identity stays `identity(repo)`, and the prefix map resolves an ID to that identity.
        3. The new argument is `direction` only: `dependents` uses `<-[:DEPENDS_ON*1..depth]-`, and `dependencies` uses `-[:DEPENDS_ON*1..depth]->`. `RELATES_TO` is returned only at depth 1.
        4. Add `RELATES_TO` to the `edge.type` enum, and add `skipped_ambiguous` to `sync_schema`.
        5. Sync once at startup.
    *   Change: Delete `MENTIONS` (see E1).

### [Issue P22] [`codebase-details`, `zoom-out`] - Severity: Major
*   **Affected Scope:** Line 441 (item 12); `lib.rs:122`; `gateway.rs:649-713`
*   **Discrepancy Description:** A `trait Tracker` already exists in `lib.rs`. A Gitea request builder already exists in `gateway.rs`. Sources: R-M15, Z9.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 12.
    *   Change: Rename the new trait to `Forge`. Implement GitHub over REST with the installation token. If Gitea stays (see E2), reuse `gateway::Gitea::request` (`pub(crate)`) and add `gateway.rs` (Modify).

### [Issue P23] [`implementation-readiness`] - Severity: Major
*   **Affected Scope:** Lines 166-171, 389 (component 7, item 5)
*   **Discrepancy Description:** The new tools have no schemas, no error codes, and no pagination rules. Sources: R-M16.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Component 7.
    *   Change: Add the schemas:
        *   `story_repos()` returns `[{key, prefixes, sha, fetched_at, state: ready|cloning|unavailable, last_error}]`.
        *   `story_comments({repository, id})` returns `{issue_url, comments: [{id, author, author_association, created_at, updated_at, body, resolved}]}`. It follows the `Link` header with `per_page=100`.
        *   `story_resolve_comment({repository, id, comment_id, note})` takes a `note` of 1 to 500 characters and returns `{reply_url}`.
    *   Change: Add the error codes `no_tracker_issue`, `comment_not_found`, `already_resolved`, and `note_required`. Issue coordinates are parsed from `remote_url` (`https://github.com/<owner>/<repo>/issues/<n>`).

### [Issue P24] [`style`, `codebase-details`, `zoom-out`] - Severity: Major
*   **Affected Scope:** Lines 180, 306, 431-436; `mcp.rs:1711-1722`
*   **Discrepancy Description:** The design overstates what the redactor needs. The regex already matches `ghs_` and `Bearer`. The values are fixed at startup. Sources: C-M12, R-M17, I-N2.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Component 8 and the NFRs.
    *   Change: "The existing redactor regex already covers installation tokens (`ghs_`) and `Bearer` values. `Redactor.values` becomes `RwLock<Vec<String>>` so that client tokens loaded at startup are also redacted when they appear without a prefix."

### [Issue P25] [`codebase-details`] - Severity: Major
*   **Affected Scope:** Lines 489-552 (items 20-25)
*   **Discrepancy Description:** The test files in `tests/` cannot reach `pub(crate)` internals. A fake `Tracker` cannot be injected into a subprocess. Sources: R-M18.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Items 22 and 25.
    *   Change: These become `#[cfg(test)]` modules in `src/org_index.rs` and `src/mcp/forge_auth.rs`, in the same style as `saga_tests.rs`.
    *   Location: Items 21 and 24.
    *   Change: These drive the binary as a subprocess against a local HTTP forge stub, selected with `--github-api-url`.

### [Issue P26] [`codebase-details`] - Severity: Major
*   **Affected Scope:** Lines 372-379, 474-487, 600
*   **Discrepancy Description:** The build file list and commands are incomplete or wrong. Sources: R-M19, R-M20, R-M21, R-M22.
    *   `help.rs` only includes `usage.txt`.
    *   The registration doc test also checks `SKILL.md`.
    *   The Docker build needs the repository root as its context.
    *   `Cargo.lock` is missing from the list.
    *   The `clippy` command has no manifest path.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 17.
    *   Change: Target `skills/story-time/scripts/usage.txt` and `skills/story-time/SKILL.md`.
    *   Location: Item 18.
    *   Change: "Build context is the repository root: `docker build -f native/story-push/Dockerfile .`. The builder stage runs `cargo build --release --locked --bin story`, and the runtime base is `debian:bookworm-slim` plus `git` and `ca-certificates`."
    *   Location: Item 4.
    *   Change: Add `Cargo.lock` and pinned versions.
    *   Location: The Verification Plan.
    *   Change: Use `--manifest-path native/story-push/Cargo.toml --locked` in every cargo command.

### [Issue P27] [`implementation-readiness`, `codebase-details`, `catalyst`] - Severity: Major
*   **Affected Scope:** Lines 196-200, 566-582 (item 28)
*   **Discrepancy Description:** The deployment section has several gaps. Sources: R-M23, M17, M18, I-M7, Z13, C-M9.
    *   The manifests are not split by slice.
    *   `readOnlyRootFilesystem` is applied to the whole pod, which breaks Neo4j.
    *   The Neo4j settings are not pinned.
    *   git has no `HOME` or `/tmp`.
    *   There is no ServiceAccount.
    *   The secret wiring is not pinned.
    *   Readiness waits for every repository.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: Item 28.
    *   Change: List the manifest contents for each slice:
        *   S1: everything except the Neo4j sidecar.
        *   S3: adds the Neo4j sidecar.
    *   Change: Specify these settings:
        *   `strategy: Recreate`.
        *   `readOnlyRootFilesystem` on the `story` container only, with `emptyDir` volumes for `/data` (`sizeLimit: 2Gi`), `/data/home`, and `/tmp`.
        *   The Neo4j image `neo4j:5.26-community` with taskgraph's environment: listen address 127.0.0.1, 512m heap, 256m pagecache, `runAsUser: 7474`.
        *   `automountServiceAccountToken: false`.
        *   Requests of 100m CPU and 256Mi memory. A limit of 768Mi, as in taskgraph core.
        *   An ExternalSecret using `ClusterSecretStore vault-taskgraph`, with the keys `global/infra.story-time.{clients,github-app-id,github-app-key}` and `property: value`, mounted at `/etc/story-time/secrets/`.
        *   Readiness that depends on the registry load only. A repository that is not yet mirrored returns `repository_unavailable`.

### [Issue P28] [`style`, `implementation-readiness`, `codebase-details`] - Severity: Major
*   **Affected Scope:** Lines 596-607 (Verification Plan)
*   **Discrepancy Description:** The verification plan has gaps and overclaims. Sources: style M3, M4, M5, R-N8, R-M24.
    *   S1 has no cluster check.
    *   The plan claims that passing tests "prove" byte identity.
    *   It claims that `ls-remote` "proves egress".
    *   The S0 check is in the wrong slice.
    *   The sandbox repositories are not named.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: The Verification Plan.
    *   Change: Write "shows that output covered by the existing contract tests is unchanged".
    *   Change: Move the cluster checks to S1:
        *   `/healthz` returns 200.
        *   `story_repos` lists both repositories, with a SHA and `fetched_at` for each.
        *   `story_show VC-109` works.
        *   After an upstream commit, `story_show` returns a new `sha` within the TTL.
        *   After a pod delete, the same SHA is returned when no new commit has landed.
        *   `curl -sI https://api.github.com` from the pod returns 200, which shows REST egress.
    *   Change: S0 checks are CI only.
    *   Change: S4 and S5 checks name a sandbox repository, `Varsity-Engine/story-time-sandbox`, which the owner must create (see E-list).

### [Issue P29] [`structural`, `style`, `zoom-out`, `catalyst`] - Severity: Major
*   **Affected Scope:** Lines 196, 273, 313, 53, 571
*   **Discrepancy Description:** The design states several infrastructure facts wrongly or leaves them open. Sources: C-M14, Z14, Z15.
    *   The memory sizing is not taskgraph's: taskgraph core uses 768Mi.
    *   The siblings use a single shared secret, not one token per client.
    *   The Ingress pattern comes from coffee-cluster-mcp, not taskgraph.
    *   The registry and the secret store are known facts, but the design lists them as open.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Change: Correct each claim.
    *   Change: Write "per-client hashed tokens are new. The siblings use one shared secret."
    *   Change: Cite coffee-cluster-mcp as the Ingress and ExternalSecret pattern, and taskgraph as the Neo4j sidecar pattern.

### [Issue P30] [`design`, `zoom-out`] - Severity: Major
*   **Affected Scope:** Lines 116, 181, 304, 323
*   **Discrepancy Description:** The token lifecycle is not specified. Sources: C-M10, I-N1.
*   ***AI Suggestion:*** PROPOSE FIX (the minimal version from Stage 8)
*   ***Proposed Edit:***
    *   Location: Component 9.
    *   Change: "Client tokens are 32 random bytes (base64url). To revoke one, remove its hash in Vault and restart the pod, which is safe because the pod holds only caches. Rate limiting and expiry are Future Improvements."

### [Issue P31] [`zoom-out`, `catalyst`, `codebase-details`] - Severity: Major
*   **Affected Scope:** Lines 126-138, 182, 399 (component 2, item 6, item 8)
*   **Discrepancy Description:** Two gaps. Sources: Z10, R-M5, N3, N6.
    *   The prefix map is derived from data. One stray file can make a prefix ambiguous.
    *   The audit line would be duplicated.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Location: The registry example.
    *   Change: Declare `prefixes = ["VC", "ST"]` and `default_branch = "main"` (optional; the default is the remote HEAD) on each `[[repo]]`. The key is `fetch_ttl_seconds` everywhere.
    *   Change: A lint check fails when a backlog file uses a prefix that is not declared.
    *   Location: Item 6.
    *   Change: "No new audit line is added. The existing per-call line (`mcp.rs:340-384`) gains `client_id` and `sha` in hosted mode."

---

## DECLINE (Nits, recorded only)

These are recorded and not fixed now:

*   **Cosmetic and editorial:**
    *   Use H4 for the components and `#####` for the items.
    *   Add a blank line before each list.
    *   Promote Future Improvements to H2.
    *   Move "proposed solution" out of The Problem.
    *   Add links for ADR 0006, ST-011, Rule 4, Rule 5, and Article VIII.
    *   Add a glossary.
    *   Item 3 should say Modify.
    *   Replace "(Step 6)".
    *   Use one capitalization for the org name.
    *   Rewrite item 12 as bullets.
    *   Use the rule IDs `independent.deps` and `spike.unblocks`.
    *   Remove "exactly like".
    *   Make the test oracles concrete.
    *   Remove the unmeasured timings ("well under one second", "about 1 s today").
    *   Remove "a few clients".
*   **Small technical nits:**
    *   Match the bot by numeric ID. P23 could absorb this.
    *   `toml_subset` support for arrays of tables.
    *   The PEM-to-DER step.
    *   The installation lookup endpoint.
    *   Name `ring::constant_time`.
    *   A Cargo `hosted` feature.
    *   The overlap between `story_graph_sync` and auto-sync.
    *   The ID allocation owner for story files.
*   **One deliberate keep:** Line 609, "Drafted with EGM", is required by the EGM design template (`egm-draft/references/design_doc.md`). The template is outside the reviewers' reading list.

## Remaining owner handoffs, outside the document

*   **GitHub App:**
    *   Create the GitHub App `storytime`.
    *   Install it on `Varsity-Engine/varsity-core` and `Team-Teddy-Development/varsity-engine`, with read access for S1.
    *   Store the App ID and key in Vault under `global/infra.story-time.*`.
*   **Sandbox repository:** Create a sandbox repository for the S4 and S5 checks.
*   **Image path:** Answer E3, the image build and push path.
