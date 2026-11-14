# Read-only adoption preview

ST-022 adds `story adopt preview` to the native Rust CLI. Run it from the project
root. It prints one JSON document on stdout, including for conflicts. It does not
write files, load credentials, contact trackers, or call the retained Node CLI.
ST-023 adds `apply`, `verify`, and `rollback`; see "Apply, verify, and rollback"
below.

```sh
cargo build --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST022-native-target
/path/to/story adopt preview
```

Use the built `/tmp/ST022-native-target/debug/story` as `/path/to/story`, or install
the native binary separately. The unchanged skill installer copies skills only.
`story-setup` routes existing settings, backlogs, and lifecycle metadata to this
same preview before its tracker-specific branches. A new project retains native
`story init --from <answers.json>` behavior.

## Result and preservation contract

| Result | Exit | Meaning |
| --- | --- | --- |
| `preview` | 0 | A read-only additive metadata and receipt plan is available. Adoption has not happened. |
| `already_adopted` | 0 | Version1 metadata and its receipt bind the current repository root, configuration bytes, and full backlog membership and bytes. No writes are proposed. |
| `conflict` | 2 | Diagnostics name paths and recovery or export/import actions. `allowed_writes` is empty. |

`schema_version` is 1. `source_version` is 0 when lifecycle metadata is absent;
`target_metadata_version` is 1. Package and configuration versions are separate.
An unknown lifecycle or receipt version is a conflict, not an upgrade success.
A version1 marker alone cannot produce `already_adopted`.

The preview reads the actual current `.story-time/settings.json` or legacy
`.story-time.json`. Both present is a conflict. It preserves that location, sync
policy, custom settings, and exact bytes. Existing configuration omissions retain
the native compatibility defaults; missing configuration does not establish a
tracker or prefix. Without configuration, `backlog/` is an inspection candidate,
not an adopted backlog identity. Review missing settings inputs separately.

The `config` projection reports location, byte length, SHA256, tracker, and sync.
`backlog` reports the configured directory, prefix, and sorted full flat membership.
Every regular file has a byte digest, including unsupported or malformed files.
Directories and unsafe entries remain visible with diagnostics. No filename,
status, or prefix filter discards custom, non-ASCII, completed, split, or archived
records. Supported flat frontmatter is parsed for identities only. Original bytes
are never serialized or rewritten. Custom fields and sections remain in those
bytes; they are not copied into the report.

`preimages` maps every successfully inspected source file to its exact SHA256 and
length, or records an absent target. Unsafe or unreadable sources have diagnostics
instead of an asserted preimage. Hashes use the locked `ring` SHA256 implementation.
The report excludes raw configuration, story prose, environment contents, and
credential-bearing remote URLs. Existing remote identities and credential-free
links remain visible. Duplicate local IDs, duplicate remote identities or links,
and conflicting supported tracker repository links require a reviewed mapping.

`unknowns` explicitly records `value: null` and a reason for goal, capacity, epic
links, and historical events. Existing epic fields remain in source bytes. No
synthetic goals, assignments, capacity, events, or past sprint dates are produced.

## Proposed writes and identity

A successful unversioned preview lists two ordered `create` operations. They target
`.story-time/lifecycle.json`, then `.story-time/adoption-receipt.json`. Each has an
absent preimage, proposed content, and an exact postimage SHA256. Proposed content
contains preservation identities and explicit unknowns, not original source text.
The receipt binds the exact proposed metadata bytes. Neither file is written.
Existing invalid, orphaned, divergent, or unsupported metadata blocks the plan.

`migration_id` is `adoption-v1-` plus the SHA256 of the compact JSON preservation
object. That object contains the canonical project root, configuration projection,
and sorted backlog inventory in emitted field order. `plan_digest` hashes the
compact JSON report before adding `plan_digest`, without a trailing newline.
The final stdout document ends with a newline. No clock or random value enters
either identity. Repeated previews of unchanged inputs have identical output.

