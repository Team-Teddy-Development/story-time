# ST-018 verification

## RED before implementation

Command `cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-native-target --test planning_contract` exited 101.

```text
running 1 test
test native_plan_dispatch_needs_no_node ... FAILED
native plan dispatch failed: Commands other than init, push, and adopt require STORY_TIME_LEGACY_CLI or ~/.config/story-time/runtime.json with legacyCli.
test result: FAILED. 0 passed; 1 failed; 0 ignored
```

The process clears its environment and uses PATH=/nonexistent. The failure is the missing native plan dispatch, not a compilation or invocation error. Implementation follows this observation.

## Execution decisions

Parent supplied the completed two-option design exploration. No recursive delegation, story edits, commits, or pushes in this delegate. Model the Domain selected typed request, snapshot, and artifact records. Boundary Discipline keeps filesystem validation outside the pure planner. Native code does not use adoption projection or the push gateway.

Throughput checkpoint: dispatch RED blocks implementation; code and skills share one documented interface and stay with one writer. Source state is read-only. Publication owns only its explicit JSON/Markdown pair. Parent owns independent review, acceptance, and integration.

AC3 interpretation was resolved with `jev_choice`. Choice `remote_provisional` had confidence 0.98, probability 0.99. Remote tracker adapters lack sprint-field publication contracts, so even complete local inputs retain `publication.tracker_sprint_fields` as a provisional reason. A complete files-only plan can be planned. An earlier advisory cite supported the local-only boundary at 0.83 but did not settle the literal provisional requirement; the later choice controls the implementation. Neither result grants tracker-write authority.

Existing ADR 0005 owns the local planning architecture. This implementation introduces no parallel adoption or epic service.

## GREEN and corrected failures

The final build-watch chain `st018-verified-final` exited 0. It ran full Cargo tests, formatting, strict Clippy, Node regressions, both compiled Rust policy suites, and `git diff --check`.

| Check | Observed result |
| --- | --- |
| `cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-native-target` | 71 passed, 0 failed. Includes 14 planning process tests and 1 real installation test. |
| `cargo fmt --manifest-path native/story-push/Cargo.toml --check` | Exit 0. |
| `cargo clippy --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-native-target --all-targets -- -D warnings` | Exit 0. |
| `npm test` | 418 passed, 0 failed, 0 skipped. |
| `rustc --edition=2021 --test scripts/story-policy.rs -o /tmp/ST018-policy-tests && /tmp/ST018-policy-tests` | 12 passed, 0 failed. |
| `rustc --edition=2021 --test test/story-policy-contract.rs -o /tmp/ST018-policy-contract && /tmp/ST018-policy-contract` | 11 passed, 0 failed. |
| `git diff --check` | Exit 0. |

The earlier strict Clippy run rejected an unnecessary size unwrap. Pattern matching now binds the known size directly. No lint suppression was added. Earlier native installer and Node regressions rejected the expanded setup skill's 251 split lines. Its handoff prose was shortened; the original <=250 contract and all original tests remain unchanged. Current setup has 249 newline characters and passes both installed and repository contracts.

A failed targeted edit used text from the installed router instead of the repository router. The repository file was read, exact text was used, and the corrected edit succeeded. A read requested line 480 of a 477-line test file; the subsequent bounded read used line 430. These were invocation errors, not source fixes. The session ledger still records fingerprints `es_a3a66d683550`, `es_005acf2166ce`, `es_1685d7625846`, `es_b0ffb937689c`, and `es_bef0b71ed63c`. Its generic resolver requires commit or pinned historical evidence, which this no-commit delegate cannot supply. Preserve this recovery evidence for parent handoff rather than claim the original invocations succeeded.

## Real current-backlog proof

The actual binary executed `plan --snapshot` and `plan --from /tmp/ST018-current-request.json` from this owned checkout. The request and result are outside the repository. Observed result was schema1, 19 source story hashes, 9 open-story dispositions, zero selected IDs, provisional assessment, explicit missing goal/calendar/capacity, and local-only publication. Source HEAD was `c3463f873cfaca66539abdfc765cd68bc2658e05`. Config SHA-256 was `3cd1468b016d0c3ce8d01f9ac5b457d0d16ca546b06e72eaba952552912de245`.

