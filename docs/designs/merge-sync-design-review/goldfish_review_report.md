# Goldfish Resolution Report — Stage 7, round 1

*   **Target:** `docs/designs/merge-sync-design.md` (FeatureRange)
*   **Findings source:** `critic_report.md`
*   **Resolver:** Elephant (primary context). The owner delegated decisions to TypeSafe Jev.
*   **Gate:** Jev chose "resolve" (confidence 0.40; "pause" 0.39).
*   **Result:** Revision 3. 31 findings were classified as PROPOSE FIX and applied. 1 finding was recorded as an accepted risk. 0 were contested.

## Design decisions taken during resolution (Jev)

| Finding | Decision | Confidence |
| --- | --- | --- |
| K-M3 ordering | One run at a time. Each run covers the range from the last successful run to HEAD (D13). | 0.61 |
| K-M2 bare `#N` | Map it to the card by `remote` (D14). | 0.22 (weak) |
| K-M7 keywords | Rule 4 covers any story-type target that no passing reference covers. Commit-message keywords are a documented limit (D15). | 0.56 |
| K-M8 no-PR mirror | Mirror changed cards. Only comments need a PR. This amends ST-151 AC6 (D16). | 0.42 (weak) |
| K-M13 mode source | Repository variable `vars.STORY_ACCEPTANCE` (D17). | 0.06 (weak; the technical reason decides it) |

## Classification

| ID | Class | Applied change |
| --- | --- | --- |
| K-B1 | PROPOSE FIX | Amended the cards. ST-151: AC1, AC6, Out of Scope, and Notes. ST-152: Story, AC1, AC2, AC5, Notes, and `depends_on: ST-153`. Both cards were published (#344, #346). The Goals cite the amended ACs. |
| K-B2 | PROPOSE FIX | Every Reason for Rejection now states its trade-off. Dropped "by author". Alternative F now points to D13. |
| K-M1 | PROPOSE FIX | Defined the reference unit grammar in Terms and Settings. Added a parser test for the CUJ 2 string. |
| K-M2 | PROPOSE FIX | Rule 2 maps a bare `#N` to its card (D14). |
| K-M3 | PROPOSE FIX | Runs are serialized and cover the range from the last success. A weekly `--all` runs. CUJ 6 and Alternative F were rewritten. |
| K-M4 | PROPOSE FIX | Step 3 gives the selection rule for merged PRs in the range. Step 1 checks that the tracker repository equals the code repository. |
| K-M5 | PROPOSE FIX | Uses `fetch-depth: 0`. A last success that is not an ancestor takes the `--all` path. |
| K-M6 | PROPOSE FIX | Both verbs read `prefix`. Added an `XY-` test. |
| K-M7 | PROPOSE FIX | Reworded the closer statement. Extended rule 4. Added the commit-message Non-Goal. |
| K-M8 | PROPOSE FIX, with one risk accepted | Added a Risks and Mitigations section. The hand-edited `done` risk is accepted: PR review is the control. |
| K-M9 | PROPOSE FIX | Verified the label recolor in `gateway.rs:886-901` against `gh label create --help`. Filed it as bug ST-154 (#350), a prerequisite of ST-151. Cost is now stated per call and marked as an estimate. `--all` is paced. |
| K-M10 | PROPOSE FIX | An ID/`(#N)` mismatch is a hard link rule. After merge it is recorded as a failure. |
| K-M11 | PROPOSE FIX | Added Alternatives G (scheduled only), H (release binaries), I (`workflow_call`), J (status only), and K (fail from day one). Corrected D's create path (`lib.rs:224-232`). |
| K-M12 | PROPOSE FIX | Terms and Settings states the CI meaning of `sync`. |
| K-M13 | PROPOSE FIX | The mode comes from a repository variable. |
| K-M14 | PROPOSE FIX | Every skip names the SHA. CUJ 4 was rewritten. |
| K-M15 | PROPOSE FIX | Promoted Additional Resources and Future Improvements to H2. Added an H2 Roll-out. |
| K-N1–K-N14 | PROPOSE FIX | Applied: Article VIII quoted without a home-directory path, "often" removed, review metadata moved to Resources, example numbers in the CUJs, confidence definition fixed, "a check and a sync", type label noted, required-check risk, variable kind, "already on the default branch", Terms H2, fewer horizontal rules, "Proposed Architecture", merge-ref approximation, comment pagination, `GH_TOKEN` scoped to its step, cache scope, success measure, warn-mode reopen risk. K-N12: step 4 now comments on each issue it closes or reopens that step 5 does not cover, keyed on the last commit in the range that changed the card (ST-151 AC2). |

## Settled list (carried to later rounds)

- K-M8(c): A hand-edited `done` passes `check-pr`. This is an accepted risk; the control is PR review and the `story-accept` changelog line.
