# Native planning request reference

`story plan` creates local version-1 proposals. It does not start sprints, change
story status or estimates, synchronize trackers, or write authoritative history.
The source backlog and settings remain authoritative. Run from the project root.
The planner uses the current settings path or legacy `.story-time.json`, then its
`backlogDir` and `prefix`. It ignores environment and retained-CLI path overrides.
Two settings files conflict. With no settings, compatibility defaults are
`backlog` and `ST`; configuration remains a named missing input.

## Native prerequisite and complete request

Install from a Story Time source checkout, not from a copied skill directory:

```bash
cargo install --path native/story-push --locked
```

The skill installer does not install the binary. No `story` command needs Node.

These commands produce and execute a complete, valid request without placeholders:

```bash
story plan --snapshot > request.json
story plan --from request.json
```

Snapshot writes JSON to stdout only. Its request has schema 1, `plan_id` set to
`review-required`, current source preimages, every open ID in source-ID order,
null goal/calendar/capacity, and empty evidence, forecasts, and output preimages.
It is runnable without edits and produces a provisional inventory. The default
order is not a semantic priority ranking. Review and change it before selection.
Keep requests outside the configured backlog. Save them outside the repository
when their retention is not required.

The [JSON schema](./planning-request.schema.json) is shipped with this reference.
The native boundary also checks arithmetic, references, cycles, paths, and hashes.

## Request fields

| Field | Type and meaning |
| --- | --- |
| `schema_version` | Required integer `1`. Unknown versions fail. |
| `plan_id` | Required nonempty repository-scoped identifier, not an active sprint ID. |
| `source` | Required snapshot object. Preserve its exact repository identity, config path/hash, backlog path, and sorted path/hash membership. |
| `candidate_order` | Required unique open IDs in reviewed priority order. Omitted open IDs still get dispositions, but cannot be selected. |
| `goal` | Supplied string or null. Blank or absent is missing, never inferred from titles. |
| `calendar` | Null or `{start, end, time_zone}`. Each value can be null; dates use `YYYY-MM-DD`, and end cannot precede start. Time zone is supplied text, not a holiday or working-day calculation. |
| `capacity` | Null or `{budget, reserve, unit, source}`. Nonnegative points up to 1,000,000,000 with at most six decimal places, reserve <= budget, unit `points`, and nonempty evidence source. |
| `epic_evidence` | Optional map from open ID to `{disposition, epic, evidence}`. Disposition is `mapped`, `justified-standalone`, or `unresolved-epic`. Mapped requires an epic ID; standalone requires null epic and no existing link. Evidence is nonempty text. |
| `exclusions` | Optional map from open ID to a nonempty access, permission, or eligibility reason. Blocks selection and names the missing evidence in a provisional result. |
| `forecasts` | Optional list of `{label, ids}` for later groups. IDs must be unique, open, and not selected. Output marks them as forecasts without commitment or dates. |
| `output_preimages` | Optional map from the exact two output paths to reviewed SHA-256 strings, or null for reviewed absence. Used only with `--output`. |

All request objects reject unknown fields. There is no readiness override or
story-record payload. Every source status and estimate comes from the hashed
backlog, including terminal records used as completed dependency evidence.
An unknown dependency stays blocked and names its ID; a dependency cycle fails.
String or nonpositive sizes fail; absent or null size is unknown and excludes
selection. Draft, refined, and in-progress stories cannot be selected as ready.

`source` contains `repository: {root, head, origin}`, `captured_at`, `config`,
`backlog_dir`, and `files`. `config` is null or `{path, sha256}`; `files` is a
sorted list of `{path, sha256}`. `captured_at` is RFC3339 observation time, not
historical delivery. Root is the canonical current directory. Head is the observed
Git commit or null. Origin is `sha256:<digest>` of the observed remote URL, or
null; credentials in a remote URL are never printed. Git availability must stay
consistent between capture and execution. Configuration contents are not emitted.

All flat Markdown backlog files are hashed, including epic `.map.md` evidence.
Map files do not become stories. Other Markdown files must follow the flat story
format. Nested backlog directories and symlink traversal fail. Original bytes,
custom fields, sections, links, and settings are never rendered back or changed.

## Reviewed planning values

After capture, edit only the planning fields. For example, replace their null or
empty values with the following supplied inputs. These are example values, not
observations about your project:

