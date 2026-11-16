# Stage 9 — Implementation Readiness Report (round 1)

*   **Target:** `docs/designs/merge-sync-design.md`, revision 3.2, DetailedRange lines 593–808
*   **Reviewers:** `egm-implementation-readiness` (FAIL), `egm-codebase-details` (FAIL), `egm-style` (approve with changes)
*   **Verdict:** FAIL. After merging duplicates: 5 Blockers, 14 Majors, and 14 Nits.

## Blockers

| ID | Source | Issue | Fix |
| --- | --- | --- | --- |
| R9-B1 | IR B1, CD B2 | Adding `check-pr` and `sync-merge` to `NATIVE_COMMANDS` breaks `tests/doc_native_lists.rs` and `tests/node_unknown_command.rs`. | Add as Modify: `AGENTS.md`, `README.md`, `skills/story-time/SKILL.md` (two lists and the command table), and `skills/story-time/scripts/story.mjs` (`NATIVE_ONLY`). Give the exact text for each. |
| R9-B2 | IR B1, CD B3 | `help.rs` holds no help text. It uses `include_str!` on `skills/story-time/scripts/usage.txt`. | Item 6 edits `usage.txt` instead. Drop `help.rs` and `help_contract.rs` from the plan. Extend `help_lists_native.rs` with the new verb prefixes. |
| R9-B3 | CD B1 | Removing `story-link` and `references()` from `story-policy.rs` breaks `validate_links`, `link`, 7 unit tests, and 4 tests in `test/story-policy-contract.rs`. That contract runs in the required check `story-policy-tests`. | Name each removed function and test. Add `test/story-policy-contract.rs` to the plan, with its meta-test update. |
| R9-B4 | IR B2, CD B4 | `Cli::run` (`gateway.rs:127-135`) drops stderr and the HTTP status, so the plan cannot tell retryable failures from permanent ones. | Define the error type and the mapping rules (HTTP 404/410, "Could not resolve" → permanent; 5xx, rate limit, timeout, spawn failure → retryable). Keep the test that no secret is echoed. |
| R9-B5 | IR B3, CD B5 | `download-artifact` writes into the checkout, so the clean-tree step fails. The binary also loses its executable bit. | Download to `${{ runner.temp }}/story-bin`, run `chmod +x`, and call the binary by absolute path. Use named artifacts. |

## Majors

| ID | Source | Issue |
| --- | --- | --- |
| R9-M1 | IR M1, CD M1/M2 | `sync-merge` inputs are not defined: workflow name, default branch, repository, HEAD. The `gh run list` call has no `--workflow`, `--branch`, `--repo`, or `--limit 1`, so it can anchor on another workflow's run. |
| R9-M2 | IR M2, CD M13 | The plan has no ordered phases with green exit checks. The verification plan misses `rustc --test` for both policy files and `npm test`. |
| R9-M3 | IR M3, CD M5 | Trait and struct signatures are not pinned. `MergeTracker` duplicates the existing `Tracker` (`lib.rs:123`). `closing_targets` cannot filter by repository. |
| R9-M4 | IR M4, CD M11 | `check-pr` and `sync-merge` output is undefined: the JSON schema, the message text, the incident reminder text, input precedence, an invalid mode value, and the exit code when the label API fails. |
| R9-M5 | IR M5, CD M4 | The card parser and predicates are not named (`lenient_story::parse_noting_drops`, `lib.rs:209-223`). Bare `#N` handling in `sync-merge` and the plain-text report are not defined. |
| R9-M6 | CD M3 | `comment_once` has no list command or author filter. Anyone can suppress a merge comment by posting the marker first. `mcp/comments.rs` already trusts a marker only from the bot user. |
| R9-M7 | IR M6, CD M8 | The `story-link.yml` shape is not given. The job ID must stay `story-link` (required by the ruleset), and the rustc step's fate is not stated. |
| R9-M8 | CD M9 | Cache key and install command: `usage.txt` is not in the key, and `cargo install --git` needs the package name, `story-time-native --bin story`. |
| R9-M9 | IR M7, CD M10, ST M1 | The dispatch `all` input contradicts the step that always runs `--all`. There is no cron expression. |
| R9-M10 | IR M8, CD M6/M7 | No harness is given for the fake `gh`. `stub_cli` is private, cited at the wrong line, and only answers `issue` commands. Name `push_lenient_contract.rs`'s `install_stub_gh` pattern. One test expects a state comment where the design gives a merge comment. |
| R9-M11 | IR M9, CD N4 | The consumer YAML templates are not written out. Playbook §5 and the §3 table need `story-sync.yml`. |
| R9-M12 | IR M10, CD M12 | Rule 2 edge cases are not decided: zero criteria, plain bullets, a spike with both sections. The ID grammar (3+ digits, `#0`) and the comma-separated form have no tests. |
| R9-M13 | ST M2/M4/M5 | Wording: "each amended criterion" should be "each criterion"; "link failure" is undefined; "AC3: drift is fixed" cannot be tested. |
| R9-M14 | ST M3 | Item 13's status is unclear: it says done, but also says to publish. |

## Nits

*   **Style (ST N6–N13):**
    *   a line anchor;
    *   "directly after";
    *   test inputs with no expected result;
    *   "true merge";
    *   where the skip reasons appear;
    *   the "template" antecedent;
    *   "within one minute";
    *   the attribution line.
