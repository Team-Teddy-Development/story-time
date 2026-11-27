# Named epic and feature grouping with Jev (ST-214)

Problem: in varsity-core almost every card is in one epic, `VC-CORE`, and
the rest have `epic: null`. A board grouped by epic shows one undifferentiated
bucket, so no one can tell infrastructure work from the Core cutover.
Goal: every open story carries a **named** epic ("Core cutover") and a
**named** feature ("Engine call cutovers"). Jev's `choice` model picks them
from a taxonomy the team writes, and each GitHub issue shows them as custom
issue fields.

The plan is organized in three phases: understand, align, act.

## 1. Structural understanding: what is the system?

### Ontology (nouns and valid states)

| Noun | Shape | Valid states / invariants |
|---|---|---|
| `Taxonomy` | `.story-time/taxonomy.json`: `{ "epics": [Epic] }` | 1..15 epics (1 epic question + 1 feature question per epic ≤ Jev's 16-question batch); names unique, case-folded |
| `Epic` | `{ name, description, features: [Feature] }` | `name` is a human name, 1..50 chars (GitHub label limit minus `epic:`); 1..15 features |
| `Feature` | `{ name, description }` | unique within its epic |
| Story frontmatter | `epic: Core cutover`, `feature: Engine call cutovers` | `feature` requires `epic`; both strings. `epic` keeps its existing key, and its value is now a name, not an id |
| `Placement` | `{ id, epic, feature, epic_confidence, feature_confidence, source, outcome }` | `source` is `jev` or `code`. `outcome` is `assigned`, `unchanged`, `unresolved` (below the threshold or `none_fit`), or `kept` (already set to a taxonomy epic and not `--reassign`) |

### Mereology (parts and blast radius)

New module `native/story-push/src/group/`:

- `taxonomy.rs`: domain. Parses and validates the taxonomy. Pure.
- `questions.rs`: domain. Builds the batched Jev `choice` questions from a story and the taxonomy, and interprets the answers into a `Placement`. Pure.
- `classifier.rs`: the port (`trait Classifier`) and two adapters: `JevClassifier`, which reuses `judge::Client` (retry, canary, egress scan), and `KeywordClassifier`, the code fallback.
- `repository.rs`: `StoryRepository` (`trait`) over `list::get`/`list::save`, plus an in-memory fake for tests.
- `fields.rs`: org issue-field provisioning (`--sync-fields`): plans the create and option changes, then applies them through `Cli::github_rest`.
- `saga.rs`: applies placements as a saga. Each step writes one story and keeps its original bytes, and compensation restores them newest first.
- `mod.rs`: the `story group` command (composition root).

Existing files touched (blast radius):

- `story.rs` and `lenient_story.rs`: `feature` joins `KEY_ORDER`. `tracker_card` emits a `feature:` label and `fields`.
- `gateway.rs`: GitHub only. After create or update, it sets the `Epic` and `Feature` issue field values, and it treats `feature:` as a managed label.
- `judge.rs`: `Client`, `Retry`, `Answer` and `JEV_ENDPOINT` become `pub(crate)`. Behavior does not change.
- `lib.rs`, `help.rs`, `main.rs`: dispatch and usage. `skills/story-time/SKILL.md` and `skills/story-map/SKILL.md` gain docs.

Not touched: ADO, Gitea and MCP tools (follow-up stories).

### Topology (wiring and data flow)

```
story group [id...] [--apply] [--reassign] [--min-confidence 0.7] [--json]
  settings ─┐
  taxonomy ─┼─► StoryRepository.open_stories ─► Classifier.place(story) ─► Placement[]
  jev key ──┘        (JevClassifier → POST api.typesafe.ai/v1/systemone, 1 call per story;
                      KeywordClassifier when no key or Jev fails)
  --apply ─► Saga: for each assigned placement, set epic/feature + changelog line
                   (failure → restore every written file, report the step)
  --sync-fields ─► GET /orgs/{org}/issue-fields
                  ─► POST /orgs/{org}/issue-fields            (create Epic / Feature, single_select)
                  ─► PATCH /orgs/{org}/issue-fields/{id}      (existing options kept by id + missing appended)
  story push ─► Gateway(GitHub): labels epic:<name>, feature:<name>
                ─► GET /orgs/{org}/issue-fields                (field ids; cached per run)
                ─► GET  repos/{repo}/issues/{n}/issue-field-values
                ─► POST repos/{repo}/issues/{n}/issue-field-values {issue_field_values:[{field_id,value}]}
                ─► DELETE …/issue-field-values/{field_id}      (only Epic/Feature, only when cleared)
```

### Semiotics (names and contracts)

- Command: `story group`. Default is a dry run; `--apply` writes.
- Changelog line: `set epic=Core cutover, feature=Engine call cutovers — grouped by jev-1.14.0 (epic 0.99, feature 0.91)`.
- JSON report: `{ engine: {mode, model?, why}, placements: [Placement], summary: {assigned, unchanged, unresolved, kept} }`.
- Jev option keys are slugs (`core_cutover`), and their criteria text is `"<Name>: <description>"`. A reserved `none_fit` option is added to every question.
- GitHub field names are fixed: `Epic` and `Feature`, both org-level `single_select`. `story group --sync-fields` creates them and appends the missing options (color `blue` for epics, `purple` for features; priority is the taxonomy order). It never deletes an option. Feature names must be unique across the whole taxonomy, because they share one field.
- When a repository owner has no `Epic`/`Feature` field, push sets labels only. That is opt-in, not an error.
- Refusals name what to fix: `taxonomy: epic "X" listed twice`, `GitHub issue field "Epic" has no option "Recruit portal"; add it in the organization's issue fields`.

## 2. Alignment and verification: what is true, and what matters?

### Epistemology (proof)

Facts verified before planning:

- Jev's `choice` model separates the bucket. `ask_jev` put VC-043 in `platform_infra` (1.00) and VC-073 in `core_cutover` (0.99).
- Team-Teddy-Development already has org issue fields (`Priority` and `Effort`, both single-select). The owner is an admin of the Varsity-Engine and Team-Teddy-Development orgs.
- GitHub's published OpenAPI description (github/rest-api-description) has these endpoints. `POST /orgs/{org}/issue-fields` creates a field and needs org admin (`admin:org` for a classic PAT). `PATCH /orgs/{org}/issue-fields/{id}` takes an options array that **replaces** the set, so existing options are sent back with their `id`. Option colors are gray, blue, green, yellow, orange, red, pink or purple, and `name`, `color` and `priority` are required. `POST /repos/{o}/{r}/issues/{n}/issue-field-values` adds values without clearing the others (at most 25; a single-select value is the option name). `DELETE …/issue-field-values/{field_id}` clears one value.

Proof added by red→green tests:

- Taxonomy: valid parse; refusals for duplicates, >15 epics, empty names, names over 50 chars.
- Questions: ≤16 per story, `none_fit` present, the feature is read from the chosen epic's question only.
- Interpretation: below the threshold or `none_fit` → `unresolved`, with no write.
- Saga: an injected failure on the 2nd write restores the 1st file byte for byte.
- CLI contract with a fake Jev HTTP server: dry run writes nothing; `--apply` writes fields and the changelog.
- Field sync with a fake `gh`: creates missing fields, appends only missing options, keeps existing option ids, and is idempotent on a second run.
- Gateway with a fake `gh`: sets the Epic/Feature values, leaves other fields untouched, clears a removed feature, refuses a missing option naming `story group --sync-fields`, and skips when the fields are absent.
- Real exercise: `story group` on this backlog, and `ask_jev` on sampled Varsity cards.

### Axiology (guardrails)

- Dry run by default. A human-set epic that is already in the taxonomy is never overwritten without `--reassign`.
- Nothing is written below `--min-confidence` (default 0.7), so a low-confidence call is never presented as a decision.
- Egress: the existing PII and secret scan runs before every Jev call. The key never goes in settings.
- SOLID: classification, persistence and orchestration sit behind traits, and the command is only the composition root.
- Rust only, per AGENTS.md. Existing `epic:` label behavior is unchanged for stories that have no feature.

### Teleology (target)

See the acceptance criteria in ST-214. In short: a named epic and feature on every confidently placed story, unresolved ones listed for a human, and `Epic` and `Feature` fields set on GitHub issues.

## 3. Execution: how we act

### Praxeology (loop and recovery)

1. Story ST-214, then a kickoff test plan (this doc).
2. Red→green, per slice: taxonomy → questions and interpretation → classifiers → repository and saga → command → frontmatter and card → GitHub fields.
3. Verify: `cargo test --locked`, `npm test`, a real `story group` run, and `ask_jev` sampling of Varsity cards.
4. PR declaring `Stories: ST-214 (#issue)`; story-accept with evidence.

Recovery:
- Jev fails (auth, canary, protocol) → keyword fallback, reported as `code`.
- A saga step fails → compensate and report the step.
- An option is missing on GitHub → the push fails and names `story group --sync-fields`. Field sync is idempotent, so re-running it is the fix.

## Proposed Varsity taxonomy (owner to confirm)

| Epic | Features |
|---|---|
| Platform & infrastructure | Core runtime hosting · Database roles & credentials · Workflow runtime (Temporal) · Observability & recovery budgets |
| Core cutover | Engine call cutovers · Bypass retirement · Cutover rehearsal & rollback |
| Core contracts & domain model | Ontology & identities · Topology & registrations · Access & request context |
| Competition & results | Schools & programs · Schedules & fixtures · Scores & corrections · Standings & statistics |
| Athletes & privacy | Athlete profiles & guardian consent · Personal data export · Personal data erasure |
| Recruit portal | Prospects & private notes · Relationship graph · Recruiting capabilities |
| Ingestion & evidence | Source rights & identity links · Ingestion sweeps · Media reporting index · Claims & accepted facts |
| AI & knowledge | Model invocation & spend limits · Knowledge indexes & retrieval · AI tool bounds |
| Media & social | Score media · Artifact approval · Social posting |
| Notifications | Follow notifications |

## Follow-ups (out of scope)

- MCP `story_group` tool and a `feature` filter on `story_list`.
- ADO (`Tags`) and Gitea mirroring.
- User-owned repositories (`/user/issue-fields`).
- Deleting options for epics removed from the taxonomy.
