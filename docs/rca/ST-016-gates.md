# Gates: ST-016 six-fingerprint recovery

OWNS: native/story-push/src/main.rs, native/story-push/tests/recovery_contract.rs, backlog/ST-016.md, docs/rca/ST-016-*.md

Scope: Repair native command help, prove returned-ID selection, preserve all six typed dispositions, and deliver the independent source corrections.

## Runtime outcomes

- [x] G1: Native help succeeds without a legacy runtime or writes.
  CHECK: CARGO_TARGET_DIR=/tmp/story-title-prefix-target cargo test --manifest-path native/story-push/Cargo.toml --test recovery_contract new_help_runs_without_legacy_runtime_or_writes
  EXPECT: test result: ok. 1 passed; 0 failed
  CWD: ../..
  EVIDENCE: automatic-evidence=v1; definition-sha256=19413479b51f3ecfa6c0be2fd830cde53c9accc5e13f18eb4c345d07a8e7a3d3; exit=0; EXPECT=matched; output-sha256=a525e6782482254217fd54f9216b5e5006f0eba4fad71b52d8f55e35c7096dd6; output-bytes=364; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/40577840c34076cb21f2cd98dbcd44ca; path=1f7a7bbdb862/22 entries

- [x] G2: A real newly created story is selected using its returned ID.
  CHECK: CARGO_TARGET_DIR=/tmp/story-title-prefix-target cargo test --manifest-path native/story-push/Cargo.toml --test recovery_contract create_then_push_uses_the_returned_id_instead_of_guessing
  EXPECT: test result: ok. 1 passed; 0 failed
  CWD: ../..
  EVIDENCE: automatic-evidence=v1; definition-sha256=037d64c71f355e60e3e447ab381620583c0fcbf10838b22a0a5d338f0e386b7d; exit=0; EXPECT=matched; output-sha256=bfa5fd7bbea1977ab43ca95d306c2eddbb1a9c6f61c1c597b7f6049f4a94d745; output-bytes=375; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/40577840c34076cb21f2cd98dbcd44ca; path=1f7a7bbdb862/22 entries

- [x] G3: The complete native suite, including changed-title payload contracts, passes.
  CHECK: CARGO_TARGET_DIR=/tmp/story-title-prefix-target cargo test --manifest-path native/story-push/Cargo.toml
  EXPECT: test result: ok.
  CWD: ../..
  EVIDENCE: automatic-evidence=v1; definition-sha256=102ad2cbb440be3d22e0cd64c12a393c10938f81a5e6567e74837455403fbbeb; exit=0; EXPECT=matched; output-sha256=0f33687e9cf6bd1d8f98cb5a6db96c77a64cacf0a31dad14511fdf01052b8dc4; output-bytes=4673; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/40577840c34076cb21f2cd98dbcd44ca; path=1f7a7bbdb862/22 entries

## Delivery and evidence outcomes

- [x] G4: Each requested fingerprint has an accepted typed disposition with its exact correction or recovery evidence.
  EVIDENCE: error_sentinel_resolve returned resolveKind filed for all six, sequences 185 through 190. Each confirmed signal binds the original fingerprint and correction commit to the existing historical-evidence prerequisite. These are confirmed gap filings, not tested-source-fix verdicts. See ST-016-sentinel-recovery.md.

- [x] G5: The source corrections are merged and the installed native CLI exercises the repaired path.
  EVIDENCE: PR #40 merged at 01d56defc7ff102dc52069ed50a11c6fd91005a9. PR #42 merged at 6dfbd92a85b546824342b75435a562d926f51db4 after all nine checks passed. cargo install --path native/story-push --locked --root /home/ejernigan/.local --force exited 0. Both env -i PATH=/nonexistent /home/ejernigan/.local/bin/story new --help and new -h printed usage and exited 0. Restore archive: /tmp/story-native-before-ST016.tar.gz. New executable source and tests are Rust; retained Node creation is an unchanged product interface under test. The installed unlazy checker is existing external orchestration, not new implementation.

- [ ] G6: The installed resolver accepts the correct tested-source or operational-recovery classifications for the original evidence.
  EVIDENCE: UNMET. All six direct submissions were rejected because the original Cargo and CLI invocation families are unsupported. The existing historical-evidence prerequisite remains Blocked; see the exact signal IDs and handoff in ST-016-sentinel-recovery.md. No ledger edit or substitute Node proxy is permitted.
