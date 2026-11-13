# ST-022 adoption preview gates

Scope: read-only preview only. Apply, recovery, tracker import and tracker writes remain ST-023 or later. Owner selected ST-022; ST-019 is done. This run is the only active implementation owned by this agent.

## Workflow

Frame and inventory existing boundaries. Establish failing Rust process tests. Implement native inventory and setup routing. Review safety and byte preservation. Run the complete regression matrix and disposable installed CLI. Publish, integrate only under repository policy, accept with criterion evidence, and release owned resources.

Data shape: repository/config/backlog identities, byte preimages, diagnostic conflicts with recovery actions, explicit unknown history, and ordered additive allowed writes. One native preview owner. No init or tracker gateway call from preview.

Throughput checkpoint: prioritize real-command byte-preservation and failure paths before documentation polish. Use one implementation delegate with exclusive code/test/setup ownership; coordinator owns this ledger and story acceptance. No parallel competing source writers.

- [x] G1: Current and legacy preview inventories exact bytes and identities without writes, including custom and terminal records.
  CHECK: cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST022-native-target --test adoption_preview_contract --test adoption_preview_install
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=2edb9ac4983f5c249363b09a1782a5214403c939a3c9059d843922ca5dfab4aa; exit=0; EXPECT=matched; output-sha256=a9c360cf111cb31e110d61f7a773061d0cb5e77d7505b5f2c8cee1fa8501182e; output-bytes=1453; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/d82d0883388360e548166d466a088ff3; path=7fb6f0b79bc1/21 entries
- [x] G2: Native preview and retained initialization pass their full Rust regression suite.
  CHECK: cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST022-native-target
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=d735f4c5af8c3758f6a453de4859994f2a4b297a4f5e9d57253324db84be213b; exit=0; EXPECT=matched; output-sha256=216bbda1656fc88c6391a584dc67c8c27a801cdd5c60f42e205c21645b1f1b14; output-bytes=6016; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/d82d0883388360e548166d466a088ff3; path=7fb6f0b79bc1/21 entries
- [x] G3: Format and strict Rust checks pass.
  CHECK: cargo fmt --manifest-path native/story-push/Cargo.toml --check && cargo clippy --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST022-native-target --all-targets -- -D warnings && printf 'ST022_STRICT_CHECKS_PASS\n'
  EXPECT: ST022_STRICT_CHECKS_PASS
  EVIDENCE: automatic-evidence=v1; definition-sha256=cc0a5bb5efbddb0ab2163ae13a0dcba310be26959f489d27d25c7fac07da581b; exit=0; EXPECT=matched; output-sha256=2b27d300b1e54661a3ae68ad8f01de6fa0e1a8de9c890d05e744c9de074b4a29; output-bytes=238; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/d82d0883388360e548166d466a088ff3; path=7fb6f0b79bc1/21 entries
- [x] G4: Configured Node and Rust policy regression suites pass.
  CHECK: npm test && rustc --edition=2021 --test scripts/story-policy.rs -o /tmp/ST022-policy-tests && /tmp/ST022-policy-tests && rustc --edition=2021 --test test/story-policy-contract.rs -o /tmp/ST022-policy-contract && /tmp/ST022-policy-contract
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=bf1f69623f520c9d3653747baafb7d293f9e4c79735c17ba2d2da4744e3ff624; exit=0; EXPECT=matched; output-sha256=51fa8a3163a306a6c9d6d0dbf6f102d21b0f8926d2a0a2b5b42e645934dc2de5; output-bytes=32466; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/d82d0883388360e548166d466a088ff3; path=7fb6f0b79bc1/21 entries
- [ ] G5: Real installed preview, Rust scope audit, independent review, repository checks, story acceptance and owned cleanup have concrete evidence.

Rust scope gate: MET after implementation and coordinator source inspection. `src/adoption.rs`, the two adoption process/install tests, native dispatch, hashing through locked ring and reused flat-story parsing use Rust. No new non-Rust executable was added. Required new preview implementation/process tests use Rust. Existing Node CLI, installer and regression runner remain unchanged compatibility interfaces outside conversion scope. Markdown/JSON are required documentation/configuration/fixtures, not executable implementation. Any added non-Rust executable in scope leaves this gate unmet.

G3 emits its success token only after numeric zero exits from both strict checks; it is not behavior proof. G1/G2 and actual installed invocation provide behavior proof. Pending gates are not execution failures.
