# ST-017 acceptance gates

## Scope and throughput checkpoint

One shared Rust URL validator, existing Rust integration tests, and Gitea documentation. Two execution stages: preserve failing regression tests, then correct the validator and verify the installed native CLI. Do not parallelize shared-file implementation. Existing legacy commands and other tracker implementations remain outside this repair.

## G1: Regression detects the defect

CHECK: Run the new private-address HTTP and story-push tests at the test-only commit 8a0a7596217a12c3caefa58340771241c5a5271a.
EXPECT: Both new acceptance tests fail against the unchanged loopback-only validator.

## G2: URL policy and regressions pass

CHECK: `cargo test --offline --manifest-path gitea-gateway/Cargo.toml`; format check; Clippy with warnings denied; existing npm and Rust repository-policy checks.
EXPECT: All tests pass with no new warnings. Public HTTP, hostname HTTP, userinfo, query, fragment, and redirect protections remain enforced.

## G3: Original feature path works

CHECK: Install the corrected Rust gateway, then run `gitea-gateway story-push --root /tmp/snt-story-setup-20261112 --dry-run backlog/SNT-001.md` without GITEA_TOKEN.
EXPECT: Exit 0, status ok, action create for SNT-001, no local settings or story changes. A dry run does not prove authentication or live Gitea compatibility.

## G4: Rust conversion gate

CHECK: Inspect the active repair chain and committed diff.
EXPECT: Validator, gateway dispatch, story planning, and tests are Rust. JSON and Markdown are configuration/data/docs. No new non-Rust execution or wrapper implementation enters this path. Legacy Node commands and unrelated repository executables remain out of scope.

## G5: Delivery and report closure

CHECK: Review the source diff, land green CI through a story-linked PR, preserve exact source/test refs, and resolve es_0b91f1afe186 using tested-fix evidence.
EXPECT: Merged PR, installed CLI proof, exact fix reference, and session-owned worktrees removed. Record any concrete blocker instead of claiming success.

## Decision trail

- Jev selected RFC1918 IPv4 and IPv6 unique-local literals plus existing loopback HTTP. Public HTTP remains blocked. Plaintext HTTP is only appropriate on a trusted LAN.
- The implementation retains Url and IpAddr as its data shape and changes one existing predicate. No new API, DNS lookup, fallback, or transport retry is introduced.
- Test-only commit 8a0a759 preserves the original failure before the source correction.
