# Stage 7 — Critic Review Report (round 1)

*   **Target:** `docs/designs/merge-sync-design.md`, revision 2.1, FeatureRange lines 1–445
*   **Reviewers:** `egm-design` (completed), `egm-structural` (completed), `egm-style` (completed)
*   **Standards:** `feature-spec-standard.md` (design, structural); persona only (style)
*   **Date:** 2026-11-16
*   **Verdict:** Not clean. 2 Blockers, 15 Majors, and 14 Nits after deduplication.

Verification notes:
- The design and structural reviewers confirmed the code citations `gateway.rs:400/:511/:912`, `story.rs:144-185`, `playbook.md:147-190`, and `comments.rs:11`.
- I confirmed two claims:
  - `ensure_github_labels` runs `gh label create <name> --force` with no `--color` (`gateway.rs:886-901`).
  - Settings carry `prefix` (`config.rs:208`).

## Blockers

| ID | Source | Lines | Finding | Fix |
| --- | --- | --- | --- | --- |
| K-B1 | design B1, structural F3, style 3/4 | 104–119 | The Goals claim ST-152 AC1–AC6 and ST-151 coverage, but the design contradicts the cards on four points. (1) ST-152 AC5 bans any closing keyword on a story issue; D7 allows it. (2) AC1 and AC2 say "head branch"; the design reads the merge ref. (3) ST-152 Notes put the check in `story-policy.rs`. (4) ST-151 Notes name `--no-write-back`, and ST-151 Out of Scope bans keywords. | Amend the cards to match D1, D3, and D7 and the merge ref. Cite the amended ACs. |
| K-B2 | structural F1, design M11, style 7 | 346–399 | The Reasons for Rejection are pointers ("Rejected by author (Dn)") with no trade-off. "By author" conflicts with the Jev delegation. F points to the wrong decision. | State each trade-off inline. Drop "by author", or say "adopted Jev's call". |

## Majors

| ID | Source | Lines | Finding | Fix |
| --- | --- | --- | --- | --- |
| K-M1 | design M1 | 275–276 | `(partial)` binds to `#346`, not `ST-152`, in `ST-152 (#346) (partial)`, so CUJ 2 fails in `fail` mode. | Treat a reference as one unit, `ID [(#N)] [(partial)]`. Add a parser test for the CUJ string. |
| K-M2 | design M2 | 260–266, 298 | A bare `Stories: #344` skips rules 2–3, and its card is never selected. | Map `#N` to a card by `remote`, or require an ID. |
| K-M3 | design M3, style 2 | 219–221, 330–333 | The claim "last to finish writes newest" is false: each run reads HEAD once, so a run that fetched early can finish late, and nothing re-selects the card. | Serialize runs and select over the range from the last synced point, or re-read HEAD until stable. Add a scheduled `--all`. |
| K-M4 | design M4 | 284–295 | One push can carry several PRs (a merge queue, or a fast-forward). The commit→PR API can return open PRs. The tracker repository may differ from the code repository. | State the selection rule (merged, base = default branch, walk `before..after`). Require tracker repository = code repository for `GITHUB_TOKEN`. |
| K-M5 | design M5 | 298–301 | The diff needs `before` locally. A force push can make `before` unreachable. | Require `fetch-depth: 0`. When `before` is unreachable, fall back to `--all` or a recorded failure. |
| K-M6 | design M6 | 260–266 | The `ST-` prefix is hard-coded. Settings carry `prefix`. | Both verbs read `prefix`. Add a test with another prefix. |
| K-M7 | design M7, style 1 | 49–51, 269–273 | "The gateway is the only closer" contradicts D7. A keyword can close an unnamed story's issue silently. Commit-message keywords are not checked. | Reword. Extend rule 4 to keywords on story-type issues not covered by a passing reference. Document commit-message keywords as a known limit. |
| K-M8 | structural F2, design M8 | — | There is no Risks & Mitigations section. Missing risks: `story-link` not required, reverts, hand-edited `done`, legacy warn noise, outage recovery, rate limits, token scope, the warn-to-fail switch never happening. | Add an H2 Risks & Mitigations table. |
| K-M9 | design M9, style 6 | 411–416 | Idempotency and cost are understated. Each CI process re-runs `gh label create --force` with no color, which can recolor labels on every merge. `issue edit` always runs. `--all` hits secondary rate limits. The timings are unmeasured. | Skip existing labels. Diff before edit. Pace `--all`. Mark the timings as estimates. |
| K-M10 | design M10 | 320–323 | A `#N`/`remote` mismatch is caught only after merge and drops the comment. | Make it a link rule in `check-pr`. Record a post-merge mismatch in `failed`. |
| K-M11 | structural F4, design M11 | 146–159, 346–399 | Alternatives do not cover D4, D8, or D10 (prebuilt binaries), or reusable `workflow_call` and scheduled reconcile. Alternative D's cons are wrong: an unlinked card takes the direct `create` path (`lib.rs:224-232`). | Add the missing alternatives. Correct D. |
| K-M12 | design M12 | 94–100, 292 | In CI, `sync: ask` acts as `auto`, and the design does not say so. | State the CI meaning of each value. The workflow's presence is consent. |
| K-M13 | design M13 | 155, 425 | A PR can set `STORY_ACCEPTANCE: warn` in its own workflow file, or `sync: off`, and pass. | Read the mode from `vars.STORY_ACCEPTANCE`. Note the settings-file limit. |
| K-M14 | style 5 | 120–122, 202–204 | CUJ 4 and step 1 skip messages lack the SHA that ST-151 AC6 requires. | Put the SHA in every skip message. |
| K-M15 | structural F5 | 431–443 | Additional Resources and Future Improvements are H3s that nest under NFR. | Promote both to H2. |

