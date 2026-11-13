# Gates: ST-018 planning skills

Scope: Deliver refinement and sprint-planning skills, one compatibility alias, native version-1 planning artifacts, safe failure paths, and installed-surface verification. No adoption apply or tracker sprint writes.

- [x] G1: Native process contracts cover complete inventory, selection arithmetic, provisional inputs, source freshness and safe output.
  CHECK: cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-final-native-target --test planning_contract
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=1ec229eb44d2d98a633380e6ee63287a29abe78f342cec72ae6194f1286aad93; exit=0; EXPECT=matched; output-sha256=1124f3f7163051315dffa37f4236eb61bdf98def9bdfc128efa4c995a7576c4d; output-bytes=1625; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/54eb00525f9840817bd96491a1134fba; path=7fb6f0b79bc1/21 entries

- [x] G2: Disposable installed skills expose both entry points and the compatibility alias with a real native plan.
  CHECK: cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-final-native-target --test planning_install
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=5dba68b5f652bb3d8fd74c56591579a237eac35054046d7b50d01598e3750eae; exit=0; EXPECT=matched; output-sha256=39981103bca5694c6f684a80218ae98b3a1c75aa7f12f18400b67f3bda654b45; output-bytes=401; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/54eb00525f9840817bd96491a1134fba; path=7fb6f0b79bc1/21 entries

- [x] G3: Full native tests, formatting and strict Clippy pass.
  CHECK: cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-final-native-target && cargo fmt --manifest-path native/story-push/Cargo.toml --check && cargo clippy --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-final-native-target --all-targets -- -D warnings && printf '\nST018_NATIVE_CHAIN_VERIFIED\n'
  EXPECT: ST018_NATIVE_CHAIN_VERIFIED
  EVIDENCE: automatic-evidence=v1; definition-sha256=36e7cad88f26f30601bfefb7bd3f0286401f88744907a3ace8f1b29c66311697; exit=0; EXPECT=matched; output-sha256=5d47b3e0d5e0a01c0496de783fed797d55131d638b9fc7ad4885a4ea5d9105a7; output-bytes=8043; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/54eb00525f9840817bd96491a1134fba; path=7fb6f0b79bc1/21 entries

- [x] G4: Retained Node and Rust policy contracts pass without replacing their executable implementations.
  CHECK: npm test > /tmp/ST018-final-node.log 2>&1 && rustc --edition=2021 --test scripts/story-policy.rs -o /tmp/ST018-policy-tests && /tmp/ST018-policy-tests && rustc --edition=2021 --test test/story-policy-contract.rs -o /tmp/ST018-policy-contract && /tmp/ST018-policy-contract && git diff --check
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=e6fffd2f15c41a17a04e7d2b069cc6e2edb56498f5ce55e9b3311ce58f9f2b99; exit=0; EXPECT=matched; output-sha256=75135477c9cb14522cc0ba87d6f8f79c08e94ada2211a76d75753d8a5d476d80; output-bytes=1669; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/54eb00525f9840817bd96491a1134fba; path=7fb6f0b79bc1/21 entries

- [x] G5: Real installed CLI produces a provisional plan from the current backlog, source bytes remain unchanged, and scope language audit passes.
  EVIDENCE: Cargo-installed binary a8df95cabd80c161a84dc9271b71043aae9f1ab74e259a6df03a9820ad882587 ran snapshot and plan with exit0,19 source files,9 dispositions,selected[],provisional and publication writes[]; all story/config digests OK. See verification.md parent proof and scoped language audit. Actual installer exercised both agents and documented native paths.

- [ ] G6: Independent review, story acceptance, required exact-head CI, normal PR integration and owned-resource cleanup complete.
  EVIDENCE: pending
