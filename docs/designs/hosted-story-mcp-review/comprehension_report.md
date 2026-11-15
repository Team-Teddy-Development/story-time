### What this proposes

The design runs the existing `story mcp` server as one shared HTTPS service at `storytime.teamteddy.net/mcp` on a cluster.

* **Clients and tokens:** Clients authenticate with their own bearer tokens. Each token carries scopes (read, write, comment) and an optional allow list of ID prefixes.
* **Mirrors and freshness:** The server keeps a bare git mirror of every repository in one "org" registry. It reads at the default-branch SHA and fetches again when the last fetch is older than a TTL (30 s).
* **Cross-repo links:** The server builds an in-memory index of the whole org, so a story can name a story in another repository by a bare ID (for example `VC-109`) in `depends_on` or the new `relates_to`. Lint resolves those IDs and rejects missing or ambiguous ones.
* **Neo4j:** Neo4j runs as a sidecar. It receives these cross-repo edges and answers dependents and dependencies queries. If Neo4j is down, only `story_graph` fails.
* **Writes:** A write never goes straight to the default branch. Each story gets its own branch (`story-time/<ID>`) and PR. The write is made in a temporary worktree and checked against a content-hash precondition. A bot commits it with a `Requested-by` trailer and pushes with `--force-with-lease`.
* **Comments:** New tools list a story's tracker-issue comments and "resolve" one by posting a bot reply that carries a hidden marker. A comment counts as resolved only while a bot-authored marker matches the comment's current `updated_at`.

The work ships in six slices, S0 to S5.

### How the existing system works, per this document

* **Story files:** story-time stores agile stories as markdown files with frontmatter, at `backlog/<PREFIX>-###.md` in each repository.
* **The CLI:** The Rust `story` CLI (`native/story-push`) edits the files and publishes them one way to GitHub or Gitea issues.
* **The MCP server today:** Since ADR 0006 and ST-011, `story mcp` runs a stdio-only, no-network MCP server over local checkouts. It lists, shows, lints, creates, updates, and notes stories. Writes need `--write`. With `--graph`, it syncs one repository's stories and `depends_on` edges into Neo4j.
* **What is missing today:** Links across repositories exist only as prose mentions, 157 of them in VC files. Nothing can read or track tracker comments.
* **Existing safeguards:** Writes already use a file lock and a SHA-256 precondition. Lint already has the `independent.deps` and `spike.unblocks` rules, and IDs come from `next_id`.

---

# Stage 6 Comprehension Test report: hosted-story-mcp.md

* **Target:** `docs/designs/hosted-story-mcp.md`. FeatureRange is lines 1 to 328, and DetailedRange is lines 329 to 609.
* **Reviewer:** `egm-comprehension`, with fresh context. It read the document alone, and no standard applied.
* **Result:** The reviewer could paraphrase both ranges. The review found 0 Blockers, 11 Majors, and 9 Nits.

## Major

**M1. "Option A" points to the wrong alternative (line 56).** Line 56 says writes use server-held clones "(Option A)" and HTTP uses the same binary "(Option A)". Alternative A (line 209) is "Neo4j or the tracker as the source of truth". The chosen options are not lettered anywhere.
*Fix:* Drop the "(Option A)" labels and state each decision in words, or point to the alternatives it replaces: "rather than Alternatives F, G, H, I" and "rather than Alternative N".

**M2. "Jev", its scores, and "the owner" are never explained (lines 213-297, 219, 243).** Many rejection reasons say "Jev chose X at 0.99". The document never says what Jev is or what the numbers mean, and it never identifies "the owner".
*Fix:* Add one sentence before the Alternatives that names Jev and the meaning of the 0 to 1 scale. Define "the owner" once, by role.

**M3. "org" means two different things, and team-teddy is never placed (lines 15-16, 44, 54, 126-138, 590).**
* The registry's `org = "varsity"` covers repositories in two GitHub orgs: `Varsity-Engine` and `Team-Teddy-Development`.
* "team-teddy" is also a Gitea org.
* Line 138 says the cluster runs one process for each org, but the deployment describes one pod.

The reader cannot tell whether team-teddy (Gitea) repositories are served in v1, by which process, or why a Gitea bot user is built.
*Fix:*
* Define "story org" (a registry) and "forge org" (a GitHub or Gitea owner).
* State which story orgs v1 deploys, and how many pods.
* Show a registry example for Gitea.

