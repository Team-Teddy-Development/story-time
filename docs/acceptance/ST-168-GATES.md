# Gates: ST-168 local ownership recovery

OWNS: native/story-push/src/assignment.rs, native/story-push/src/lib.rs, native/story-push/src/rank.rs, native/story-push/tests/assignment_contract.rs, skills/story-pick/SKILL.md, skills/story-time/scripts/usage.txt, skills/story-time/references/assignments.md, docs/adr/0008-local-execution-ownership.md, docs/acceptance/ST-168-GATES.md

Scope: Claim ranked local work atomically, recover verified terminated owners, and protect live, uncertain, and legacy ownership. No Cartographer status reset or automatic legacy migration.

The hosted story recorded acceptance conditions and its criterion-to-test plan before implementation. This ledger consolidates those conditions during root verification; it is not claimed as a pre-implementation file.

- [x] G1: Real CLI verifies atomic selection, verified termination recovery, preserved work, and stale-token fencing.
  CHECK: CARGO_TARGET_DIR=/tmp/st168-wip-target cargo test --manifest-path native/story-push/Cargo.toml --test assignment_contract
  EXPECT: test result: ok. 14 passed; 0 failed
  CWD: ../..
  EVIDENCE: automatic-evidence=v1; definition-sha256=3c04d52757e7776954ba6f3b6e4eb073f7bbfea8a1067302eec8d39252538196; exit=0; EXPECT=matched; output-sha256=4b8b10836bd7cf19fddab914fa9d0f26c5a2d4afa6523e9a9a4f0a4be2b0b3ec; output-bytes=1220; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/1d173901b623fe72e2fb08c658ce02ae; path=e96303706696/35 entries

- [x] G2: Existing ranking, help, locking, skill installation, and Node regression checks still pass.
  CHECK: CARGO_TARGET_DIR=/tmp/st168-wip-target cargo test --manifest-path native/story-push/Cargo.toml --test rank_contract --test help_contract --test help_lists_native --test cli_lock --test install_skills_contract --test doc_native_lists --test list_contract && npm test
  EXPECT: fail 0
  CWD: ../..
  EVIDENCE: automatic-evidence=v1; definition-sha256=0d37262d680581a0fd6371fe83d2d2b09452e433a5fdc75b5994ba6179e3f438; exit=0; EXPECT=matched; output-sha256=850d8f59a345e963bc290eeaffd1cde13cf10138395a22f5cd0314a9beaab149; output-bytes=37605; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/1d173901b623fe72e2fb08c658ce02ae; path=e96303706696/35 entries

- [x] G3: The installed native CLI and updated agent instructions expose the verified ownership path.
  EVIDENCE: Installed the release CLI and selection/reference/native command instructions in four agent roots; all 13 final file hashes verified. Installed CLI smoke observed claimed, heartbeat, capacity_full, reconciled, recovered, stale_token, released, status; partial work retained and fixture/process removed. Live Cartographer status scanned 237 registered worktrees: 242 unknown legacy stories, zero recorded claims, zero changes. Bounded frontmatter scanning completed in 45.3 seconds versus the earlier full-body scan's 70.78 seconds; this is not a controlled benchmark. Evidence, binary digest, and both rollback archives are under ~/.local/state/story-time/ST-168/. No owner was released.

- [x] G4: Source is preserved with an independent evidence review and explicit live-backlog limits.
  EVIDENCE: Source commit c661bfc3eea813c0e9d43920de89f0c0473728d8, implementation PR https://github.com/Team-Teddy-Development/story-time/pull/383 stacked on story PR382. Independent Jev evidence review: supported0.90, verdict supported, confidence0.97. Legacy Cartographer ownership was not reset; active-loop adoption and main-branch merge are not claimed. Source-format CI repair is recorded in this follow-up commit. Retain the source branch and rollback evidence; remove only this run's worktree/build output after verification.
