# PR #16 review fixes

## Acceptance gates

- [x] Story identity: Gitea synchronization uses ST-004 and issue #17. PR #15 retains Cartographer ST-002; concurrent PR #20 already uses ST-003. Update only the Gitea story's delivery references; preserve generic example and fixture IDs.
- [x] Readiness: every tracker retains local `story lint`. Gitea also runs the native Rust render dry run, which remains distinct from INVEST readiness. Verify the skill contract and actual malformed-story lint behavior in Rust integration tests.
- [x] Markdown: fenced headings do not export local Test Plan content or drop public Notes. Cover backticks, tildes, indented fences, longer delimiters, invalid closing delimiters, and an unclosed fence through the actual Rust CLI.
- [ ] Integration and delivery: run the full Node and Rust suites, warnings-denied lint and formatting, and combined PR #15/#16 configuration/story checks. Push without rewriting history. Merge only through current required checks and review policy; record an external review blocker rather than bypass it.

## Verification commands

Run from this PR's owned checkout. Build output belongs outside the source tree.

```sh
cargo test --offline --locked --manifest-path gitea-gateway/Cargo.toml --target-dir /tmp/story-time-pr16-fix-target
cargo fmt --manifest-path gitea-gateway/Cargo.toml --check
cargo clippy --offline --locked --manifest-path gitea-gateway/Cargo.toml --target-dir /tmp/story-time-pr16-fix-target --all-targets -- -D warnings
npm test
rustc --edition=2021 -Dwarnings --test scripts/story-policy.rs -o /tmp/pr16-policy-tests
/tmp/pr16-policy-tests
rustc --edition=2021 -Dwarnings --test test/story-policy-contract.rs -o /tmp/pr16-policy-contract
/tmp/pr16-policy-contract
```

## Language scope

The renderer fix and regression tests use native Rust. Skill instructions, stories, and evidence remain Markdown; configuration remains JSON. Existing Node planner, adapter, and regression tests are unchanged and outside conversion scope. Scoped Rust conversion gate is MET: the changed renderer and executable tests are Rust, and the native suite, formatting, and warnings-denied Clippy pass. Whole-repository conversion remains outside scope.

## Evidence

RED parent `5679a0dd2782e93f6d8bb9903e52e6366499f2aa` reproduces the three review regressions: 18 existing story-push tests pass and the three added tests fail. The source fix makes all 21 story-push tests pass, including actual CLI runs covering six fence variants in each direction. The native suite totals 29 passing tests (5 CLI, 3 HTTP, 21 story-push). Formatting and warnings-denied Clippy pass. The existing Node suite passes 408 tests; the standalone repository policy suites pass 12 unit tests and 7 contract tests.

The first provisional replacement ID, ST-003, was already claimed by concurrent PR #20 before publication. The published fix uses ST-004, preserving both PR #15's ST-002 and PR #20's ST-003. Generic example and fixture IDs remain unchanged.

Combined-branch verification, exact pushed heads, final CI, and the protected merge result remain to be recorded. Live Cartographer admission remains outside this fix and must not be marked done.
