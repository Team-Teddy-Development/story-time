# Stage 8 — Intermediate Zoom Out Report

*   **Target:** `docs/designs/merge-sync-design.md`, revision 3.1 (full file)
*   **Reviewer:** `egm-zoom-out` (fresh context), focused on the Stage 6 and Stage 7 additions
*   **Standard:** `zoom-out-standard.md`
*   **Verdict:** Changes required. 0 Blockers, 4 Majors, 5 Nits.

The core split still holds: a check before the merge (ST-152) and a sync after it (ST-151). These Stage 6 and 7 additions are forced by real constraints, and they stay:

*   the range anchor;
*   two jobs, the first with `permissions: {}`;
*   the retryable/permanent split;
*   `vars.STORY_ACCEPTANCE`;
*   first-parent walking;
*   the merge-ref note;
*   the rule tiers;
*   the `v1` marker namespace;
*   the 11 Alternatives.

The design grew beyond the stories in three places: closing keywords, a second comment type, and estimates that were not redone.

## Majors

| ID | Where | Issue | Suggested fix |
| --- | --- | --- | --- |
| Z8-M1 | Step 4 state comment; CUJ 3; CUJ 4; ST-151 AC6 | The state comment contradicts CUJ 4 and AC6 ("no comment" on a direct push), and CUJ 3 ("one comment"). For a squash merge, it can share the merge comment's marker key. | Use one comment type. The merge comment satisfies AC2 when the PR names the story. Post a state comment only for unnamed cards, with its own key `merge-sync-state=<sha>`. Make the CUJs, the Goals, and AC6 agree. |
| Z8-M2 | Rule 4; step 4, third selection kind; D7/D15; Risks | Three mechanisms cover keyword closes. The sync-side selection matters only in `warn` mode and for `incident` PRs, and the weekly `--all` covers those. | Keep rule 4 (ST-152 AC5). Drop the keyword-target selection from `sync-merge`. Restore ST-151's Out of Scope. Change the Risks row to cite the weekly `--all` lag. |
| Z8-M3 | Rule 2 | The rule re-implements criteria parsing. `story::acceptance_criteria` (`story.rs:236`) handles `*` bullets, `[X]`, and bullets with no box, and falls back to Exit Criteria for spikes. As written, a spike passes with unticked Exit Criteria. | Rule 2 uses `story::acceptance_criteria` and requires every entry to be `done`. Add a contract test for a spike. |
| Z8-M4 | Header; ST-151/ST-152 sizes; plan items 1–12 | The scope grew a lot, and the cards were amended to fit. Neither was re-estimated (ST-152 is still 3, ST-151 is still 5). | Re-estimate both, or split the deferrable parts (rule 3 link consistency, `--all` pacing, the 200-commit cap) into follow-up stories. |

## Nits

| ID | Issue | Suggested fix |
| --- | --- | --- |
| Z8-N1 | Commands, JSON fields, and rates are back in the architecture section. | Move them into items 3, 4, and 11. |
| Z8-N2 | The 200-commit cap and the `--all` pacing are premature. | Drop the cap and let a rate limit be a retryable failure. Move pacing to Future Improvements. |
| Z8-N3 | D8b, D11, D12, and D17 are not decisions. | Fold them into Roll-out, Non-Goals, and Terms. |
| Z8-N4 | The Risks table repeats mechanisms already in the body. | Keep the accepted and open risks. Point to the body for the rest. |
| Z8-N5 | Renaming the workflow, or expiry of its run history, resets the anchor. | Add both to the `--all` fallback cases and to the matching Risks row. |

## Resolution (revision 3.2)

On the third try, after two `fetch failed` connection errors, Jev chose to update the design and then run Stage 9. Its confidence was 0.28, with update at probability 0.52 and pause at 0.47.

| ID | Resolution |
| --- | --- |
| Z8-M1 | Named stories get only the merge comment. A card that no PR names and whose status changed, including on a direct push, gets one state comment with its own marker `merge-sync-state=<commit>`. ST-151 AC6 and CUJ 4 are amended. Jev chose this at 0.39, against 0.38 for keeping AC6 as it was: a coin flip. |
| Z8-M2 | Removed the keyword-target selection from `sync-merge`. `closing_targets` is used by `check-pr` only. Restored ST-151 Out of Scope. The Risks row cites the weekly `--all` lag. |
| Z8-M3 | Rule 2 uses `story::acceptance_criteria` (`story.rs:236`), including Exit Criteria for spikes. Added a contract test for a spike. |
| Z8-M4 | Re-estimated at the owner's delegated request (Jev 0.60). ST-151 went from 5 to 8, ST-152 from 3 to 5. Lint now warns `size.small` on ST-151 (8). A split is a candidate for `story-split` before kickoff. |
| Z8-N1 | Removed the `gh run list` command from step 2. The JSON field list stays in step 6. |
| Z8-N2 | Dropped the 200-commit cap and the `--all` pacing. A rate limit is a retryable failure. Pacing moved to Future Improvements. |
| Z8-N3 | Folded D8b, D11, D12, and D17 into Roll-out, Non-Goals, and Terms. |
| Z8-N4 | Not applied. The Risks table keeps its mitigation rows, which consumer-repository readers use as a checklist. |
| Z8-N5 | Added a renamed workflow and expired run history to the `--all` fallback cases and to the Risks row. |
