# Gates: ST-168 local ownership recovery

OWNS: native/story-push/src/assignment.rs, native/story-push/src/lib.rs, native/story-push/src/rank.rs, native/story-push/tests/assignment_contract.rs, skills/story-pick/SKILL.md, skills/story-time/scripts/usage.txt, skills/story-time/references/assignments.md, docs/adr/0008-local-execution-ownership.md, docs/acceptance/ST-168-GATES.md

Scope: Claim ranked local work atomically, recover verified terminated owners, and protect live, uncertain, and legacy ownership. No Cartographer status reset or automatic legacy migration.

The hosted story recorded acceptance conditions and its criterion-to-test plan before implementation. This ledger consolidates those conditions during root verification; it is not claimed as a pre-implementation file.

- [ ] G1: Real CLI verifies atomic selection, verified termination recovery, preserved work, and stale-token fencing.
  CHECK: CARGO_TARGET_DIR=/tmp/st168-wip-target cargo test --manifest-path native/story-push/Cargo.toml --test assignment_contract
  EXPECT: test result: ok. 12 passed; 0 failed
  EVIDENCE: pending

- [ ] G2: Existing ranking, help, locking, skill installation, and Node regression checks still pass.
  CHECK: CARGO_TARGET_DIR=/tmp/st168-wip-target cargo test --manifest-path native/story-push/Cargo.toml --test rank_contract --test help_contract --test help_lists_native --test cli_lock --test install_skills_contract && npm test
  EXPECT: fail 0
  EVIDENCE: pending

- [ ] G3: The installed native CLI and updated agent instructions expose the verified ownership path.
  EVIDENCE: pending; record installation backup, installed CLI smoke results, and exact installed skill-file comparisons.

- [ ] G4: Source is preserved with an independent evidence review and explicit live-backlog limits.
  EVIDENCE: pending; record source commit, PR, review, and cleanup. Legacy Cartographer ownership is not silently reset and live loop adoption is not claimed without evidence.
