# Setup help and schema gates

Scope: ST-012 (formerly this PR's ST-005, issue #23), es_a8170328dbb7, and es_fda112102cf8.

## Contract

- [x] G1: Native setup help exits zero without Node, settings, or writes.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml --test setup_contract setup_help
  EXPECT: test result: ok.
  CWD: ..
  EVIDENCE: automatic-evidence=v1; definition-sha256=bf0fe867bd0f9b1c0926321bd1ecf5238e531a2ac675d93f5146d3cd591eb0ce; exit=0; EXPECT=matched; output-sha256=7c152ae6a0d03ded4d162af7baa7f38a4793cf9c1078872aacd45b9fcc69418b; output-bytes=369; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/7585a89f03a1e98d80b497f2072c75ca; path=1f7a7bbdb862/22 entries
- [x] G2: The skill's GitHub JSON example round-trips through init/config; the bare-name mutant is rejected without writes.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml --test setup_contract github_example
  EXPECT: test result: ok.
  CWD: ..
  EVIDENCE: automatic-evidence=v1; definition-sha256=870c7b11601ca29c825b95013449df6c20e9f30a05a7dac8e5a86dfb82be75bc; exit=0; EXPECT=matched; output-sha256=90e981844da2f3450ab7aa011a371f0cdb7091979908163ff292c6a010e72f83; output-bytes=431; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/7585a89f03a1e98d80b497f2072c75ca; path=1f7a7bbdb862/22 entries
- [x] G3: Existing native behavior and repository checks stay green.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml
  EXPECT: test result: ok.
  CWD: ..
  EVIDENCE: automatic-evidence=v1; definition-sha256=1f243be4fdf6d93b88e537bac4743eefc0cce256fb8dd59852b2b92de129249b; exit=0; EXPECT=matched; output-sha256=4b9e38c4e5145264d721f15964e289deb7bde35ce06cc084b5d8a56bcd6db762; output-bytes=3269; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/7585a89f03a1e98d80b497f2072c75ca; path=1f7a7bbdb862/22 entries
- [x] G4: Rust conversion is MET for new help dispatch and regression tests.
  EVIDENCE: docs/setup-help-verification.md source audit; source commit 815925598286a4b2184054745686e1d714a1fcc5. New help dispatch and tests are Rust; unchanged init/config dependency is explicitly outside migration scope.
- [x] G5: Installed binary and skill pass the same real CLI contracts.
  EVIDENCE: Installed-artifact Cargo setup_contract run passed all three tests. Installed binary hash matches release a3af1b6037ab59420fa15f4a466f79ad995c9395c634cf94b095f46c9a4f15e1.
- [ ] G6: Fixes have commits, review, remote CI, and protected integration, or a concrete preserved delivery blocker.
  Evidence required: exact head, PR URL, CI state, and integration state.

Existing push and retained non-push Node commands are unchanged dependencies, not migration targets. This task does not claim full-project Rust conversion.
