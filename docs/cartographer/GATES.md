# Gates: Story Time onboarding recovery

OWNS: backlog/ST-002.md docs/cartographer/** docs/adr/0002-read-only-onboarding-preflight.md tests/cartographer_setup.rs tools/cartographer-preflight/** .github/workflows/test.yml

Scope: Repair the named operational and story-score incidents, inspect admission prerequisites, and preserve the authorized repository onboarding work. Private signing material, backend rollout, and the global sentinel implementation remain outside scope.

## Repository preparation

- [x] G1: The actual Story Time CLI has an estimated and scored onboarding story.
  CHECK: rustc --edition=2021 -D warnings --test tests/cartographer_setup.rs -o /tmp/story-time-cartographer-setup-tests && /tmp/story-time-cartographer-setup-tests --test-threads=1
  EXPECT: test result: ok.
  EVIDENCE: automatic-evidence=v1; definition-sha256=15993af7afce2764d0800f1933c4e7b4d429a42ffd3b3e8fd8bc4d3cea1d8352; exit=0; EXPECT=matched; output-sha256=ffda7eafaef7d14c9c45441f2b2bc4b79438d2f3bc311a9588624e48a6b9d3ba; output-bytes=228; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/3dd5ec74690c26fe1a241eae08c00997; path=1f7a7bbdb862/22 entries

- [x] G2: Every named error has a specific cause and actual recovery evidence, with API limitations distinguished from fixes.
  EVIDENCE: error-recovery.md names all nine fingerprints. Five have historical accepted absolute-path corrections; three are symptoms of a rejected recorder invocation subsequently recovered by the accepted corrections; one has a tested data fix. The generic recorder gap remains explicit. No claim of nine product-code fixes or ledger-only completion.

- [x] G3: Production admission prerequisites are measured through supported read-only operations.
  EVIDENCE: Real Rust MCP response in live-preflight.jsonl; actual Coffee deployment projection, Argo readback, and unauthenticated browser observation are recorded in admission-evidence.md. v2 desired0/ready0 and both missing v2 reader URLs are blockers, not passes. Core intake CRT-052 is preserved as Cartographer PR #194; its draft lint gaps are intentional intake work for the Cartographer session, not passed readiness.

- [x] G4: Repository onboarding changes pass the existing regression suite.
  CHECK: npm test
  EXPECT: pass 408
  EVIDENCE: automatic-evidence=v1; definition-sha256=779d06795fbfa59c02a3ebc6fd7844ee34cedb7fd2ccab5963288e5681d45067; exit=0; EXPECT=matched; output-sha256=0388865b181709593b318fad41ab2d84c9e73098a9b391368351fc41d214a92e; output-bytes=30827; shell=/bin/sh; cwd=/home/ejernigan/source/team-teddy/worktrees/runs/3dd5ec74690c26fe1a241eae08c00997; path=1f7a7bbdb862/22 entries

- [x] G6: The Rust scope audit classifies each changed implementation and adds no non-Rust executable implementation.
  EVIDENCE: README.md language table and ADR 0002 classify the Rust preflight, Rust protocol and CLI tests, YAML CI configuration, Markdown/data records, external Cartographer services, and pre-existing out-of-scope JavaScript. Scoped Rust gate MET; project-wide JavaScript conversion remains UNMET and outside scope. Clippy all targets -D warnings and 14 Rust tool tests pass. The v2 predicate mutant was rejected after strengthening the per-clause assertion; restored source passes. No lint suppression or weakened test was added.

## Live onboarding

- [ ] G5: The authenticated portfolio lists Story Time under Team Teddy and a permitted worker completes a bounded task.
  EVIDENCE: UNMET. No qualified active v2 path, authenticated portfolio session, private operator signing inputs, or execution permit was available. The remaining original onboarding criterion stays unchecked in ST-002 after the peer rename. GitHub dogfooding ST-001 is unchanged. Use the conditional operator handoff after backend qualification. Do not access private keys or enable an unqualified authority to force this gate to pass.

## Independent review

Jev reviewed the actual Rust implementation, Rust tests, Cargo manifest, and CI commands across correctness, readability, architecture, security, and performance. Typed result: `choice=approve`, `confidence=0.56`, probabilities `approve=0.78`, `request_changes=0.22`. This is a bounded pre-merge review, not a claim that production onboarding is complete. No retry seeks a higher review score.

## Remote preservation

Pending the final Story Time commit and GitHub PR #15 checks. Live repository policy requires one code-owner/team approval and last-push approval; this run does not bypass that rule. Jev's evidence review cannot supply a GitHub account or remove a required review. ST-002 is mirrored as GitHub issue #19. Incoming PR head fa0f39f renamed onboarding from ST-001 to ST-002; the repair follows that existing identity and preserves GitHub settings and the separate ST-001 byte-for-byte. Cartographer intake commit `84a99fc1d1574035af615087bdf872c5c9c83f16` is already preserved in PR #194; Rust story-ID validation reported 269 clauses, 0 failed. The feeder run stops at that intake and does not merge backend changes.
