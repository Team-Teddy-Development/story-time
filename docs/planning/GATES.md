# Gates: ST-018 planning skills

Scope: Deliver refinement and sprint-planning skills, one compatibility alias, native version-1 planning artifacts, safe failure paths, and installed-surface verification. No adoption apply or tracker sprint writes.

- [x] G1: Native process contracts cover complete inventory, selection arithmetic, provisional inputs, source freshness and safe output.
  CHECK: cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-final-native-target --test planning_contract
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=3f511c675a46a0f755be430f94b1eed65a7ca729de031856251a32db6ae39ab2; exit=0; EXPECT=matched; output-sha256=c408f0ce591fd44dd391ff87dcff654e6c1e6fc4c2b18f8578993fe2e586b58b; output-bytes=1619; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/54eb00525f9840817bd96491a1134fba; path=7fb6f0b79bc1/21 entries

- [x] G2: Disposable installed skills expose both entry points and the compatibility alias with a real native plan.
  CHECK: cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-final-native-target --test planning_install
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=d3b89e684483c0ffcde7fc05c4c8255c0bd8f0e7351e5bfcd677eb30d818627b; exit=0; EXPECT=matched; output-sha256=efaf7a1bf0c2c201303f972ce460484579ef8f95c891d2dc981253e81ab39761; output-bytes=535; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/54eb00525f9840817bd96491a1134fba; path=7fb6f0b79bc1/21 entries

- [x] G3: Full native tests, formatting and strict Clippy pass.
  CHECK: cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-final-native-target && cargo fmt --manifest-path native/story-push/Cargo.toml --check && cargo clippy --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-final-native-target --all-targets -- -D warnings && printf '\nST018_NATIVE_CHAIN_VERIFIED\n'
  EXPECT: ST018_NATIVE_CHAIN_VERIFIED
  EVIDENCE: automatic-evidence=v1; definition-sha256=8268a32d8a0a914ebb4ddb0a191f35ba4991ebb85f887d432706e028ff26eabb; exit=0; EXPECT=matched; output-sha256=8c58310fd351f69db28a568560cf7f54f5645ae2f21c61ea6dafbcbfa58bbe34; output-bytes=8264; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/54eb00525f9840817bd96491a1134fba; path=7fb6f0b79bc1/21 entries

- [x] G4: Retained Node and Rust policy contracts pass without replacing their executable implementations.
  CHECK: npm test && rustc --edition=2021 --test scripts/story-policy.rs -o /tmp/ST018-policy-tests && /tmp/ST018-policy-tests && rustc --edition=2021 --test test/story-policy-contract.rs -o /tmp/ST018-policy-contract && /tmp/ST018-policy-contract && git diff --check
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=bf4ef465b7dce59a78e4aae184305231f24408c15c6f550ada1b155af299f640; exit=0; EXPECT=matched; output-sha256=66f01a0ebae3f3cc62ed1ac7e1795884aab110c2605c3260bade141567d64f47; output-bytes=33163; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/54eb00525f9840817bd96491a1134fba; path=7fb6f0b79bc1/21 entries

- [x] G5: Real installed CLI produces a provisional plan from the current backlog, source bytes remain unchanged, and scope language audit passes.
  EVIDENCE: Cargo-installed binary a8df95cabd80c161a84dc9271b71043aae9f1ab74e259a6df03a9820ad882587 ran snapshot and plan with exit0,19 source files,9 dispositions,selected[],provisional and publication writes[]; all story/config digests OK. See verification.md parent proof and scoped language audit. Actual installer exercised both agents and documented native paths.

- [ ] G6: Independent review, story acceptance, required exact-head CI, normal PR integration and owned-resource cleanup complete.
  EVIDENCE: pending
