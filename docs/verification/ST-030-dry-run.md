# ST-030 dry run: gap-sweep on story-time

*   **Date:** 2026-11-14
*   **Base:** `origin/main` `af8dc9b`, plus branch commits up to `45ef730`
*   **Mode:** Read-only. The sweep followed `skills/gap-sweep/SKILL.md` step by step. No story was filed and no tracker item was written. `git status` after the sweep showed no change under `backlog/` or `.story-time/`.

## Collector status

| Collector | Status | Found | Kept after walk | Notes |
|---|---|---|---|---|
| Code markers | ok | 7 | 0 | All 7 are string literals or test strings (`create-story.mjs:4,14,15`, `invest.test.mjs:124-125`, `use-cases.test.mjs:26,28`), not comments. The evidence check dropped them. |
| Skipped tests | ok | 0 | 0 | No test is marked skip or ignore. The anchored patterns avoided the `Iterator::skip` and `process.exit` false hits from revision 2. |
| Doc promises | ok | 5 | 0 | `story as`, `story covers`, `story status`, `story takes`, `story to` are prose lines that start with "story", not commands. Dropped on evidence. |
| Story follow-ups | ok | 29 sections | 3 | 28 Out of Scope sections and 1 Findings section. Most Out of Scope items are prohibitions, not work, and the collector skips them. |
| GitHub issues | ok | 7 | 2 | `gh issue list --state open --limit 200 --search "-label:story"` returned 7, not truncated. #26, #50, #56, #57, and #61 are story remotes and were dropped. |
| Feature anchors | ok | 1 | 1 | |

## Proposed table

| # | Kind | Proposed story | Persona, benefit | Evidence | Match |
|---|---|---|---|---|---|
| 1 | defect | Keep an empty `depends_on` list empty | maintainer, `depends_on=[]` clears dependencies | `#12` | new, will overwrite the title and body of #12 |
| 2 | follow_up | Map the remaining lifecycle skills from ST-019 | maintainer, the lifecycle research turns into ordered work | `ST-019 §Findings`, `docs/spikes/ST-019-atlassian-lifecycle.md:124` | new |
| 3 | follow_up | Handle a closed stdin pipe in the push test helper | maintainer, the Node suite stops failing at random | `#11`, `skills/story-time/scripts/test/cli-push.test.mjs:32` | new, will overwrite the title and body of #11 |
| 4 | follow_up | Reconcile an ambiguous tracker create automatically | maintainer, a timed-out create does not leave a duplicate issue | `ST-004 §Out of Scope` | new |
| 5 | follow_up | Lock concurrent label edits on the tracker | maintainer, two pushes do not overwrite each other's labels | `ST-004 §Out of Scope` | new |
| 6 | feature | Let native `story` find its own checkout's CLI | maintainer working in a checkout, commands use the code under test instead of an installed release | `AGENTS.md:7-8`, `native/story-push/src/main.rs:53-62` | new |

Counts: 6 kept (5 gaps, 1 feature), 12 dropped on evidence, 5 dropped as covered (issues with a story `remote`), 0 merged, 0 not yet walked.

## Evidence checks run for the kept rows

*   **#12 reproduces on `origin/main`.** In a temporary backlog, `story set ST-001 'depends_on=[]'` wrote `depends_on: ["[]"]`. ST-028's review had not reproduced it (`docs/planning/open-issues-2026-11-13.md:37`).
*   **#11's helper is unchanged.** `cli-push.test.mjs:32` still calls `child.stdin.end(input ?? '')` with no error handler. ST-027 covers the same helper but a different symptom (empty JSON), so #11 is not covered by it.
*   **Row 6.** The native binary dispatches to `STORY_TIME_LEGACY_CLI`, or else to `~/.config/story-time/runtime.json` (`main.rs:53-58`). During this session the runtime file pointed at an installed release (`cb9f9ee`), so running `story` in a checkout used that release's CLI unless the variable was set by hand. AGENTS.md asks every contributor to set it.
*   **Duplicates.** Each row was compared against all 30 stories, archived included. None is covered. Row 1 is related to ST-018, which implemented only `refinement` and `sprint-planning`.

## What the dry run says about the skill

*   **Useful rows.** I judge rows 1, 2, 3, and 6 useful. They are real, unfiled, and checkable. Rows 4 and 5 are deliberate deferrals that ST-004 put out of scope, and a maintainer may decline them. Feature rows: 1 shown, which I judge useful.
*   **Order defect, found and fixed in this PR.** On the first pass, the reproduced bug #12 ranked last among gaps. It has no `bug` label, so the skill classified it as `follow_up`, and the two speculative deferrals ranked above it. The cause was that kind came only from the label, and the walk never re-ranked. `SKILL.md` now sets the kind to `defect` when the evidence check reproduces a defect, and it orders kept gaps by kind again after the walk. The table above uses the fixed rule.
*   **Noise handled.** The evidence check removed all 17 code, test, and doc false positives. Revision 2's unanchored patterns would have put them at the top as defects.
*   **Starvation.** It did not occur, because the 6 rows fit under the cap of 10.