The exact preservation check was:

```bash
sha256sum backlog/*.md .story-time/settings.json > /tmp/ST018-source-before.sha
/tmp/ST018-native-target/debug/story plan --snapshot > /tmp/ST018-current-request.json
/tmp/ST018-native-target/debug/story plan --from /tmp/ST018-current-request.json > /tmp/ST018-current-plan.json
sha256sum -c /tmp/ST018-source-before.sha
```

All 19 story files and settings reported `OK`. The real installation test runs the retained Node installer into disposable HOME directories for Claude and Codex. It inspects both installed skill entry points, both metadata files, the alias, router, shipped schema/reference, source README, and expected skill data. It executes the documented snapshot/stdout/output-pair commands for both refinement modes and sprint planning, with Node unavailable on the native execution PATH. JSON artifacts equal stdout; Markdown exists; original settings and estimates retain exact bytes. Temporary homes and fixture projects are removed by Rust `Drop`.

## Scoped language audit and conversion gate

Scoped Rust conversion gate: MET for new planning implementation and tests. No active in-scope non-Rust implementation or unclassified implementation remains.

| Path or execution chain | Language and role | Conversion status |
| --- | --- | --- |
| `native/story-push/src/main.rs`, `src/lib.rs`, `src/planning.rs` | Rust dispatch, typed source snapshot, boundary validation, pure selection, JSON/Markdown generation, local publication | Native Rust. No adoption-private projection or push Gateway. |
| `native/story-push/tests/planning_contract.rs` | Rust real-process safety and behavior contracts | Native Rust. |
| `native/story-push/tests/planning_install.rs` | Rust test controller over real installer and native command paths | Native Rust. |
| `ring`, `serde`, `serde_json`, `chrono`, Rust standard library | Third-party or platform dependencies | Required dependencies, not project conversion targets. Existing ring provides hashes. Cargo.lock retains original versions; only serde derive/direct dependency edges changed. |
| `skills/refinement`, `skills/sprint-planning`, alias/router/handoff prose | Markdown orchestration and YAML interface metadata | Documentation/configuration, not executable implementations. |
| Planning request JSON schema and `test/expected-skills.mjs` names | Static schema and expected-skill data | Required data formats. No executable legacy test changes. |
| Retained `skills/story-time/scripts`, `scripts/install.mjs`, `scripts/run-tests.mjs` | Node compatibility CLI, installer, regression runner | Explicitly out of this approved conversion scope; executable bytes unchanged. No native wrapper delegates planning logic to them. |

`git diff --exit-code -- skills/story-time/scripts scripts/install.mjs scripts/run-tests.mjs` exited 0. No repository target or node_modules directory was created. All Cargo output uses `/tmp/ST018-native-target`.

## Changed-file handoff

Delegate-owned files are listed below. Parent-owned `backlog/ST-018.md`, `docs/planning/GATES.md`, and `docs/planning/decisions.tsv` are preserved, not edited by this delegate.

```text
README.md
native/story-push/Cargo.lock
native/story-push/Cargo.toml
native/story-push/src/lib.rs
native/story-push/src/main.rs
native/story-push/src/planning.rs
native/story-push/tests/planning_contract.rs
native/story-push/tests/planning_install.rs
package.json
skills/backlog-groom/SKILL.md
skills/refinement/SKILL.md
skills/refinement/agents/openai.yaml
skills/sprint-planning/SKILL.md
skills/sprint-planning/agents/openai.yaml
skills/story-accept/SKILL.md
skills/story-kickoff/SKILL.md
skills/story-map/SKILL.md
skills/story-pick/SKILL.md
skills/story-setup/SKILL.md
skills/story-time/SKILL.md
skills/story-time/references/planning.md
skills/story-time/references/planning-request.schema.json
test/expected-skills.mjs
docs/planning/README.md
docs/planning/verification.md
```

## Parent review corrections

