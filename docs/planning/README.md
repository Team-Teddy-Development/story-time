# Generate a local sprint proposal

ST-018 provides `refinement` and `sprint-planning`. `backlog-groom` is a thin alias
to refinement in backlog mode. `story-map` remains the epic owner. Plans do not
start sprints, update story status or estimates, or synchronize tracker fields.

## Install the native prerequisite

Install native `story` with the steps in the repository
[README install section](../../README.md#install). From the Story Time source
checkout, the manual route is:

```bash
cargo install --path native/story-push --locked
```

The native `story` binary runs every command, including `story config` and
`story plan`, and no `story` command requires Node.

The retained Node skill installer copies skills only. It does not install native
`story`, Cargo, or documentation outside the skills tree. Copied skills include
the [planning request reference](../../skills/story-time/references/planning.md)
and [version-1 JSON schema](../../skills/story-time/references/planning-request.schema.json).

## Run a complete request against your project

From the target project root, run this complete request example:

```bash
story plan --snapshot > request.json
story plan --from request.json
```

The first command generates the complete request, including real current hashes.
The second executes it unchanged and prints a provisional JSON inventory. It uses
null goal, calendar, and capacity without inventing dates or commitments. Keep the
request outside the configured backlog. Use a temporary path if you do not need
to retain it. Neither command changes backlog, configuration, or tracker state.

Review the generated `plan_id`, `candidate_order`, `goal`, `calendar`, `capacity`,
`epic_evidence`, `exclusions`, and `forecasts`. The reference contains a complete
planning-field example with budget 8 and reserve 1. Preserve the captured `source`
object. Do not supply a new readiness claim; the current backlog owns readiness.
Point inputs support at most six decimal places and a maximum of 1,000,000,000 points. Arithmetic uses exact integer millionths rather than floating-point subtraction. Candidate sizes 3 and 3 select 6 points and leave 1 point. An 8-point candidate is
deferred. WIP is a per-person pull policy, not a maximum plan count.

Every open story gets a selection disposition and a separate epic disposition.
Known done prerequisites are retained as dependency evidence. Missing epic intent
is an explicit question for `story-map`. Missing size, access, goal, calendar, or
capacity stays visible; missing capacity means no commitments. Future groups are
forecasts with no promised dates or automatic assignments.

## Save or replace the JSON and Markdown pair

```bash
story plan --from request.json --output docs/planning/sprint.json
```

This writes `sprint.json` and `sprint.md`. Both destinations must be safe relative
paths under `docs/planning/`, without symlink traversal or overlap with the configured backlog or captured inputs. Existing targets require
reviewed preimages of both files, not just the JSON file:

```bash
sha256sum docs/planning/sprint.json docs/planning/sprint.md
```

After reviewing both contents, put the observed hashes in the request:

```json
{
  "output_preimages": {
    "docs/planning/sprint.json": "<reviewed JSON SHA-256>",
    "docs/planning/sprint.md": "<reviewed Markdown SHA-256>"
  }
}
```

Replace these illustrative placeholders with the exact observed hashes. If one
output is absent, record null for that exact path. Rerun the same output command.
A stale hash refuses replacement. A changed config, repository identity, story,
or backlog membership requires a fresh snapshot and review. Do not rewrite the
source manifest to bypass freshness.

## Interpret the result

`assessment: planned` means a local proposal with the required planning inputs.
It is not sprint-start authority. Missing inputs produce `provisional` and named
reasons. Supplied capacity can produce provisional selected proposals when goal
or calendar evidence is missing; these are not commitments. Selection never makes
a draft or blocked story ready. Tracker sprint and epic publication remain
unsupported. A remote tracker adds `publication.tracker_sprint_fields` to missing
inputs and keeps the proposal provisional. A complete files-only proposal can be
planned; publication still stays local.

The native boundary rejects malformed data, future schemas, duplicate identities,
dependency cycles, unknown candidates, invalid capacity, stale sources, and unsafe
paths. Success exits 0. Conflicts exit 2 with a cause-specific JSON diagnostic.
Both outputs are checked and staged before replacement. Unexpected publication
failures attempt restoration and report failures; a process kill is not an atomic
two-file transaction. Inspect both targets before recovery. No tracker calls occur.

## Verify the installed paths

```bash
cargo test --offline --locked --manifest-path native/story-push/Cargo.toml --target-dir /tmp/ST018-native-target --test planning_contract --test planning_install
```

The Rust process suites execute the actual native binary. Installation tests run
the retained Node installer into disposable agent homes, read both skill entry
points and their shared shipped contract, and execute the documented plan commands.
The scoped Rust audit and observed RED/GREEN evidence are in
[verification.md](./verification.md). Story acceptance and integration remain the
parent coordinator's responsibility.
