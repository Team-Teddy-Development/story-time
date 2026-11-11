# Native push and setup integration

## Delivered source

PR [#20](https://github.com/Team-Teddy-Development/story-time/pull/20), ST-003 and issue #18, refreshed head `32982e4fbed602ccf09004ff2cb4ecd2acfe2cb6`, merged as `1344627053952ab4a6e1bc25ed18d6eb44ea4ea3`. All nine observed exact-head checks passed. The native sources did not change during the main refresh. The original independent Jev source review remains recorded at 0.90; the integration evidence citation was supported at 0.83.

PR [#24](https://github.com/Team-Teddy-Development/story-time/pull/24), ST-012 and issue #23, refreshed head `e4855af18f1c2062b026e8e2c8bcc3b0532a86da`, merged into main as `ff133d32f75fa5c8a8bf08856a7c1a0968ab1923`. All eleven observed exact-head check runs passed. Its help implementation and Rust regression source are byte-identical to the previously reviewed `94e5ac3d5aa9cfe6cb8440cd5642eca8de839243`. The independent Jev bounded-diff review returned 0.88.

Both merges used normal merge commits with exact-head guards. No administrator bypass, force push, human-approval fabrication, protection change, or merge into a dependency branch occurred. The accepted policy is `docs/adr/0004-solo-owner-review.md`. Required PRs, strict latest-base checks, resolved review threads, deletion protection, and non-fast-forward protection remain enabled.

## Story identity and ownership

This setup story was originally allocated as ST-005. Main independently acquired a different ST-005 at issue #21. Open PR #32 reserves ST-006 through ST-011. After a fresh fetch, all-ref history inspection, and open-PR/issue inspection, this same setup task was renamed to ST-012. Issue #23, original dates, all acceptance criteria, historical failures, and RED/GREEN commits remain unchanged. Jev selected the identity-preserving repair at 0.96 confidence. No task was invented to reset a failure or a score.

The current work touched neither PR #32 nor its story reservations. Main's explicit automatic-sync policy was preserved. The GitHub example's explicit `sync: off` remains an override, not a change to setup defaults.

## Real-path verification

Fresh local verification of the integrated source passed:

- Native Rust suite: 19 library, 9 executable, and 3 setup tests, 31 total, zero failures.
- Retained Node reference suite: 408 passed, zero failed, skipped, or canceled.
- Rust policy suite: 12 passed. Rust repository contract suite: 10 passed.
- Cargo format, strict Clippy, and release compilation passed.
- Installed setup contracts: 3 passed. Original installed empty-push regression: 3 passed.

The setup tests launch the real binary with Node absent and an empty environment, assert no files are written, extract the actual Markdown example, round-trip real init/config, and assert the bare-repository mutant is rejected before writes. The empty-push tests exercise the installed binary with credentials absent and an unreachable tracker address, assert the exact empty plan, and retain selector/sync-off controls.

The actual installed command also passed `env -i PATH=/nonexistent /home/ejernigan/.local/bin/story init --help`. Its usage names the complete `owner/repo` coordinate. Binary/release SHA256: `a3af1b6037ab59420fa15f4a466f79ad995c9395c634cf94b095f46c9a4f15e1`. Installed/source setup-skill SHA256: `cdd5591068537bf5549bda87fc624a9e98dfee678fde14943b2060fe7e8e8c19`.

GitHub reported PR #24 `merged: true`, base `main`, merge commit `ff133d32f75fa5c8a8bf08856a7c1a0968ab1923`. The run fetched that actual merge and fast-forwarded to it. Comparing its native crate and setup skill against tested head `e4855af18f1c2062b026e8e2c8bcc3b0532a86da` returned no differences.

## Rust conversion acceptance

| Scope | Language and role | Conversion status |
| --- | --- | --- |
| Native main, library, gateway, story parser, compensation | Rust, native push and exact setup-help execution | MET |
| Native executable, setup, and empty-push regression tests | Rust, actual binary and negative-path verification | MET |
| Setup skill, JSON examples, stories, and evidence | Documentation, configuration, and data, not executable implementation | Not a conversion target |
| Retained non-help init/config and Node reference suite | Unchanged dependency and historical reference; outside the approved push/help conversion scope | Recorded out of scope |

The bounded conversion gate is MET. This is not a claim that the whole repository uses Rust. No Rust wrapper is represented as conversion of retained non-help logic.

## Errors and attestation limits

`es_a8170328dbb7` and `es_fda112102cf8` have committed source corrections, RED/GREEN Rust tests, installed-artifact verification, remote CI, and main integration. The sentinel resolver still cannot attest them because it requires the later regression test path in the original failing command. Their historical session-task clearances are not successful tested-fix attestations.

`es_3cc67e4ed421` was an exact-edit argument typo. The failed argument contained an extra leading space. The corrected edit succeeded. Exact original/corrected call IDs, patch output, and the then-current installed-file comparison are retained in the external release evidence. This is an operational correction, not a product source fix. The subsequent canonical skill installation has its own new checksum above and does not rewrite that historical evidence.

The four docs-relative Cargo gate failures retain their original history and corrected `CWD: ..` evidence. G6 was previously genuinely unmet and is now satisfied by the two actual protected merges, not by its follow-up task clearance.

During integration, `gh pr edit 24` in installed gh 2.45.0 failed because its GraphQL request still queries deprecated `repository.pullRequest.projectCards`. The prior Git push had succeeded; it was not repeated. The same authorized PR base/body update succeeded through `gh api --method PATCH repos/Team-Teddy-Development/story-time/pulls/24`, and API readback confirmed base main and the exact head. This corrects the invocation, not the external gh implementation. Upgrading that external CLI is outside this source-fix scope. The pending-check exit code 8 from `gh pr checks` represented running checks, not a failing test; all checks later completed successfully.

## Recovery

Verified Git bundles and pre-install binary/skill copies are retained under `/home/ejernigan/.pi/agent/release-evidence/story-setup-help-schema-20261111/`. Both source PRs preserve contributor commits. A source rollback requires a reviewed revert PR; do not reset protected main. Local installation rollback uses the retained binary and skill copies. The run-owned branches retain committed evidence after their worktrees are released.
