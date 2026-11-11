# Repository recovery verification

Status: repository checks pass. Full onboarding is **NOT DONE**. The original live acceptance criterion remains unchecked in ST-002.

## Source and story reconciliation

The run began from preparation commit `3efcee71f0fd600c4eef17f87063c733e7672861`. Before its push, another session advanced PR #15 to `fa0f39f8b47d9ed579de9c6b6d89aea135c3fb68` and merged main's GitHub dogfooding policy. This run merged that exact head without force and retained both stories.

`git diff fa0f39f8b47d9ed579de9c6b6d89aea135c3fb68 --exit-code -- backlog/ST-001.md .story-time/settings.json` returned zero. ST-001 and GitHub tracking with sync ask are unchanged from that peer head. Cartographer onboarding follows its existing new ID, ST-002. The original incident names and RED/GREEN commits remain preserved.

The actual CLI reports ST-002 `READY (0 errors, 0 warnings)` after explicitly proposed size 3 and baseline WSJF inputs of 1. Lifecycle remains refined because live admission inputs are blocked. GitHub issue #19 is open, titled `Manage story-time through Cartographer`, with `story`, `size:3`, and `value:1` labels. Its URL was verified after the scoped create operation.

## Local checks

| Command or artifact | Observed result |
| --- | --- |
| `npm test` | 408 passed, 0 failed, 0 skipped |
| `rustc --edition=2021 -D warnings --test scripts/story-policy.rs -o /tmp/story-policy-tests && /tmp/story-policy-tests` | 12 passed, 0 failed |
| `rustc --edition=2021 -D warnings --test test/story-policy-contract.rs -o /tmp/story-policy-contract && /tmp/story-policy-contract` | 8 passed, 0 failed |
| `cargo test --offline --locked --manifest-path tools/cartographer-preflight/Cargo.toml` | 12 unit and 2 executable protocol tests passed |
| `rustc --edition=2021 -D warnings --test tests/cartographer_setup.rs -o /tmp/story-time-cartographer-setup-tests && /tmp/story-time-cartographer-setup-tests --test-threads=1` | 2 real CLI tests passed |
| `cargo fmt --manifest-path tools/cartographer-preflight/Cargo.toml --check` | Passed |
| `cargo clippy --offline --locked --manifest-path tools/cartographer-preflight/Cargo.toml --all-targets -- -D warnings` | Passed |

Total: 408 Node tests and 36 Rust tests. The test binaries capture and assert expected error paths. No test or lint rule is disabled.

The predicate mutation experiment initially exposed a missing per-clause assertion. After strengthening it, replacing `desired > 0` with `desired == 0` caused the expected exit 101 in `v2_scaled_to_zero_is_a_blocker`. Restoring the actual predicate returned the full suite to green.

The unlazy gate checker re-ran G1 and G4 against the reconciled tree. It recorded five met gates and one unmet gate, G5, for live authenticated portfolio and worker execution. Exit 1 was expected and asserted; it was not changed to a pass. The real stdio MCP response is [live-preflight.jsonl](live-preflight.jsonl), which reports measured production blockers as data.

## Required GitHub integration handoff

The repository's server-side rules require one code-owner/team review, a last-push approval, resolved review threads, and successful required checks. This run does not alter those rules or use an administrator bypass. Jev's bounded implementation review is recorded in GATES.md, but Jev cannot supply a separate GitHub account.

The authorized code-owner reviewer must inspect the current PR head and record the review through GitHub. If that reviewer has the necessary account access, use the following commands only after the review is complete:

```sh
REPO=Team-Teddy-Development/story-time
PR=15
HEAD_SHA=$(gh pr view "$PR" --repo "$REPO" --json headRefOid --jq .headRefOid)
gh pr checks "$PR" --repo "$REPO" --required
# After independently reviewing this exact HEAD_SHA with the required code-owner account:
gh pr review "$PR" --repo "$REPO" --approve --body "Reviewed exact head $HEAD_SHA; repository evidence in docs/cartographer/repository-verification.md. Live admission remains incomplete."
gh pr merge "$PR" --repo "$REPO" --merge --match-head-commit "$HEAD_SHA"
```

If the head changes, review that new head and recheck CI before approval. Do not approve an unseen commit. The matched-head merge fails rather than merge a different revision.

Record the final head, CI URLs, approving review, and merge commit in this file after integration. Until then, the repository work is preserved in PR #15, not claimed as merged.

## Production handoff

Cartographer intake [PR #194 / CRT-052](http://192.168.1.56:30003/team-teddy/cartographer/pulls/194) records the backend planning need. Existing backend PRs #139, #132, and #135 keep their current execution owners. The feeder run makes no production writes and does not merge those changes.

After a qualified admission path exists, the operator completes the private signing and publication steps in README.md. Then verify the authenticated owner listing, UI portfolio, source authorization, and a permitted bounded worker task. Record sanitized evidence in admission-evidence.md. Until those checks pass, ST-002 stays incomplete.
