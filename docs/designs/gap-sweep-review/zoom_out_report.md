# Stage 5 Zoom Out Report, re-audit: gap-sweep (revision 2)

*   **Target:** `docs/designs/gap-sweep.md` (lines 1..411), based on `origin/main` `af8dc9b`
*   **Reviewers:** `egm-zoom-out`, `egm-inventive-catalyst` (fresh context, read-only)
*   **GoldfishContext:** `AGENTS.md`, `skills/refinement/SKILL.md`, `skills/story-create/SKILL.md`,
    `skills/story-time/SKILL.md`, `skills/story-time/references/backends.md`,
    `skills/story-time/scripts/src/domain/judgement.mjs`, `native/story-push/tests/planning_install.rs`,
    `test/expected-skills.mjs`, `README.md`, `docs/spikes/ST-019-atlassian-lifecycle.md`
*   **Round 1:** `zoom_out_report_round1.md` (stale base). This is the one allowed re-audit.

## Round-1 status

Resolved: Z1 (no Jev from the skill), Z2 (hand off to `story-create`), Z4
(adopt issue via `remote`), Z5 and C1 (sort, then walk), C3 (merge by shared
reference), E0 (rebased on `origin/main`). Recorded as owner decisions: C2 (no
memory), Z8 (reserved feature rows). Reviewers confirmed `lib.rs:456`,
`story.mjs:11`, `judgement.mjs:123`, `story list --json`, settings, and the
`project-discovery` boundary.

## Findings

Status key: **Verified** (Elephant checked in code), **Judgment**, **Owner**
(reverses a recorded owner decision or Non-Goal).

### Major

**R1. Mechanical collectors produce only false positives here, and the walk does not filter defects.** (zoom-out)
`.skip(` matches `env::args().skip(1)` (`gitea-gateway/src/main.rs:232`,
`native/story-push/src/gateway.rs:1390`), and `xit(` matches every
`process.exit(` (`docs/playbook.md:268`). There is no real skipped test.
`TODO|FIXME` hits are string literals and this design's own files. `defect`
sorts first, and the excerpt check applies only to features and doc gaps, so
false defects lead the table. CUJ 1 assumes a skipped test that does not
exist. Fix: describe collectors by intent, run the excerpt check on every
kind, exclude `docs/designs/`, and base CUJ 1 on real gaps: open issues #11
and #12 have no story (`docs/planning/open-issues-2026-11-13.md`). Status:
**Verified**.

**R2. The Gitea collector needs the token the skill must never handle.** (zoom-out)
Gitea has no signed-in CLI. A raw API call needs `GITEA_TOKEN`, and
`skills/story-time/SKILL.md:125` says "Never ask for a token, and never
handle one." Fix: collect only through signed-in tools (`gh`, `az boards`,
Jira or Linear MCP). Report Gitea as "tracker collector skipped". A gated
native issue-list command is its own story. Status: **Verified**.

**R3. Adding the skill breaks `planning_install.rs`, and the plan's commands would not catch it.** (zoom-out)
`native/story-push/tests/planning_install.rs:76` asserts
`"Installed 19 story-time skills"`. `testCommand` runs no Cargo tests, and
the plan runs only `--test gap_sweep_skill`. Several planned assertions
duplicate `test/skills.test.mjs:61-63`, `test/install.test.mjs:52`, and the
loop at `planning_install.rs:88-92`. Fix: update the count to 20, add
`gap-sweep` to that loop, keep the new Rust file to gap-sweep-specific
rules, and run the full `cargo test --manifest-path native/story-push/Cargo.toml`.
Status: **Verified** (line 76).

**R4. "Bounded work" does not cover the doc-promise and feature-anchor collectors.** (zoom-out)
Both judge the whole repository before the sort, which contradicts "sorting
uses collector data only", and the walk checks the same excerpts again. Fix:
these collectors only gather candidate claims with `path:line`, and the walk
does the judgment for walked rows. Status: **Verified** (design text).

**K1. The deterministic core runs as prose and cannot be tested.** (inventive-catalyst)
Collect, merge, sort, counts, and the "next batch" cursor are mechanical.
The README says stacks "hand the deterministic parts to a tested script".
The planned test can only grep `SKILL.md`. Proposal: a read-only native
`story gaps --json [--after <cursor>]` that runs the code, test, and
story-follow-up collectors, merges, sorts, and returns a cursor. It takes
tracker issues on stdin, so it needs no credentials. The agent keeps the
judgment steps. Status: **Owner**. It reverses the "No new `story` CLI
command" Non-Goal and brings a Rust executable into scope.

**K2. The walk does not check candidates against rows already kept.** (inventive-catalyst)
Two signals about the same work with no shared ID (a `TODO` and a skipped
test of the same function) become two rows. `story-create` checks only
against existing stories, not the rest of its batch. Fix: in the walk,
compare each candidate against kept rows too, and fold matches in. The cost
stays bounded by the cap. Status: **Verified** (design text and
`skills/story-create/SKILL.md:27-29`).

### Nit

**K3. Starvation comes from a deterministic tie-break, not only from no memory.** (inventive-catalyst)
Rotate ties within a tier and evidence band by a seed such as the HEAD SHA,
show it in the report, and show "N not yet walked" on the table. Status:
**Judgment**. Trades reproducibility for coverage.

**K4. A failed collector looks the same as an empty result.** (inventive-catalyst)
Each collector reports `ok`, `skipped (reason)`, or `failed (error)`. Say "0
new candidates" only when every enabled collector is `ok`. Status:
**Verified** (design text). This matches the RCA-on-sight rule against
treating a failure as absence.

**R5. The `story-create` adoption rule is useful on its own.** (zoom-out)
It is the identity-preserving intake for #11 and #12 that ST-028 named. It
fits better at `story-create`'s sync step (`SKILL.md:109-114`). Fix: file it
as its own small story that gap-sweep depends on. Status: **Judgment**.

**R6. The decisions cite round-1 finding IDs instead of reasons.** (zoom-out)
Add one line of reasoning inline. Status: **Judgment**.

## Summary for the author

*   R1 to R4, K2, K4, and R6 are text edits inside the current architecture.
*   K1 is the one architecture question. It reverses a Non-Goal and needs the
    owner.
*   R5 and K3 are optional.
