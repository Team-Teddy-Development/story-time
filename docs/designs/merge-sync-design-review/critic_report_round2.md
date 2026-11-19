# Stage 7 — Critic Review Report (round 2, re-audit)

*   **Target:** `docs/designs/merge-sync-design.md`, revision 3, lines 1–533
*   **Reviewers:** `egm-design` (completed), `egm-structural` (completed), `egm-style` (completed)
*   **Verdict:** Not clean. 0 Blockers. 11 Majors: 7 from the design reviewer and 4 from the style reviewer. 32 Nits. The structural reviewer passed.
*   **Re-audit cap:** reached (1 per stage).

## Gate

Jev first chose to accept the round 2 findings as trade-offs (confidence 0.28; probability 0.52 against 0.47). Four facts were then added:

- The fixes are text edits.
- Design M3 is a token exposure in a template that consumer repositories copy.
- M1 and M6 break ST-151 AC1 and AC2.
- Stage 8 re-checks the whole document.

With those facts, Jev chose to fix and advance (confidence 0.52). The fixes went into revision 3.1, and there was no third Stage 7 audit.

## Majors and resolution

| ID | Finding | Resolution in 3.1 |
| --- | --- | --- |
| D-M1 | An `--all` run advances the "last success" range and drops comments. | The range uses only successful `push` runs (`--event push`). `--all` shares the concurrency group. |
| D-M2 | A permanent failure blocks every later run, and the range grows without limit. | Failures are retryable or permanent. A permanent failure is reported, and the range advances. A range over 200 commits takes the `--all` path. Added a Risks row. |
| D-M3 | `persist-credentials` lets the build step read the write token. | The build is a separate job with `permissions: {}`. It uses `cargo install --locked` and passes the binary as an artifact. The sync job checks out with `persist-credentials: false`. |
| D-M4 | The new remote-equality rule is a hard failure, which bypasses the warn-first roll-out. | Moved it into rule 3 (warn or fail). Rule 1 is today's behavior only. |
| D-M5 | A keyword closes the issue of an unnamed story, and nothing reopens it. | Step 4 selects the card whose `remote` is a keyword target in a kept PR. |
| D-M6 | The AC2 state comment is lost on a rerun. | The comment is keyed on the card's status change in the range and the marker check, not on the result of `update`. |
| D-M7 | The per-commit PR lookup cost is not counted. | The range is walked with `--first-parent`. Cost counts one lookup per first-parent commit. |
| S-M1 | The keyword guarantee is overstated for `warn` mode. | Recalibrated in the Overview. |
| S-M2 | "Close only when" is false. | Changed to "Within story-time". The ST-151 card is fixed too. |
| S-M3 | "Every skip names the SHA" is overstated. | Recalibrated to the AC6 cases. |
| S-M4 | The switch to `fail` has no trigger or owner. | D18: the repository owner switches after 7 days with 0 warnings (Jev 0.22, weak). Added two success measures with an owner and a query method. |

## Nits applied

- **Rule 4 and checks:** rule 4's keyword grammar now matches GitHub (optional colon, `owner/repo#N`, and URLs). Both verbs read `backlogDir`. The design states how a conflicted PR with no merge ref behaves.
- **Skip reasons:** deleted and `pending_create` cards get their own skip reasons. The first-run comment gap is stated.
- **Header:** added the Revision field and ST-154 (#350).
- **Decision references:** the AC amendment notes now match the cards. D8 is split into D8 and D8b. D17 is relabeled as a constraint.
- **Wording:**
  - The Decisions preamble is tightened.
  - Alternative I uses "Reason for Deferral".
  - Alternatives G, H, and J lose their subjective words, and H's estimate is labeled.
  - The Article VIII text is quoted.
  - "Neither verb writes a card file" replaces "no automation".
  - The playbook-template sentence is recalibrated.
- **Not applied:**
  - Full paths for the second citation of a file (structural N7).
  - Cross-repository URLs (structural N8).
  - Splitting step 4 into shorter sentences was partly done.
  - The forward references from Goals to D-numbers now say "See Decisions".
