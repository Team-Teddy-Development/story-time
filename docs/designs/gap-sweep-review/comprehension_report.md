# Stage 6 Comprehension Report: gap-sweep (revision 2)

*   **Target:** `docs/designs/gap-sweep.md`. FeatureRange 1..307, DetailedRange 308..411.
*   **Reviewer:** `egm-comprehension` (fresh context, document only, no standard, no context files)
*   **Date:** 2026-11-13

### What this proposes

The design adds `gap-sweep`, a new story-time skill that is instructions only. It looks for work that has no story yet. It searches code markers (TODO/FIXME), skipped tests, doc claims the code does not support, Out of Scope and spike follow-up items in existing stories, and open tracker issues not linked to any story. It can also propose up to 2 "feature" rows, each backed by an artifact in the repository or tracker and naming a persona. Candidates that share a reference are merged, then sorted by tier and evidence count. They are then walked in order, and each is checked against every story, archived ones included, until 10 rows are kept. The user approves rows from a table. Approved rows go to `story-create` and then `story-estimate`. Rows that extend an existing story go to `story-edit`. Rows that come from an existing tracker issue adopt that issue: a new rule in `story-create` sets `remote` before the push. The implementation adds the skill, its Codex metadata, a registry entry, a router row, a README row, one rule in `story-create`, and a Rust test that checks the skill text.

### How the existing system works, per this document

story-time is a set of agent skills for managing a backlog of `ST-xxx` story files. The skills include `refinement`, `story-map`, `story-accept`, `spike`, `story-create`, `story-estimate`, and `story-edit`, with a `story` CLI underneath (`story list --json`, `story set`, `story judge`, `story push`). Every current skill starts from something already written down: a story ID, an epic, or a question. `story-create` checks for duplicates, asks for batch approval, runs `story judge`, and then follows a per-repository `sync` policy (`auto`, `ask`, `off`) when pushing to a tracker. The tracker can be GitHub, Gitea, ADO, Jira, or Linear. A push updates the existing issue when the story's `remote` is set and creates a new issue otherwise. "Jev" appears to be a scoring service that the design process used to settle decisions. Inside the product, its only entry point is `story judge`, which sits behind an "egress gate".

### Findings

**1. Major. Jev is undefined (lines 78-80, 89-96, 250-267, 301).** The Decisions Log and Alternatives justify choices with Jev numbers ("Jev 0.69, confidence 0.53", "near-tie 0.52 against 0.44, confidence 0.28"). A reader cannot tell what Jev is, what the numbers measure, what "confidence" means, or why 0.52 against 0.44 is a near-tie the owner overrode. Jev is also a product concept here (lines 78-80, 263), so it needs a one-line definition.

**2. Major. The egress gate is undefined (lines 92, 265, 302).** Rejecting Alternative C and the Security NFR both rely on "the egress gate the README promises". The document never says what it gates or guarantees, so the security claim cannot be checked.

**3. Major. Review finding IDs carry decisions (lines 22-25, 92-97, 106, 266, 274, 280, 287).** "Stage 5 review" is not explained. C3 (line 95) is cited with no content. C2 and E point to the round-1 report for the actual designs. That report is created alongside this design (Target File 9), so a reader may not have it. Revision 1 targeted a stale checkout, so it is unclear whether its findings still apply.

**4. Major. Three different ranking keys (lines 95, 124-125, 198-200).** "Independent source count", "two sources", and "number of evidence entries, then collector order" differ when two TODO lines cite the same issue. Say which key is used and what makes sources independent.

**5. Major. How features and gaps share the walk and the table is unspecified (lines 198-212, 218-220).** They are sorted separately, but the walk is one pass. The document does not say how the lists interleave, where feature rows appear in the table, or how "unused feature slots go to gaps" works across "next batch".

**6. Major. The Rust test's file list contradicts Target File 6 (lines 334-345, 368-369).** File 3 reads three files, not `skills/story-create/SKILL.md`, yet File 6 says file 3 asserts the `remote=` rule there. Line 341 credits gap-sweep with setting `remote`, while Filing (231-238) gives that job to `story-create`.

**7. Major. Adopting an issue rewrites it (lines 127-134, 231-238).** The push replaces the issue's title and body with the story's text, on a tracker others watch. The document does not state this side effect, whether the user is told before approving, or what happens to labels and comments.

**8. Nit. Doc promises collector is vague (line 184).** It does not say how the agent decides the code fails to provide a behavior. The walk's excerpt check covers only the doc side.

**9. Nit. "Covered by" and "statement" are undefined (lines 204-209).** One sentence on what "covered" means would help.

**10. Nit. Story follow-ups assume `story list --json` returns section bodies (line 185).** The document does not state this.

**11. Nit. Undefined project terms (lines 110-118, 310-313, 394-395, 406-407).** `draft`, "make it ready, then kick it off", "the two Rust policy suites", `story-link` and `story-ids`, "persona", and `native/story-push`.

**12. Nit. "Installed" skill files is ambiguous (line 334).** Source tree, or the output of `scripts/install.mjs`? `planning_install.rs` is not described.

**13. Nit. `sync` values are only implied (lines 15-16, 113, 273).** Define `auto`, `ask`, `off` at first use.

### Summary

Every section could be paraphrased, so there are no Blockers. The main gaps are the undefined Jev and egress gate (1, 2), which the decisions and security claim depend on. Next are the inconsistent ranking key and the unspecified feature/gap ordering (4, 5), and the test file list contradicting Target File 6 (6).

### Elephant note

Finding 3 overlaps Stage 5 re-audit finding R6. Finding 6 is a real defect: the `remote=` assertion must read `skills/story-create/SKILL.md`, and line 341 should credit `story-create`. Finding 7 adds a user-visible side effect that the approval table must state.
