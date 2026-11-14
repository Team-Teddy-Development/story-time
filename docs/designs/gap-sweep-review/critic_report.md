# Stage 7 Critic Report: gap-sweep (revision 2), round 1

*   **Target:** `docs/designs/gap-sweep.md`, FeatureRange 1..307
*   **Reviewers:** `egm-design` (completed), `egm-structural` (completed), `egm-style` (completed).
    The design and structural reviewers used `feature-spec-standard`.
*   **Totals before deduplication:** design 2 Blocker, 9 Major, 7 Nit; structural 0, 3, 6; style 0, 9, 10.
*   **Prior-stage overlap:** each item notes the matching finding from Stage 5 (`zoom_out_report.md`) or Stage 6 (`comprehension_report.md`).

## Blockers

**S7-1. Alternatives rejected on unlinked owner choice or a bare Jev score.** (design-1-B1, structural-1-2, style-1-5)
Lines 89, 93, 96, 250-251, 258, 288, 294. The feature-spec standard requires a
concrete technical or scope trade-off, and it forbids attributing a rejection
to a person without a link. For Alternative A, the cited Jev numbers favor the
rejected option. Fix: write each reason as the trade-off already listed in its
Cons. For example, A breaks `refinement`'s "every open story ID exactly once"
contract and couples intake to `planning_install.rs`. F adds a format and
tracker writes. Keep the owner decision and Jev numbers as provenance, not as
the reason. Explain Jev once. Also Stage 6 #1, Stage 5 R6.

**S7-2. No Risks & Mitigations section.** (design-1-B2, structural-1-1)
The standard requires failure modes, security, scale limits, and mitigations.
Fix: add the section, covering S7-3 to S7-9 below.

## Major

**S7-3. Adopting an issue overwrites it, and failure paths are not defined.** (design-1-M2, style-1-4; Stage 6 #7)
The push replaces the title and body and adds managed labels. The update has
no rollback. Under `sync: off` or a declined `ask`, `remote` is set locally but
nothing is pushed. ADO, Jira, and Linear IDs are not defined. Fix: show the
overwrite in the approval table, limit adoption to native-push trackers, and
define each failure path.

**S7-4. Issues owned by stories on unmerged branches look like gaps.** (design-1-M3)
`story list --json` sees only the local checkout. Fix: skip issues that carry
the managed `story` label, and state the behavior in tracker-as-backlog mode.

**S7-5. Collector patterns are noisy, and the noise ranks first.** (design-1-M4, structural-1-3; Stage 5 R1)
`.skip(` matches Rust `Iterator::skip`. `XXX` and `TODO` match placeholders
and this design's own files. The table row also breaks, because its `|`
characters are not escaped. Fix: describe collectors by intent, anchor test
skips to test contexts, exclude `docs/designs/` and vendored paths, run the
excerpt check on every kind, and move regexes out of the table.

**S7-6. The tracker query misses comments and truncates without warning.** (design-1-M5)
`gh issue list` returns 30 items without `--limit`, and the GitHub command
reads no comments. Fix: paginate and report truncation. Fetch comments, or
drop comment-based feature anchors.

**S7-7. "Bounded work" and "no duplicates" claim more than the design does.** (design-1-M6, style-1-1, style-1-2; Stage 5 R4)
The worst case walks every candidate. Two collectors run judgment before the
sort. Duplicate detection is agent judgment. Fix: add ceilings per collector
and on walked candidates, report truncation, and reword both goals to what
the design does.

**S7-8. Gap rows have no persona or benefit, so `story-create` stalls or invents them.** (design-1-M7)
`story-create` requires a persona and benefit and forbids inventing them. It
also asks for batch approval a second time. Fix: propose a persona and
benefit in the table, or file maintenance rows as `chore`. Pass the table
approval through so the user is not asked twice.

**S7-9. Security misses untrusted input and outbound excerpts.** (design-1-M8; Stage 5 R2)
Issue text from outside contributors reaches an agent that can file and push
under `sync: auto`, which is a prompt-injection path. Excerpts pushed in Notes
skip the egress gate. Fix: treat collected text as data only, and redact or
refuse token-shaped strings and personal data in excerpts. Gitea collection
needs a token the skill must not handle, so skip Gitea.

**S7-10. A tracker-issue row that matches an open story leaves the issue unresolved.** (design-1-M9)
Fix: report the issue and propose linking or closing it as a duplicate.

**S7-11. The Non-Goal "no other skill changes" is false.** (design-1-M1; Stage 5 R3)
The router (`skills/story-time/SKILL.md`), the README, `test/expected-skills.mjs`,
and `planning_install.rs:76` ("Installed 19") all change. Fix: name them.

**S7-12. One sort key and one feature/gap interleave are needed.** (style-1-3, design-1-N1; Stage 6 #4, #5)
Fix: define the key as distinct evidence references, merged. Say that the
walk fills gaps first and features take their reserved slots, then list
features after gaps in the table.

**S7-13. The `duplicate(ST-xxx)` value is never used, and Filing groups rows two ways.** (style-1-4)
Fix: drop `duplicate` from the table schema (keep it in counts), and group
Filing by `match` only.

**S7-14. `project-discovery` claims go beyond ST-019.** (style-1-7, design-1-N5)
Fix: say "does not estimate or schedule the backlog", and reject Alternative
B on that basis.

**S7-15. The open questions have no measure.** (style-1-9, design-1-N6)
Fix: count it a yes if at least 1 of the 2 feature rows is approved across the first 3 sweeps. Treat starvation as a problem if the same declined row leads 3 sessions in a row.

## Nits (grouped)

*   Walk step 3 renders as part of the last bullet (structural-1-4).
*   Finding codes and "Stage 5" are unlinked or undefined (structural-1-5, style-1-6).
*   Heading labels, horizontal rules, and NFR placement (structural-1-6, 1-7).
*   The table needs an example row with several evidence entries (structural-1-8).
*   The citation types differ between Goals and `ref` (style-N1). Feature terms drift (N2). `add to` and `add_to` both appear (N3). A Non-Goal is repeated (N4). "Covered" is undefined (N5, Stage 6 #9). "Asking for something" is vague (N6). "File 6 below" does not name the file (N7). Wording (N8). ST-028 and the business-value rule have no citations (N9, N10).
*   Merge semantics and story-ID references (design-N2). `story list --json` output is assumed (N3, Stage 6 #10). Out of Scope items are noise (N4). "Next batch" uses a stale read (N7).

## Not verified by reviewers, verified by the Elephant

*   `native/story-push/src/lib.rs:456` chooses update when `remote` is set (checked this session).
*   `story list --json` returns archived stories without `--status`. This was checked on the stale base in round 1 and needs a re-check on `origin/main`.

## Open owner decision carried from Stage 5

*   K1: a read-only native `story gaps --json` command for the deterministic steps. It reverses a Non-Goal.
