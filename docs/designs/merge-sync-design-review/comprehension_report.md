# Stage 6 — Comprehension Test Report

*   **Target:** `docs/designs/merge-sync-design.md` (FeatureRange 1..374, DetailedRange 375..545)
*   **Reviewer:** `egm-comprehension` (fresh context; read only the target)
*   **Date:** 2026-11-16
*   **Verdict:** No Blockers: the reviewer could paraphrase every section. 5 Majors, 8 Nits.

## What this proposes

The design adds two automated checks around a pull request merge so that story tracker issues stay in step with their backlog cards.

**Before merge (ST-152).** The existing `story-link` check in `scripts/story-policy.rs` gains three rules:
- A `(partial)` marker that exempts a reference.
- An acceptance rule. Each non-partial `ST-###` card on the PR checkout must be `status: done` with no unticked `- [ ]` boxes under Acceptance Criteria.
- A ban on GitHub closing keywords (`Closes #N`) that point at story issues.

Rules 2 and 3 run in `warn` or `fail` mode, set by `STORY_ACCEPTANCE`. The template ships `warn`. An `incident` label exempts both rules.

**After merge (ST-151).** A new `story-sync.yml` workflow runs on `push` to the default branch. It calls a new Rust verb, `story sync-merge`, which:
1. Finds the merged PR for the commit.
2. Parses its `Stories:` line.
3. Runs the existing push planner and executor with a new `write_back = false` mode, which never creates issues and never writes card files.
4. Posts one comment per named issue, made idempotent by a SHA marker.
5. Skips with a report when sync is off, the tracker is unsupported, there is no PR, or there is no `Stories:` line.

A shared TSV fixture keeps the two `Stories:` parsers from drifting apart.

## How the existing system works, per this document

Each story is a card file, `backlog/ST-###.md`, and the card is the source of truth. `story push` mirrors each card to a GitHub, Gitea, or ADO issue. It closes the issue for `done`, `split`, or `archived` and reopens it otherwise. On every create or update it also writes back to the card: `remote`, `remote_url`, `updated`, and a changelog line.

`story-accept` is the only path to `done`. It checks the Definition of Done, CI, review threads, and per-criterion evidence. Today agents are expected to run `story-accept` and then `story push` after the merge. When they skip it, issues stay open or drift from their cards.

PRs declare their stories on a `Stories: ST-### (#N)` line. `story-link` currently only checks that the named stories and issues exist, and fails a PR with no reference unless it carries the `incident` label. The CLI gateway has no comment operation. The hosted MCP server can comment, but only through its own GitHub App.

## Findings

### Major

| # | Where | Issue | Fix |
| --- | --- | --- | --- |
| C1 | Decisions (103-119); Alternatives C, E | "TypeSafe Jev (typed choice)" is undefined. The Confidence column mixes `author choice` with bare numbers. R2's "(0.60, confidence 0.20)" gives two numbers with no explanation. It is unclear whether a weak decision is final. | Define Jev and the confidence score in one sentence. Explain the R2 numbers. Say that low-confidence decisions are final. |
| C2 | Overview item 1; rule 2 (207); item 10 (483-484) | The acceptance rule reads the "head branch" in two places and the "PR merge ref" in a third. These are different trees. | Use one ref everywhere. If it is the merge ref, say why it is equivalent for card content. |
| C3 | Rule 3 (214-216) | "The story label" and `validate_links` are not defined. | Name the label or the setting that defines it. Describe `validate_links` in one line. |
| C4 | Goals 83; step 1; item 6 | "Files-only tracker", "not a supported tracker", and `tracker: none` overlap. `sync` and `tracker` are not introduced. | Add a short note on the `sync` and `tracker` settings keys and their values. Use one term for the files-only case. |
| C5 | Overview 23; Problem 36; Future Improvements 365 | `story-accept` is called a "step" in one place and a "skill" in another. The design does not say what it writes or when it runs today. | Add two or three sentences: it is an agent skill, it edits the card, the author commits that edit, it runs after merge today, and it runs before merge under this design. |

### Nits

| # | Where | Issue | Fix |
| --- | --- | --- | --- |
| C6 | Alternative E (321) | "Article VIII" is undefined. | Name its source, or state the rule inline. |
| C7 | Header 7; 358; footer 545 | "EGM" and "correction block" are undefined. | Expand both once. |
| C8 | Problem 38-42 | "groom" is jargon. The design does not say that `varsity-mcp` is a consumer repository. | Explain both. |
| C9 | Overview item 1 (21) | The Overview says "also fails", but the template default is `warn`. | Add "(in `fail` mode; the template ships `warn`)". |
| C10 | Trigger (225) vs item 11 (490) | The trigger says "default branch" in one place and `main` in another. | Say how a consumer whose default branch is not `main` adapts the template. |
| C11 | CUJ 3 (156) | PR #345 is used but never introduced. | Add it to the Given. |
| C12 | Step 6 (248) | When `(#N)` and the card's `remote` disagree, the design does not say which wins. | State the precedence, or report the mismatch. |
| C13 | 265; Alt D; 393; 528; 533 | Several terms are undefined: "push lock", "open-issue title lookup", `--config`/`--dir`, `$T`, and "6 after the edit". | Explain each in one clause. |

## Overlap with Stage 5 decisions

- C1 and C6 overlap zoom-out N4.
- Rule 3 (C3) changes under M3: it now flags a keyword only on a `(partial)` reference. It still needs the label definition only if a non-card issue can be a target.
- Under B1, the check moves from `scripts/story-policy.rs` to `story check-pr`. C2 then applies to the `check-pr` checkout.
- Under M4, the "write_back = false mode" paraphrase becomes "`sync-merge` calls `Tracker::update` directly".

---

## Re-audit (revision 2) — 2026-11-16

The design was revised with the Stage 5 decisions and the Stage 6 fixes, then re-audited by a fresh `egm-comprehension` reviewer.
The verdict was no Blockers, 4 Majors, and 10 Nits. The re-audit cap is now reached (1 re-audit per stage).

**Applied after re-audit (revision 2.1):**
- R1 (Major): Step 4 now says that a push with no PR or no `Stories:` line does not mirror changed cards (ST-151 AC6). It says why changed cards are included, and that `check-pr` limits this case to direct pushes and `incident` PRs, which `--all` repairs.
- R2 (Major): Failure handling moved into steps 5 and 6, so a failure is recorded and the run continues. Step 7 reports `failed` and exits 1.
- R3 (Major): Roll-out step 3 now covers the consumer-repository template swap and what happens without it.
- R4 (Major): `pending_create` is defined inline (`lib.rs:316-340`).
- Nits: Definition of Done, Article VIII path, `remote`, the D8 numbers, the `head`/`existing` fields, the hosted MCP server, the trigger wording, the `IS_INCIDENT` source, `story-ids`, `--no-write-back` history, and the ST-152 AC mapping in Goals.

**Accepted residual:** none open from this stage.
