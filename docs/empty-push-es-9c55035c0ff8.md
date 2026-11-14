# Empty story push regression

Fingerprint: `es_9c55035c0ff8`.

Status: fixed in the Node handler by ST-098, which also matches the native
`story push` behavior. `native/story-push/tests/push_flag_contract.rs` compares
both runtimes on an empty and a missing backlog.

## Root cause

At baseline `1cbe7d8`, `skills/story-time/scripts/story.mjs` resolves `--all`
against the backlog, then rejects every empty target array as missing arguments.
An empty backlog is a valid result for `--all --dry-run`. Missing explicit targets
must still produce a usage error. The sync-off policy must remain enforced.

The check precedes gateway construction. The correction must also return the
empty plan before constructing an authenticated Gitea gateway, since planning
zero operations needs no credential or network access.

## Reproduce

From the repository root:

```sh
rustc --edition 2021 --test tests/empty-push.rs -o /tmp/story-time-empty-push-tests
/tmp/story-time-empty-push-tests --test-threads=1
```

The tests execute the real CLI with an isolated empty backlog, Gitea localhost
port 1, and no Gitea credential. The empty-plan regression fails with exit 2
instead of 0. The missing-target and disabled-sync tests pass.
Observed baseline result: two passed, one failed. The failed assertion reports
`Usage: story push <id...> | --all [--dry-run]`. Each fixture is removed afterward.

## Acceptance gates

- Empty authorized `push --all --dry-run --json` exits 0 and returns
  `{"ok":true,"plan":[]}` without stderr, credentials, or local writes: MET by ST-098.
- Missing explicit targets remain a usage error: MET by baseline test.
- Disabled synchronization remains rejected: MET by baseline test.
- Nonempty push behavior and full existing suite remain verified after the fix: UNMET.
- Source fix committed, reviewed, integrated, and verified through the installed CLI: UNMET.
- Rust conversion within the approved implementation scope: UNMET.

## Language and scope

`tests/empty-push.rs` is new Rust verification. The defective CLI dispatcher,
push application service, filesystem adapter, and tracker gateways are existing
JavaScript. No JavaScript implementation was changed. Other CLI commands,
installers, and documentation are outside this bug's implementation scope.

The mandatory Rust rule requires an explicit owner exception for a narrow
JavaScript handler correction. Replacing the complete push execution path in
Rust instead changes the size and delivery shape of this task. A Rust wrapper
around the JavaScript handler would not remove the cause and is not a compliant
replacement. No such wrapper was added.

The required next decision is a one-bug JavaScript exception or an expanded
Rust-native push implementation. These tests and historical failure remain
preserved whichever implementation is selected.
