# Setup help and schema gates

Scope: ST-012 (formerly this PR's ST-005, issue #23), es_a8170328dbb7, and es_fda112102cf8.

## Contract

- [x] G1: Native setup help exits zero without Node, settings, or writes.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml --test setup_contract setup_help
  EXPECT: test result: ok.
  CWD: ..
  EVIDENCE: automatic-evidence=v1; definition-sha256=bf0fe867bd0f9b1c0926321bd1ecf5238e531a2ac675d93f5146d3cd591eb0ce; exit=0; EXPECT=matched; output-sha256=f7c2e675be9fc2d2f40a4e93da6fe93d07ab15ad39f44361233033e7b1bc9d69; output-bytes=369; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/a7a2d152af648335b8ccf908232d1be3; path=1f7a7bbdb862/22 entries
- [x] G2: The skill's GitHub JSON example round-trips through init/config; the bare-name mutant is rejected without writes.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml --test setup_contract github_example
  EXPECT: test result: ok.
  CWD: ..
  EVIDENCE: automatic-evidence=v1; definition-sha256=870c7b11601ca29c825b95013449df6c20e9f30a05a7dac8e5a86dfb82be75bc; exit=0; EXPECT=matched; output-sha256=766eef361a5384a4441218b8572e508d556db70a976adfaefa5367aa899a7c80; output-bytes=431; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/a7a2d152af648335b8ccf908232d1be3; path=1f7a7bbdb862/22 entries
- [x] G3: Existing native behavior and repository checks stay green.
  CHECK: cargo test --manifest-path native/story-push/Cargo.toml
  EXPECT: test result: ok.
  CWD: ..
  EVIDENCE: automatic-evidence=v1; definition-sha256=1f243be4fdf6d93b88e537bac4743eefc0cce256fb8dd59852b2b92de129249b; exit=0; EXPECT=matched; output-sha256=731b760dc6f96ee1d3033775bd2765182a449f8845b2585e6f57e9fcd187276f; output-bytes=3269; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/a7a2d152af648335b8ccf908232d1be3; path=1f7a7bbdb862/22 entries
- [x] G4: Rust conversion is MET for new help dispatch and regression tests.
  EVIDENCE: docs/setup-help-verification.md source audit; source commit 815925598286a4b2184054745686e1d714a1fcc5. New help dispatch and tests are Rust; unchanged init/config dependency is explicitly outside migration scope.
- [x] G5: Installed binary and skill pass the same real CLI contracts.
  EVIDENCE: Installed-artifact Cargo setup_contract run passed all three tests. Installed binary hash matches release a3af1b6037ab59420fa15f4a466f79ad995c9395c634cf94b095f46c9a4f15e1.
- [x] G6: Fixes have commits, review, remote CI, and protected integration, or a concrete preserved delivery blocker.
  EVIDENCE: PR #20 head 32982e4fbed602ccf09004ff2cb4ecd2acfe2cb6 passed nine checks and merged as 1344627053952ab4a6e1bc25ed18d6eb44ea4ea3. PR #24 head e4855af18f1c2062b026e8e2c8bcc3b0532a86da passed eleven observed checks and merged into main as ff133d32f75fa5c8a8bf08856a7c1a0968ab1923. Normal protections remained unchanged. Independent typed source review and actual CLI evidence are retained in docs/integration-2026-11-11.md. This is protected source integration, not a sentinel tested-fix attestation.

Existing push and retained non-push Node commands are unchanged dependencies, not migration targets. This task does not claim full-project Rust conversion.
