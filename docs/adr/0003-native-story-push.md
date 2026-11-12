# Native story push

Status: accepted. Owner requested “rust native” for es_9c55035c0ff8.

## Context

The JavaScript push handler rejects every empty target array. That confuses a missing selector with a valid empty `--all` selection. Gateway construction also requires credentials before a dry-run needs them.

The test-only parent, `b2327ba271a279648284ea32a101cb197b725968`, records the failure: two controls pass, and the empty-backlog regression exits 2 instead of 0. The correction must use Rust, not a Rust wrapper around the JavaScript push implementation.

## Decision

`native/story-push` owns the supported `story push` entry point. Argument parsing, settings, selection, planning, story parsing, card rendering, synchronization, credential validation, linking, and rollback run in Rust. Gitea uses native HTTP. GitHub and Azure DevOps use their existing platform CLIs through Rust subprocess adapters. No push operation invokes Node.

Planning happens before gateway construction. With sync enabled, `--all --dry-run` returns `{"ok":true,"plan":[]}` for an empty or missing backlog. It needs neither credentials nor network access. Missing selectors still exit 2. Sync off still rejects push.

Other story commands remain outside this migration. The Rust dispatcher passes only non-push commands to the retained Node CLI. Configure its location through `STORY_TIME_LEGACY_CLI` or the `legacyCli` field in `~/.config/story-time/runtime.json`. Keep the original JavaScript source and its tests as reference history. Direct `node story.mjs push` is a historical entry point, not the migrated command.

## Consequences

Rust, Cargo, and the locked dependencies are required to build native push. Node remains required only for commands not migrated here. This is not a claim that the whole project is Rust-native.

Push preserves create/update/skip planning and local tracker links. On failure, it restores links already written and compensates remote creates in reverse order. Existing remote updates cannot be rolled back automatically. A failed compensation identifies the item for manual recovery.

The native writer rejects malformed or unsupported story input before remote writes. It rejects symlinked story files and detects concurrent story edits before replacing files. Gitea credentials are not forwarded through redirects or echoed from server errors. Tests use loopback servers or Rust CLI stubs; they do not create production issues.
