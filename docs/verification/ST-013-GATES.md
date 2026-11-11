# Gates: ST-013 source-checkout contract repair

OWNS: test/story-policy-contract.rs, backlog/ST-013.md, docs/verification/ST-013*

Scope: repair runtime checkout selection and give each of the seven owner-named fingerprints an evidence-backed disposition.

- [x] G1: A committed regression reproduces caller-cwd contamination before source repair.
  EVIDENCE: test-only parent 837305880fb32af76acfa799cf37716e54de7e24; RED_ASSERTED exit 101, captured child-suite cwd contamination.

- [x] G2: The compiled real contract suite passes from the repair checkout and an unrelated directory without changing process-global cwd.
  CHECK: rustc --edition=2021 --test test/story-policy-contract.rs -o /tmp/ST-013-contract && /tmp/ST-013-contract
  EXPECT: test result: ok.
  EVIDENCE: source fix 569487443afb30f7021ec116a462577c67fcb1a4; 11 passed, 0 failed from repair checkout and /tmp; subprocess regression retains every original contract.

- [x] G3: The same binary passes when invoked from the stale shared anchor, without changing that anchor's files or branch.
  CHECK: cd /home/ejernigan/source/team-teddy/story-time && /tmp/ST-013-contract
  EXPECT: test result: ok.
  EVIDENCE: same binary passed 11 tests from the stale anchor; anchor remains at 1b2a3a5 with its pre-existing .playwright-mcp untracked directory.

- [x] G4: Changed execution logic is native Rust and retained reference contracts remain intact.
  CHECK: npm test && rustc --edition=2021 --test scripts/story-policy.rs -o /tmp/ST-013-policy && /tmp/ST-013-policy
  EXPECT: test result: ok.
  EVIDENCE: changed execution logic is Rust; native policy tests 12 passed, retained reference suite 408 passed with zero failed or skipped; audit recorded in ST-013.md.

- [x] G5: Every named fingerprint has a recorded cause, evidence, and actual supported ledger disposition; no session follow-up is represented as a fix.
  EVIDENCE: ST-013.md names all seven causes and signals. Every named fingerprint now has explicit kind resolve / resolveKind filed evidence. Five have a direct source repair; the unsupported fixed attestation remains a filed gap. The other two retain operational and expected-draft evidence, not fabricated code fixes.

- [ ] G6: Repair is committed, reviewed, and integrated under required CI and repository policy, with owned worktree and background process cleanup.
  EVIDENCE: pending exact-head PR, CI, Jev evidence review, and cleanup.

Checks are written by this repair run. Rust owns new verification logic. Exact observed commands and results are recorded in ST-013.md; this file does not claim unlazy checker-generated evidence.
