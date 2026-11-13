# ST-022 verification and acceptance

## Identity and outcome

Implementation commit: `44f7789c226b3b6d3f4f1526a5a85e261121d2ba`.
Corrected installation-contract head: `7cff569af86ce3492bb07b2144d74a4f567298ce`.
PR: https://github.com/Team-Teddy-Development/story-time/pull/59.
Story: https://github.com/Team-Teddy-Development/story-time/issues/54.

This receipt verifies read-only preview, not adoption apply or recovery. No lifecycle metadata, receipt, original configuration, backlog record, or tracker record is changed by the preview command. Publishing this story and PR is separate authorized tracker activity.

## Acceptance matrix

| Criterion | Observed evidence | Verdict |
| --- | --- | --- |
| AC1 | `current_and_legacy_exact_inventory_and_deterministic_plan` runs actual native processes with Node unavailable. Current/legacy paths, source0/target1, exact config/story hashes, sorted membership, ordered absent preimages and proposed content hashes pass. Complete project snapshots remain unchanged, and repeated stdout matches. Installed CLI on this repository reports19 records and only the two additive paths. | PASS |
| AC2 | Same test retains CRLF, non-ASCII text, custom fields/sections, split/done/archived records, IDs and remote identities. Goal/capacity/epic_links/history each has null value and explicit reason. Original bytes remain authoritative; raw prose/config credentials are not printed. | PASS |
| AC3 | Process tests cover malformed/nested records, duplicate local/remote IDs, unknown metadata/receipt versions, missing or conflicting settings, tracker-only input, unsafe components/symlinks, remote URL conflicts and preservation divergence. Each conflict asserts exit2, empty writes, diagnostic code and recovery, and unchanged snapshots. Config-conflict mutation is rejected. F1 URL mismatch has observed RED/GREEN and positive controls. | PASS |
| AC4 | `disposable_skill_install_routes_before_gitea_and_native_commands_need_no_node` runs the real unchanged skill installer, reads installed setup routing before Gitea, checks the250-line contract, and runs new/adopt/upgrade paths with Node absent. Native init/setup regressions remain unchanged and pass. | PASS |

## Commands and results

Run from the repository root:

```sh
cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST022-native-target
cargo fmt --manifest-path native/story-push/Cargo.toml --check
cargo clippy --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST022-native-target --all-targets -- -D warnings
npm test
rustc --edition=2021 --test scripts/story-policy.rs -o /tmp/ST022-policy-tests
/tmp/ST022-policy-tests
rustc --edition=2021 --test test/story-policy-contract.rs -o /tmp/ST022-policy-contract
/tmp/ST022-policy-contract
git diff --check
```

Results:56 native tests,408 Node tests,12 Rust policy tests and11 policy-contract tests pass; no failed or skipped tests. Format and strict all-target Clippy pass. G1-G4 automatic revalidation records numeric zero exits and matched expectations in `GATES.md`.

The independent negative control copies source outside the repository and changes `current_present && legacy_present` to `current_present && !legacy_present`. The targeted configuration-conflict process test rejects it with exit101. Production source remains unchanged.

Actual installation and preview:

```sh
cargo install --offline --locked --path native/story-push --target-dir /tmp/ST022-native-target --root /tmp/ST022-cli-install --debug
/tmp/ST022-cli-install/bin/story adopt preview > /tmp/ST022-installed-preview.json
/tmp/ST022-cli-install/bin/story adopt preview > /tmp/ST022-installed-preview-recheck.json
cmp /tmp/ST022-installed-preview.json /tmp/ST022-installed-preview-recheck.json
cmp /tmp/ST022-cli-install/bin/story /tmp/ST022-native-target/debug/story
test ! -e .story-time/lifecycle.json
test ! -e .story-time/adoption-receipt.json
```

Observed exit0, `status: preview`, source0/target1,19 backlog files, current settings SHA256 `3cd1468b016d0c3ce8d01f9ac5b457d0d16ca546b06e72eaba952552912de245`, github/auto unchanged, no diagnostics. Only lifecycle.json and adoption-receipt.json are proposed. Neither exists afterward. Both comparisons pass; Git status is clean. Real-artifact proof is recorded with `prove_it_works`.

## Review and corrections

Independent source review found F1: remote7 with `/issues/8/` or nested `/issues/8/comments` could evade the old last-segment comparison. Added process fixtures reproduce exit101 before correction. Source now requires a numeric issue suffix under the configured repository scope and compares its ID with the remote ID. Negative8/,8/comments,7/comments and positive7,7/ controls pass. Typed Jev correction review: supported83%, supported verdict confidence.73.

Strict Clippy rejected `filter(...).next_back()`. Replaced it with `rfind(...)`; no lint suppression or threshold change.

After restoring the existing Gitea JSON example, the exact-head Node suite found254 setup lines. Added an installed-path Rust length assertion, observed RED101, then retained the example as inline JSON. Source now has249 lines/250 split segments. Rust installation test and full Node suite pass. No test was skipped or weakened.

Criterion-specific typed Jev review at `7cff569`: AC1 supported73%, verdict confidence.70; AC2 supported85%, confidence.96; AC3 supported48%, confidence.10; AC4 supported82%, confidence.88. All choice verdicts are supported. These are probabilistic assessments, not substitutes for the process evidence; the low AC3 confidence is retained without relabeling it as high confidence.

The earlier implementation-only completeness assessment67/100, done probability.33, explicitly preceded CI, acceptance and cleanup. It is preserved as history, not a final completion assessment.

## Rust scope audit

| Active implementation | Language and role | Conversion status |
| --- | --- | --- |
| adoption.rs | Rust inventory, validation, hashes and report | MET |
| native main/lib dispatch | Rust native route before legacy fallback | MET |
| adoption_preview_contract.rs and adoption_preview_install.rs | Rust actual-process and installation verification | MET |
| Existing flat parser and locked ring SHA256 | Existing Rust validation and third-party hashing | Reused, no wrapper around non-Rust logic |
| Retained Node CLI, skill installer and Node regressions | Unchanged compatibility interfaces | Outside conversion scope; retained |

Markdown, JSON and TSV are documentation/configuration/fixtures, not new executable implementations. No new or replacement non-Rust implementation exists in scope. Signing keys and Cartographer admission remain untouched.

## Delivery and recovery

PR59 at `7cff569` has9/9 successful exact-head GitHub checks: test20/test22, story-link, story-ids, story-policy-tests, native-push, cartographer-preflight and two Native Gitea Rust contexts. GraphQL shows no review threads. The active branch rules require the five test/story contexts, strict freshness, a PR and resolved review threads; external approval count is zero under ADR0004. No administrator bypass or policy change is used.

Legacy branch-protection lookup returned404 because this repository uses rulesets. The corrected `rules/branches/main` lookup succeeds and identifies rulesets26702417 and27056058. Failed gate invocations used the ledger directory instead of the repository root; corrected `--root . --cwd .` revalidation passes. A reverify approval miss caused by the background PATH difference is retained separately from test outcomes. These are invocation recoveries, not product source repairs.

Preview is an observation, not a repository lock. ST-023 must lock, recheck freshness and membership, back up, journal, apply and recover before writing. Do not run apply from this slice. A regression can be reversed with a normal reviewed revert of this PR; no data migration rollback is needed because this change writes no project data.

`GATES.md` G5 remains pending until final delivery and owned-resource release. The terminal receipt and final ledger are retained outside the worktree at `/home/ejernigan/.local/state/story-time/ST022-adoption-preview/docs/`, so cleanup can be recorded after removal without changing a shared anchor. No pending cleanup is falsely ticked in this pre-integration snapshot.