```json
{
  "plan_id": "checkout-review",
  "candidate_order": ["PAY-001", "PAY-002", "PAY-003"],
  "goal": "Make checkout failures visible",
  "calendar": {
    "start": "2026-11-16",
    "end": "2026-11-27",
    "time_zone": "America/New_York"
  },
  "capacity": {
    "budget": 8,
    "reserve": 1,
    "unit": "points",
    "source": "Team supplied point budget, including reserve"
  },
  "epic_evidence": {
    "PAY-001": {
      "disposition": "mapped",
      "epic": "EP-1",
      "evidence": "Reviewed EP-1 outcome in work/EP-1.map.md"
    },
    "PAY-002": {
      "disposition": "justified-standalone",
      "epic": null,
      "evidence": "Independent maintenance outcome"
    }
  },
  "exclusions": {},
  "forecasts": []
}
```

Preserve the complete captured `source` object in the actual request. This
partial field example does not replace it. Use IDs and evidence from your backlog.
The planner accepts reviewed epic evidence; it does not judge product intent.
`story-map` owns epic definitions, and `story-ready` owns readiness.

## Artifact and arithmetic

JSON output contains `schema_version`, `plan_id`, `source`, `goal`, `calendar`,
`capacity`, `dispositions`, `selected`, `dependency_order`, `assessment`,
`missing_inputs`, `forecasts`, `publication`, and `wip_policy`.

Every open source ID has exactly one disposition. `selection` is `selected`,
`deferred`, `blocked`, or `needs-refinement`. `epic.disposition` is separate.
Without reviewed epic evidence, the result records the existing link or an
unresolved epic question. It never fabricates an epic definition.

Candidates are considered in request order. Only source-ready, known-size
candidates with already-done dependencies and no exclusion can fit the remaining
budget. Validated point values use exact integer millionths for arithmetic. Story sizes share the six-decimal and 1,000,000,000-point bounds; invalid precision fails instead of rounding. Selected points plus reserve never exceed the point budget. Budget 8,
reserve 1, and ordered sizes 3, 3, 8 select the first two. Total is 6 and remaining
capacity is 1. WIP 1 does not reduce the plan to one story.

Missing goal, calendar fields, capacity, configuration, required size, or supplied
eligibility evidence makes the result provisional with explicit field/reason rows.
Missing capacity selects no work. With supplied capacity but missing goal or dates,
selected rows are provisional proposals, not commitments. Zero budget selects none.
`planned` is a local proposal with the required planning inputs, not a started
sprint. For a configured remote tracker, unsupported sprint fields add the named
missing input `publication.tracker_sprint_fields` and keep the plan provisional,
even with complete local capacity and calendar. A files-only plan can be planned.
No permissions to assign a tracker sprint are requested because this command never
assigns one.

## Safe output and freshness

Without `--output`, the command writes JSON to stdout only. With an explicit path,
it writes JSON plus the same path with `.md` under `docs/planning/`:

```bash
story plan --from request.json --output docs/planning/sprint.json
```

Outputs must stay outside the configured backlog and cannot replace captured inputs, even with matching reviewed hashes. Review both existing artifacts before replacement. Supply both exact hashes:

```json
{
  "output_preimages": {
    "docs/planning/sprint.json": "<reviewed JSON SHA-256>",
    "docs/planning/sprint.md": "<reviewed Markdown SHA-256>"
  }
}
```

Use `sha256sum docs/planning/sprint.json docs/planning/sprint.md` to read the
current hashes. These placeholders are not runnable hashes. If one file is absent,
use null for that path and the exact hash for the existing file. A new pair needs
no overwrite evidence. Providing only one preimage never permits replacement.

Relative paths cannot escape the current root or traverse symlinks. Both targets
are checked before publication, then rechecked with source preimages under an
exclusive per-output-directory lock. Both files stage before either rename.
A predictable input, Markdown conflict, or stage failure never replaces JSON.
A failed second rename attempts to restore the first target's original bytes and
reports restoration failures. This is not a crash-atomic two-file transaction.
After a process kill, inspect both artifacts and any stale `.story-plan.lock`
before recovery. Do not remove a lock while its owning command is running.

Changed configuration, repository identity, source bytes, or backlog membership
fails with `stale_source`. Capture a fresh request and review its differences.
Never copy old hashes into a fresh request to bypass review.

Success exits 0. Input and publication conflicts exit 2 with JSON `status:
conflict`, a cause-specific `diagnostic`, and a recovery instruction. Help is text.
No branch invokes the push gateway, contacts a tracker, or writes backlog/settings.
Inspect the named targets after unexpected output I/O or cleanup errors; a
reported failure is not proof of successful rollback.
