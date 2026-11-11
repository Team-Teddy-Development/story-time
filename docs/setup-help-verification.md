# Setup help and GitHub schema verification

## Root causes and source corrections

`es_a8170328dbb7` came from `story init --help`. The installed native dispatcher delegated it to the retained parser, which treats an unknown flag as requiring a value. The native dispatcher now handles the exact two-argument help forms before any legacy lookup, parsing, configuration, or process execution.

`es_fda112102cf8` came from an initial configuration with a separate GitHub owner and bare repository name. The validator correctly rejected it. The setup skill lacked an explicit GitHub JSON example. The skill now shows `github.repo: "acme/shop"` and distinguishes that contract from Gitea's separate fields.

RED test commit: `2c08a7d118bd646f2630a638f089cdda5ac3450b`.
Source fix commit: `815925598286a4b2184054745686e1d714a1fcc5`.
Story: ST-005, https://github.com/Team-Teddy-Development/story-time/issues/23.

## Direct verification

The Rust integration test `native/story-push/tests/setup_contract.rs` exercises the compiled executable and reads JSON directly from the skill's Markdown fences. All three tests failed before the source changes. All three pass afterward.

The help test executes both `init --help` and `init -h` with an empty environment and no Node on PATH. It requires exit zero, usage text, empty stderr, and no new files. The positive example test runs `init` and `config --json` in a disposable directory. The negative test changes the example to separate owner and bare repo fields, captures the expected validation diagnostic, and asserts that no settings directory was created.

Final local chain exited zero:

- Rust format and strict Clippy for all native targets.
- Native Rust tests: 19 unit, 9 existing real-CLI integration, and 3 setup regressions. Zero failures or ignored tests.
- Existing Node reference suite: 408 passed, zero failed or skipped.
- Rust repository policy: 12 unit and 8 integration tests passed.
- Release build succeeded.

The same three setup tests pass against `/home/ejernigan/.local/bin/story` and `/home/ejernigan/.pi/agent/skills/story-setup/SKILL.md`. Installed `story init --help` now prints `Usage: story init` and documents the full GitHub coordinate.

## Scope and Rust conversion gate

| Implementation | Language | Role | Status |
| --- | --- | --- | --- |
| Native exact init-help dispatch | Rust | Argument boundary and usage output | Converted, tested, active |
| Setup contract regressions | Rust | Real CLI and documentation example verification | New, tested, active |
| Setup skill | Markdown with JSON data | Agent instructions and configuration example | Required documentation format |
| Retained init/config parser | JavaScript | Unchanged external non-help command dependency | Outside this correction's migration scope |
| Existing push implementation | Rust | Unchanged installed CLI behavior | Dependency from PR #20 |
| Existing Node suite and installer | JavaScript | Retained reference tests and tooling | Outside this correction's migration scope |

Rust conversion gate: MET for all new and replacement executable implementations in this task. This is not full-project conversion. Help does not wrap or execute JavaScript. The schema check intentionally tests the unchanged validator through the actual CLI rather than replacing it.

## Installation and recovery

The native release binary was installed after saving the original binary and skill at `~/.pi/agent/release-evidence/story-setup-help-schema-20261111/`.

Installed binary SHA256: `a3af1b6037ab59420fa15f4a466f79ad995c9395c634cf94b095f46c9a4f15e1`, identical to the release build. Runtime configuration was not changed. Only the GitHub example block is retained as the installed skill patch; an unrelated newer Gitea section from upstream was removed from the installation to preserve its prior content.

Recovery uses the saved `story-before` binary and `SKILL-before.md`. Restore only these two named local files if rollback is required. No application repository or production service is changed.

## Sentinel attestation limitation

Both original fingerprints were previously marked cleared by session follow-up tasks. Those tasks are historical tracking, not code fixes.

After the source commit and passing real regressions, `error_sentinel_resolve(kind: fixed)` was attempted separately for each fingerprint with the full fix SHA, test-only parent SHA, exact regression file, root-cause explanation, and changed lines. Both returned `verification test is not present in the original absolute failing command`. The original failing commands invoke `story`, not a test path, so that verifier cannot attach this later regression. Neither fingerprint has a successful tested-fix ledger attestation. This limitation does not invalidate the recorded RED/GREEN or installed CLI results; it must not be reported as a successful sentinel resolution.

## Review and delivery

Jev reviewed the actual source change and observed RED/GREEN evidence. Verdict `supported`, probability 0.83, verdict confidence 0.82. This is a technical review, not an action-approval check.

The attempted different-family delegate returned only follow-up acknowledgments, not a substantive review. It is not counted as an independent code-review pass. Jev remains the accepted third-party evidence reviewer.

The source depends on the pending native-dispatcher PR #20. The correction will use a separate PR based on that branch. No permission to modify the dependency's source or bypass its protections is inferred.