## Nits

| ID | Source | Lines | Finding |
| --- | --- | --- | --- |
| K-N1 | style 8, structural F8, design N4 | 389 | The Article VIII path is in a home directory. Quote the rule. |
| K-N2 | style 9 | 30–31 | "Often skipped" is unmeasured. |
| K-N3 | style 10, structural F9 | 4, 11–13, 189 | Review metadata sits in the body. CUJ 3 uses the real PR #345. |
| K-N4 | style 11 | 141–143 | The definition of confidence conflates confidence with option probability. |
| K-N5 | style 12 | 33 | "Two checks" should be "a check and a sync". |
| K-N6 | style 13 | 22 | `story-link` also checks the type label and that the reference is not a PR. |
| K-N7 | style 14, structural F11 | 305–307 | "Cannot merge" holds only when `story-link` is required. The rule reference is unclear. |
| K-N8 | style 15 | 155, 425 | Variable kind is unstated (see K-M13). |
| K-N9 | style 16 | 72–73 | "Reviewed merge" assumes review is enforced. |
| K-N10 | structural F6, F7, F10 | 92–100 | The Settings H3 placement, inconsistent horizontal rules, and the "Proposed Architecture" label. |
| K-N11 | design N1 | 252–256 | The merge-ref claim is overstated. Call it the "closest approximation". |
| K-N12 | design N2 | 318–323 | A changed but unnamed card closed in step 5 gets no comment (ST-151 AC2 wording). |
| K-N13 | design N3, N5, N6 | 318, 157 | Comment pagination; scope `GH_TOKEN` to the sync step only; cache scope on PR branches. |
| K-N14 | design N7, N8 | Goals, CUJ 5 | No success metric. Warn-mode close-then-reopen noise is not described. |

## Traceability

| Consolidated | design | structural | style |
| --- | --- | --- | --- |
| K-B1 | B1 | F3 | 3, 4 |
| K-B2 | M11 | F1 | 7 |
| K-M3 | M3 | — | 2 |
| K-M7 | M7 | — | 1 |
| K-M8 | M8 | F2 | — |
| K-M9 | M9 | — | 6 |
| K-M11 | M11 | F4 | — |
| K-M14 | — | — | 5 |
| K-M15 | — | F5 | — |
| K-M1, M2, M4, M5, M6, M10, M12, M13 | M1, M2, M4, M5, M6, M10, M12, M13 | — | — |
