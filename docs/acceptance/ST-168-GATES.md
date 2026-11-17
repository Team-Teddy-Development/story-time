# Gates: ST-168 local ownership recovery

OWNS: native/story-push/src/assignment.rs, native/story-push/src/lib.rs, native/story-push/src/rank.rs, native/story-push/tests/assignment_contract.rs, skills/story-pick/SKILL.md, skills/story-time/scripts/usage.txt, skills/story-time/references/assignments.md, docs/adr/0008-local-execution-ownership.md, docs/acceptance/ST-168-GATES.md

Scope: Claim ranked local work atomically, recover verified terminated owners, and protect live, uncertain, and legacy ownership. No Cartographer status reset or automatic legacy migration.

The hosted story recorded acceptance conditions and its criterion-to-test plan before implementation. This ledger consolidates those conditions during root verification; it is not claimed as a pre-implementation file.

- [x] G1: Real CLI verifies atomic selection, verified termination recovery, preserved work, and stale-token fencing.
  CHECK: CARGO_TARGET_DIR=/tmp/st168-wip-target cargo test --manifest-path native/story-push/Cargo.toml --test assignment_contract
  EXPECT: test result: ok. 12 passed; 0 failed
  CWD: ../..
  EVIDENCE: automatic-evidence=v1; definition-sha256=6b1ccb35c3bf0e572ad7405e5f12fd0657c4f0be5fa2e840193902dd3df81aa3; exit=0; EXPECT=matched; output-sha256=9a1021a0474188981c96808a42e12dcb5f03adb40e96205a0eca26e6ff3dcd6d; output-bytes=1086; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/1d173901b623fe72e2fb08c658ce02ae; path=e96303706696/35 entries

- [x] G2: Existing ranking, help, locking, skill installation, and Node regression checks still pass.
  CHECK: CARGO_TARGET_DIR=/tmp/st168-wip-target cargo test --manifest-path native/story-push/Cargo.toml --test rank_contract --test help_contract --test help_lists_native --test cli_lock --test install_skills_contract && npm test
  EXPECT: fail 0
  CWD: ../..
  EVIDENCE: automatic-evidence=v1; definition-sha256=f19d2f43ec7bd1698efb2243dced755db32d294975b0a702b11ccb1ffec0f9af; exit=0; EXPECT=matched; output-sha256=4ae0437bd09124bcccacf68907e238c595e0ad38923c1eaa56c9eef458d8cf11; output-bytes=36743; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/1d173901b623fe72e2fb08c658ce02ae; path=e96303706696/35 entries

- [x] G3: The installed native CLI and updated agent instructions expose the verified ownership path.
  EVIDENCE: Installed source c661bfc3eea813c0e9d43920de89f0c0473728d8 into ~/.local/bin/story and four agent skill roots; 9 file hashes verified. Direct installed CLI smoke observed claimed, heartbeat, capacity_full, reconciled, recovered, stale_token, released, status. Partial work retained and fixture/process removed. Evidence and rollback archive are under ~/.local/state/story-time/ST-168/. The later source correction changes module formatting only.

- [x] G4: Source is preserved with an independent evidence review and explicit live-backlog limits.
  EVIDENCE: Source commit c661bfc3eea813c0e9d43920de89f0c0473728d8, implementation PR https://github.com/Team-Teddy-Development/story-time/pull/383 stacked on story PR382. Independent Jev evidence review: supported0.90, verdict supported, confidence0.97. Legacy Cartographer ownership was not reset; active-loop adoption and main-branch merge are not claimed. Source-format CI repair is recorded in this follow-up commit. Retain the source branch and rollback evidence; remove only this run's worktree/build output after verification.
