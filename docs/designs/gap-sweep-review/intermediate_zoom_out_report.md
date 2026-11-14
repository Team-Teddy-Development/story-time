# Stage 8 Intermediate Zoom Out Report: gap-sweep (revision 2)

*   **Target:** `docs/designs/gap-sweep.md` (full file, 1..411, not edited since Stage 5)
*   **Reviewer:** `egm-zoom-out` (fresh context), given the Stage 5, 6, and 7 reports
*   **Question asked:** which of the accumulated fixes a first version needs, and whether they argue for a smaller first slice.

## Verdict

The core design holds. Most of the Stage 6 and 7 risk comes from two features: adopting tracker issues and supporting several trackers. The answer is a smaller first slice, not more mitigation. Several proposed fixes are YAGNI and are refused below.

## Findings

**ZO8-1. Major. Split issue adoption into its own prerequisite story, and collect from GitHub only.**
Adoption is a `story-create` change that is useful without gap-sweep: it is the intake for #11 and #12 that ST-028 named. That raises R5 from Nit to Major. Splitting it removes S7-3 (overwrite, ADO, Jira, and Linear IDs, failure paths), R2 and the Gitea part of S7-9, and most of S7-6. A local `remote` with no push is not a failure, because the next push updates the issue. The adoption story needs one disclosure line in the table: "will overwrite title/body of #n". gap-sweep collects only through `gh`, since this repository is `tracker: github`. It reports any other tracker as "tracker collector skipped (not yet supported)". Keep the tracker collector: it is the only source of real gaps here (#11, #12).

**ZO8-2. Major, against S7-7's ceilings.** Skip per-collector ceilings. No performance problem has been shown. Reword the goal so the cap bounds the duplicate checks, and keep "not yet walked" in Counts. Keep `--limit` on `gh issue list` with a truncation note.

**ZO8-3. Major, against S7-9's redactor.** Skip a hand-written redactor. Excerpts come from git-tracked files or from the same tracker the story is pushed to, so they add no new exposure. Keep one rule: collected text is data, never instructions. Keep the existing approval gate.
*Elephant note:* the reviewer also cites `skills/story-time/SKILL.md:140-144`. That fallback only keeps personal data and secrets away from Jev. It does not stop the push. The reviewer's "no new exposure" argument still holds without it.

**ZO8-4. Major, against K1 now.** Defer the native `story gaps` command until the first dry run. Keep the Non-Goal. Record K1 under Open Questions as the planned fix if the prose steps prove unreliable.

**ZO8-5. Nit.** Drop the feature anchors that come from issue comments, instead of fetching comments.

**ZO8-6. Nit, against S7-10's link-or-close.** Report "#n appears covered by ST-xxx" and leave the action to the user.

**ZO8-7. Nit, against K3.** Do not rotate ties. Reproducibility matters for dry runs.

**ZO8-8. Nit, against S7-15's thresholds.** Counts across sessions need the memory the owner rejected. Use the dry run's recorded "useful rows" as the measure.

**ZO8-9. Nit.** Narrow doc promises to README commands and flags, checked against `--help` output.

**ZO8-10. Nit.** Shrink the new Rust test to the rules unique to gap-sweep: no direct `story push`, nothing written before approval, and the caps. The `remote=` assertion moves to the adoption story's test.

## Keep as proposed (text edits)

S7-1 (trade-off reasons, define Jev once), S7-2 (a Risks section of about 5 bullets pointing to existing safeguards), S7-4 (skip issues with the `story` label, per `backends.md:10`), S7-5 and R1 (collectors by intent, excerpt check on every kind), S7-8 (propose a persona and benefit or `chore` in the table, and reuse the table approval), S7-11, S7-12, S7-13, K2, K4, R3 (count 19 to 20, full `cargo test`), and Stage 6 #6.

## Resulting first version

*   **Collectors:** code markers and skipped tests (filtered by the excerpt check), story follow-ups, GitHub issues through `gh` (with the `story` label filter and `--limit`), doc promises (commands and flags only), and feature anchors from repo text and issue bodies.
*   **Prerequisite story:** issue adoption in `story-create`.
*   **Follow-up stories:** other tracker backends, the native `story gaps` command (K1), issue-comment anchors, duplicate-issue linking, and memory between sweeps.