*   **Implementation readiness (IR N1–N6):**
    *   the help assertion;
    *   the cache-key glob;
    *   `runs-on` and action versions;
    *   unknown flags exit 2;
    *   `pub mod` or `mod`;
    *   preconditions for the end-to-end test.
*   **Codebase details (CD N1–N6):**
    *   the ST-151 AC4 not-closed assertion;
    *   pin the incident reminder text;
    *   `sync` defaults to `off`;
    *   `docs/native-story-push.md`;
    *   ST-152 Notes cite D17, which was removed;
    *   module visibility.

The codebase reviewer confirmed these line citations as correct: `gateway.rs:400`, `:511`, `:912`, `:886-901`; `story.rs:144` and `:236`; `lib.rs:224-232` and `:316`; `mcp/comments.rs:11`; and `playbook.md:147-190`.

## Gate and resolution (revision 3.3)

Jev chose to resolve the findings and run one re-audit (confidence 0.93). Revision 3.3 rewrites the Detailed Implementation Plan:

*   **Phases.** Five ordered phases, each with green exit checks (R9-M2).
*   **Missing files.** Item 6 lists every file the test suites check against the command table: `usage.txt`, `story.mjs`, `AGENTS.md`, `README.md`, `SKILL.md`, and `help_lists_native.rs`. `help.rs` is not changed (R9-B1, R9-B2).
*   **Policy suites.** Item 9 names each removed function and test, and the contract-suite edits. The three link tests move to item 7 (R9-B3).
*   **Error kinds.** Item 4 adds `TrackerError` and `Cli::run_classified`. The table of rules keeps stderr out of every message, and auth failures are retryable (R9-B4).
*   **Workflows.** Items 10 and 11 give the full YAML. The artifact is downloaded to `runner.temp`, then `chmod +x`. The cache key includes `usage.txt` (R9-B5, M7, M8, M9).
*   **Contracts.** Inputs, signatures, output text, JSON shapes, exit codes, card parsing, and skip reasons are pinned (R9-M1, M3, M4, M5). `MergeTracker` extends `Tracker`.
*   **Comments.** `comment_once` trusts a marker only from `github-actions[bot]` (R9-M6).
*   **Tests.** A shared `tests/common` stub with response fixtures (R9-M10). Consumer templates are given as substitutions on the full files (R9-M11). Rule 2 edge cases and ID grammar are decided and tested (R9-M12).
*   **Wording.** The style fixes (R9-M13). Item 13 states that nothing remains (R9-M14). The ST-152 note on D17 is fixed.

The DetailedRange is now lines 593–1129.

## Re-audit (revision 3.3) and final resolution (revision 3.4)

*   **Re-audit results:** All 5 round 1 Blockers are resolved. Each reviewer still found Majors, but none found a Blocker:
    *   implementation readiness: 6 Majors and 10 Nits;
    *   codebase details: 11 Majors and 9 Nits;
    *   style: 2 Majors and 9 Nits.
*   **Gate:** The re-audit cap is reached. Jev chose to apply the fixes and conclude, with no further audit (confidence 0.85).

Fixes applied in revision 3.4:

*   **`check-pr` issue lookups.** `GhIssues` uses `REPO`, so it works for every tracker. A `Cli::gh()` constructor is added. Rule 4 passes targets that are not story issues. "Accepted reference" is defined. New tests cover `tracker: files` and AC5 for issues that are not stories.
*   **Output.** The `run` signature is pinned, along with the streams: findings go to stderr through `eprintln!`, and stdout carries only the PASS line or the JSON. Every finding is reported. The message table now matches the CUJ 1, CUJ 5, and D9 text, with at most one rule 2 finding per card. The warning count wording is fixed.
*   **Gateway.**
    *   `update_classified` requests the issue `url` and detects a transfer.
    *   `ensure_github_labels` and `github_state` return `TrackerError`.
    *   `update` keeps its behavior through an adapter.
    *   A marker counts only when the numeric user ID is 41898282.
*   **`sync-merge`.**
    *   `--all` lists cards with `git ls-tree`.
    *   Card stems are checked with `valid_id`.
    *   A run is refused when `GITHUB_REF_NAME` is not the default branch, and the `sync` job has a default-branch condition.
    *   The comment and marker use the full SHA.
    *   `git log` uses `--first-parent`.
    *   The step 1 skip line and the no-PR line are defined.
    *   A failed `git` call is retryable.
*   **Workflows.** The build job has `contents: read`, so it can check out a private repository. It still holds no write token. The spec text is updated to match.
*   **Test harness.** The stub's signature, prefix rule, stdin drain, and `dead_code` allowance are pinned. `push_lenient_contract.rs` writes a `responses.tsv` with its three current answers.
*   **Policy files.**
    *   `story-policy.rs` line 1 drops `Path`.
    *   The ID test range is :311-327.
    *   The nested-path assertion is kept as its own test.
    *   The `usage.txt` column and insert point are given, and so are the exact rows for the `SKILL.md` command table.
*   **Wording and cards.**
    *   The E2E preconditions now say `sync: auto`.
    *   The AC3 test checks title, labels, and body.
    *   Item 13 is marked "No change; already published".
    *   The checker-change caveat names the required review as its control.
    *   The style wording nits are fixed.
    *   The ST-152 size note now matches size 5.
*   **Not applied (style nit 8):** the bold-label punctuation is not unified.

Result: the review is concluded, with no open Blocker. Revision 3.4 has not been audited again.