**M4. Line 138 contradicts Future Improvements (line 325).** Line 138 says "A later version of the server can serve one org per process". Line 325 lists "Serving several orgs from one process" as future work.
*Fix:* Write: "v1 serves one org per process; the cluster runs one process for each org. A later version could serve several orgs from one process."

**M5. The claim that varsity-core "owns VC and ST" is unexplained (lines 138, 518).** ST is story-time's own prefix. The reader cannot tell why varsity-core owns it, or whether this is a collision across story orgs.
*Fix:* State why varsity-core has ST files, or pick a neutral example.

**M6. The precondition hash is called two different things (lines 80, 142).** CUJ 4 says "blob SHA `A`". Component 4 says "the `base` SHA-256 of the file". These are different values, and the document never says that a read returns a hash for each file.
*Fix:* Use one term, "content hash (SHA-256 of the file bytes)". State that `story_show` returns it as `content_sha256`.

**M7. CUJ 5 is ambiguous for read tools (lines 86-88).** Read literally, it says that a read-only token calling a read tool is refused, which contradicts G1. The case of write scope without comment scope is not covered.
*Fix:* Split the cases:
* No token or an unknown token: every tool returns 401.
* A valid token without the needed scope, or outside its `prefixes`: the tool returns `scope_refused`.
* A read-only token on a read tool: the call succeeds.

**M8. The "proposed overlay" is undefined, and CUJ 3 does not show it (lines 18, 36, 140, 150).**
*Fix:* Define `state: "proposed"` as written to the PR branch but not yet merged. State that default reads do not show it until the merge, and that clients pass `ref: "story-time/<ID>"` to see it. Add that read to CUJ 3, or drop the word "overlay".

**M9. Component 7 announces three tools, but its table lists two (lines 166-171).** The intro says "Two new read tools and one new write tool", but the table and line 192 list only two tools.
*Fix:* Write "one new read tool and one new comment-scoped tool".

**M10. The slices disagree on when scopes and SHA fields arrive (lines 337-341, 390, 404, 496, 603).**
* Scopes appear in S4 in the slice table, but `caller.rs` and the `scope_refused` test are in S0.
* `{repository, sha, fetched_at}` is in S1, but the S0 cluster check expects a SHA.

*Fix:* Move scopes to the S0 row, or move the scope test and `caller.rs` to S4. Either remove the SHA check from S0, or state that S0 returns the commit SHA of the `--root` clone.

**M11. Do the new lint rules fire in the local CLI? (lines 153, 156, 459-462)** `remote.stale` needs no org index, and `links.prose_mention` could match local prefixes. If either rule fires locally, the claim that CLI output is byte-identical with `org: None` breaks.
*Fix:* State that both rules run only when `org` is `Some`, or state the opposite. Then make the byte-identical claim match.

## Nit

* **N1:** "coffee cluster", `coffee-apps`, `taskgraph-mcp`, `coffee-cluster-mcp`, `cc_deploy`, and `cc_rollback` are undefined (lines 15, 196, 200, 316, 487, 564). Add one line for each, or a link.
* **N2:** "Rule 4", "Rule 5", and "Article VIII" are undefined (lines 147, 212, 331, 557). Link the rulebook and paraphrase each rule inline.
* **N3:** ADR 0006 and ST-011 have no links (lines 11, 204, 219).
* **N4:** The registry example omits `default_branch` and has no Gitea entry (lines 123-136).
* **N5:** Several existing concepts are assumed without definition (lines 36, 143, 145, 152, 156, 164, 170, 426, 460):
  * the `set` rules
  * the repository file lock
  * the lint gate on `status=ready`, and the evidence that `status=done` needs
  * what `independent.deps` and `spike.unblocks` check
  * the frontmatter fields `remote` and `remote_url`
  * the "settings tracker"
  * `story_graph`'s current contract
  * the type `Js`
  * "ingest"

  Add a short glossary.
* **N6:** No HTTP status is given for an unsupported protocol version, and the sentence "plus `initialize` for older clients" is unclear (lines 119-121, 398).
* **N7:** "(Step 6)" is undefined, and item 3 should say Modify, not Create (line 370).
* **N8:** Gitea port `:30022` is not explained (line 582).
* **N9:** The design does not say what the losing `story_create` returns in an ID-reservation race (line 150). Line 510 implies a retry, so state it.