Independent static review at `/tmp/ST018-independent-review.md` identified B1: a configured backlog under docs/planning could collide with the generated Markdown. The parent added `output_never_overlaps_configured_backlog_even_with_reviewed_preimages`. Real RED exited 101 because publication returned 0 and replaced the disposable source. The fix rejects the entire backlog subtree and all captured inputs before any output-side mutation and rechecks under the lock. Story and epic-map controls now pass with exact unchanged project snapshots.

The parent also reproduced fractional arithmetic failure: budget0.3/reserve0.1/sizes0.1+0.1 selected only one story. `fractional_point_budget_uses_exact_decimal_arithmetic` failed101 before the fix. Validation now bounds points to six decimals and 1,000,000,000 points; pure selection uses exact integer millionths. GREEN selects both and leaves zero. Excess precision and oversized values fail without output. The shipped schema and reference disclose these bounds. Focused process suite now reports16 passed,0 failed.

## Parent verification and mutation control

Parent G1-G4 all passed with exit0 and matched expectations after both corrections. Runnable evidence is in GATES.md; the checker's exit1 named only pending manual G5/G6 and is not a failed test. Full native count is now73 (including16 process +1 install), Node418, policy12+11, fmt/strict Clippy/diff checks pass.

The parent copied the native crate to `/tmp/ST018-planning-mutant-20261113`, inverted the backlog containment condition, and ran `output_pair_requires_both_exact_preimages_and_rechecks_markdown_before_writes`. The mutant exited101 because it incorrectly rejected the valid non-backlog output pair. This demonstrates the guard is tested in both directions. Production source was never mutated. The first production rerun reused `/tmp/ST018-native-target` after the copied crate's mutation run. Cargo's colliding same-package outputs retained the mutant binary, producing two valid-output failures with the inverted guard's diagnostic despite unchanged production source. This is a verification-build isolation failure, not a new source regression. Final source gates now use the separate `/tmp/ST018-final-native-target`; mutant builds must use their own target directory. Retain the failed run and supersede it only with the isolated source rebuild. The release installation uses its separate `/tmp/ST018-install-target` and was not affected.

Cargo installed the actual executable at `/home/ejernigan/.local/state/story-time/ST018-planning-skills/bin/story`. Its SHA256 is `a8df95cabd80c161a84dc9271b71043aae9f1ab74e259a6df03a9820ad882587`; cmp against `/tmp/ST018-install-target/release/story` passed. Installed current-backlog snapshot/plan commands exited0 and produced19 source files,9 dispositions,selected[],provisional assessment,explicit unknowns and publication writes[]. All19 source stories and config passed independent SHA256 checks. Request/output/digests are retained under that installation's docs directory. Real-artifact proof recorded.

Independent Jev review of B1 and exact arithmetic corrections returned supported0.85, verdict supported with confidence0.85. Preserve the original blocking review; this correction disposition supersedes B1 only with its real RED/GREEN evidence, not model agreement alone.

## Remaining boundaries and next commands

No test or compiler failure remains. Multi-file publication is not crash-atomic; after a process kill, inspect both output files and the PID-bearing lock before recovery. No tracker sprint service, adoption apply, or authoritative history is implemented. These are preserved scope limits, not successful remote operations.

No worktree or pane was created by this delegate. The supplied worktree remains dirty and parent-owned. Cleanup's read-only call encountered an approval condition, then its retry requested a preview of the already-dry-run call. Git worktree inventory and status were inspected. The delegate did not loop approval retries or remove another run's tree. All owned build commands exited with process cleanup succeeded.

Completion checks do not establish acceptance. The first deterministic confidence check returned 0 because it did not recognize build-watch evidence as a paired Bash test, had no fresh sentinel barrier, and preceded final proof. A later direct Cargo run passed all 71 tests. The sentinel still retains the five corrected invocation fingerprints above; no unauthorized commit was created to force their resolution. Semantic completeness returned 79/100 with confidence 0.52 and done probability 0.70, below the required 0.85. It named no concrete source gap; independent parent review and integration remain explicitly pending. Do not relabel these checks as passes or repeat unchanged scoring for permission.

Parent still owns independent review, commit, story acceptance, exact-head CI, and integration. There is no delegate commit or push. G6 remains pending. Rerun the contract command below after parent edits:

```bash
cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-native-target --test planning_contract --test planning_install
```
