# Setup help and schema gates

Scope: ST-005, es_a8170328dbb7, and es_fda112102cf8.

## Contract

- [ ] G1: Native setup help exits zero without Node, settings, or writes.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml --test setup_contract setup_help
  EXPECT: test result: ok.
- [ ] G2: The skill's GitHub JSON example round-trips through init/config; the bare-name mutant is rejected without writes.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml --test setup_contract github_example
  EXPECT: test result: ok.
- [ ] G3: Existing native behavior and repository checks stay green.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml
  EXPECT: test result: ok.
- [ ] G4: Rust conversion is MET for new help dispatch and regression tests.
  Evidence required: source and execution-chain audit in docs/setup-help-verification.md.
- [ ] G5: Installed binary and skill pass the same real CLI contracts.
  Evidence required: installed-artifact regression output and checksums.
- [ ] G6: Fixes have commits, review, remote CI, and protected integration, or a concrete preserved delivery blocker.
  Evidence required: exact head, PR URL, CI state, and integration state.

Existing push and retained non-push Node commands are unchanged dependencies, not migration targets. This task does not claim full-project Rust conversion.