Each path must use normal relative components. Preview rejects absolute paths,
parent traversal, dot components, symlinks at any existing component, and lifecycle
or Git metadata used as a backlog. Checks precede reads. Nested backlogs and
unsupported formats require an immutable export and reviewed identity-preserving
mapping. Tracker-only input requires a supported read/import contract; issue-write
adapters are not import support.

Preview is an observation, not a repository lock or authorization to mutate. A
later apply owner must recheck source freshness and membership, acquire its lock,
and implement the backup, journal, and recovery contract before any write. Preview
does not establish transactional consistency while another process changes files.

## Apply, verify, and rollback

```sh
story adopt preview > /outside/repo/plan.json      # review this file
story adopt apply --plan /outside/repo/plan.json [--state DIR]
story adopt verify --plan /outside/repo/plan.json [--state DIR]
story adopt rollback --plan /outside/repo/plan.json [--state DIR]
```

Each command prints one JSON report: exit 0 on success, exit 2 with `conflict`
diagnostics (`code`, `path`, `recovery`). `--state` defaults to
`$XDG_STATE_HOME` or `~/.local/state`, then `story-time/adoption/<migration_id>`.
It must resolve outside the repository.

Apply refuses a plan that is not version 1, is a preview conflict, was edited
(`plan_digest` mismatch), names an unsafe, credential, or non-create write, or no
longer matches the repository (`stale_plan`). It takes an exclusive OS lock on the
repository root directory (`lock_held` when another run holds it); the OS
releases it when a process exits, so a crash leaves no stale lock. Under the lock
it recomputes the preview, then writes the state directory:

| Path | Content |
| --- | --- |
| `backup/files/NNNN` | Mode-600 byte copies of every reviewed present input (configuration and backlog files). |
| `backup/manifest.json` | Plan digest, migration ID, root, and each copy's path, SHA256, length, and name. |
| `journal.json` | Ordered steps with path, absent preimage, postimage SHA256, `pending`/`complete`/`rolled_back` state and error, directories this run created, and every recorded failure. |

The backup is re-read and checked against the reviewed digests before the first
write. A configuration with credential-like keys, or a dotenv/key file in the
input set, is refused (`credential_in_backup_set`) and nothing is copied. Each step
publishes its file by hard-linking a fully written temporary file, so it never
replaces bytes another writer created.

A rerun classifies each step only by current bytes: absent is pending, postimage
is complete, anything else is `step_diverged`. Apply resumes pending steps and
reports the classification in `resume`. A repeated apply on an adopted repository
returns `already_adopted` without writing. Verify is read-only: it requires an
`already_adopted` preview for the plan's migration, rechecks the backup, and
reports the receipt digest and explicit unknowns. Rollback removes, in reverse
order, only journaled files whose bytes still equal their postimage, then only
empty directories the journal proves this run created. Any diverged step stops
rollback before it removes anything (`rollback_diverged`). Source files are never
modified, so rollback never restores them; the backup stays for manual recovery.
No command contacts a tracker.

`STORY_TIME_ADOPT_FAULT=before:N|after:N` is a test-only fault hook. It stops apply
around journaled write N with exit 75, as a crash would.

## Verification

```sh
cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST022-native-target
cargo fmt --manifest-path native/story-push/Cargo.toml --check
cargo clippy --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST022-native-target --all-targets -- -D warnings
```

`tests/adoption_preview_contract.rs` drives actual native processes with Node absent
and compares full disposable project snapshots before and after each preview.
It covers successful inventory, conflicts, version1 verification, and divergence.
`tests/adoption_preview_install.rs` runs the unchanged Node skill installer in a
disposable project, checks installed setup routing, and exercises native init and
preview with Node unavailable. Existing init regression tests remain unchanged.
All new executable logic and tests are Rust. Node dispatch, the installer, and
existing Node regressions remain unchanged compatibility interfaces outside this
conversion scope. Markdown is documentation, not executable migration logic.
