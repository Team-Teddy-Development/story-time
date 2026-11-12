# ST-016 recovery gates

## Workflow
Frame: six recorded fingerprints represent three causes, not six independent product defects.
Design: repair native command help; prove returned-ID selection; preserve changed-title contract evidence; submit every fingerprint to the resolver.
Execution: use Rust regression tests, commit the test-only baseline, implement native help, and verify the real CLI.
Audit: preserve each tool verdict and distinguish source fixes, test-contract updates, invocation recovery, and unsupported resolution types.
Verification: land the scoped PRs on green, then record exact commit and CI evidence.

## G1: Native help is read-only without the legacy runtime
CHECK: CARGO_TARGET_DIR=/tmp/story-title-prefix-target cargo test --manifest-path native/story-push/Cargo.toml --test recovery_contract new_help_runs_without_legacy_runtime_or_writes
EXPECT: test result: ok. 1 passed; 0 failed
Status: UNMET.

## G2: Creation returns the ID used by native push
CHECK: CARGO_TARGET_DIR=/tmp/story-title-prefix-target cargo test --manifest-path native/story-push/Cargo.toml --test recovery_contract create_then_push_uses_the_returned_id_instead_of_guessing
EXPECT: test result: ok. 1 passed; 0 failed
Status: UNMET.

## G3: Native tracker payload contract remains correct
CHECK: CARGO_TARGET_DIR=/tmp/story-title-prefix-target cargo test --manifest-path native/story-push/Cargo.toml
EXPECT: all native suites pass with zero failures
Status: UNMET.

## G4: All six fingerprints receive a verified resolution or an explicit tool limitation
Manual evidence: exact error_sentinel_resolve responses, with correction commits and test evidence for each fingerprint. A follow-up task alone is not completion.
Status: UNMET.

## G5: Integration and Rust implementation
Manual evidence: PR commits, successful checks, merged state, native help output, and implementation audit. New executable source and tests must be Rust. Existing Node creation is retained unchanged as an external interface under integration test.
Status: UNMET.
