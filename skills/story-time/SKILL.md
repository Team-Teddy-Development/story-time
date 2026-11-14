---
name: story-time
description: Entry point for the story-time agile planning skills. Explains the story lifecycle, the backlog file format, and the `story` CLI every other story-time skill uses, and routes a request to the right skill. Use when the user says "story time", asks what planning skill to use, or starts any backlog work and no more specific story-time skill fits.
---

# Story Time

A small, opinionated agile planning loop. Each skill does one job. All of them
read and write the same backlog through the same tool, so the output of one is
the input of the next. Start a new project with the **story-setup** skill.

## Lifecycle

```
idea ──► story-create ──► acceptance-criteria ──► story-estimate ──► story-ready ──► [ready]
            │                                         │                                  │
            ├─ too big? ──► story-split ◄─────────────┘                                  │
            ├─ an epic? ──► story-map                                                    ▼
            └─ unknown? ──► spike                             story-pick ──► story-kickoff ──► build ──► story-accept ──► [done]

project-discovery writes a project brief before backlog work and hands proposed epics to story-map.
refinement owns single-story and backlog modes; backlog-groom is its compatibility alias.
gap-sweep finds work with no story and hands approved rows to story-create.
sprint-planning consumes ordered ready candidates and produces local proposals.
flow-review reports blocked, aging, and over-WIP work and sends pulls to story-pick.
sprint-review records the goal outcome, links feedback, and decides unfinished work; it marks no story done.
release-planning writes a release manifest and uses only approved deployment capabilities; it can run before sprint review.
retrospective revisits previous improvement actions and proposes at most three owned ones; it changes no policy.
story-edit changes any story at any stage.
state-machine and decision-table write checked models that acceptance-criteria and story-kickoff build on.
```

| Request | Skill |
|---|---|
| "set up story time", first use in a repo | **story-setup** skill |
| "start a project", "project discovery", no stated goals | **project-discovery** skill |
| "write a story for…", "turn this into stories" | **story-create** skill |
| "change / update / rewrite ST-012" | **story-edit** skill |
| "this is too big", "break this down" | **story-split** skill |
| "write the ACs", "what are the edge cases" | **acceptance-criteria** skill |
| "size this", "planning poker", "score value" | **story-estimate** skill |
| "is this ready?" | **story-ready** skill |
| "refine ST-012", "groom / refine the backlog" | **refinement** skill |
| "backlog-groom" | **backlog-groom** skill, alias to refinement |
| "sweep for gaps", "what are we missing" | **gap-sweep** skill |
| "plan a sprint", goal and capacity planning | **sprint-planning** skill |
| "flow review", "what is stuck?", "are we over WIP?" | **flow-review** skill |
| "sprint review", "close the sprint" | **sprint-review** skill |
| "plan a release", "what is in this release?" | **release-planning** skill |
| "retrospective", "retro", "how do we work better?" | **retrospective** skill |
| "what should I work on next?" | **story-pick** skill |
| "map out this epic / feature" | **story-map** skill |
| "we don't know enough to estimate" | **spike** skill |
| "start ST-012" | **story-kickoff** skill |
| "is ST-012 done?", "accept this story" | **story-accept** skill |
| "what states can X be in", "model this lifecycle" | **state-machine** skill |
| "the outcome depends on several inputs", "decision table" | **decision-table** skill |

The **principle-invest** skill holds the quality bar all of them apply.

## The `story` tool

Use installed Rust `story` for `init`, `push`, `adopt preview`, `plan`, `config`, `show`, `list`, and `secret`.
The skill installer copies skills only; install the executable separately.
Read `.story-time/settings.json` before selecting a tracker. Install the native
command from the repository root:

```bash
cargo install --path native/story-push --locked
```

`story push` runs entirely in Rust. Do not invoke the historical Node push
handler. Planning, `story config`, `story show`, `story list`, and `story machine` also run entirely in Rust. Other commands
still use Node 20+.
Set `STORY_TIME_LEGACY_CLI` to
this skill's absolute `scripts/story.mjs` path, or configure `legacyCli` in
`~/.config/story-time/runtime.json`. The dispatcher uses that path only for
retained commands only. Plan JSON is the default output; retained commands use `--json`.

The separate Rust `gitea-gateway story-push` interface remains available in
[Gitea Story Time](../../docs/gitea-story-time.md). It takes explicit Markdown
paths, not IDs or `--all`. Install it from the source checkout with
`cargo install --offline --path gitea-gateway`. For example, run
`gitea-gateway story-push --root <project> --dry-run backlog/ST-001.md`, then
remove `--dry-run` for an authorized push. Copied skill installations contain
neither crate nor `docs/`; the skill installer does not install either binary.
For direct Markdown edits, follow [story-format.md](references/story-format.md)
and preserve `remote` and `remote_url`. Historical Node push adapters are not
part of either Rust push execution path.

| Command | Does |
|---|---|
| `story plan --snapshot` | Capture a complete version-1 request with current source identity and hashes; read-only |
| `story plan --from request.json [--output docs/planning/name.json]` | Generate a local JSON proposal; explicit output also writes Markdown, with reviewed hashes required for both replacements |
| `story new --title T [--statement S] [--ac A]... [--type story\|spike\|bug\|chore]` | Create a draft with the next id. In a git repo the id is above every id on all local and remote-tracking branches; `git fetch` first so other branches' ids are seen |
| `story list [--status S]` | List stories |
| `story show <id>` | Print one story |
| `story lint [id...]` | INVEST / Definition of Ready check. Exit 1 if any story has errors |
| `story rank [--limit N] [--include-refined]` | WSJF ranking of ready, unblocked stories, plus why others were excluded |
| `story set <id> key=value... [--note N]` | Update fields with a changelog entry. `status=ready` is lint-gated |
| `story note <id> "why"` | Log a prose edit (sections) in the changelog |
| `story split <id> --from children.json` | Split into children atomically. A failure rolls everything back |
| `story stats` | Counts, points, unestimated and stale stories |
| `story export <id> [--format github\|ado\|markdown] [--command]` | Copy-paste card for the tracker, or the `gh` / `az boards` command that creates it |
| `story push <id...> \| --all [--dry-run]` | Create or update items in GitHub, ADO, or Gitea. Native Rust; refuses when `sync` is `off` |
| `story judge [id...]` | Judgement calls: need vs. solution, real benefit, testable criteria, type, size, split pattern. Jev with a key, code heuristics without |
| `story secret set\|status\|clear <github\|ado\|jev>` | Store a tracker token or the TypeSafe key in `.story-time/.<name>.env` (mode 600, git-ignored). The user runs `set` themselves |
| `story machine check\|criteria\|diagram <file>` | Check a state machine (`model/state/<entity>.toml`) for unreachable states, dead ends, transitions without an event, and unknown names; print one criterion per transition and per refused event; or print its Mermaid diagram. Exit 1 on defects |
| `story table check\|criteria <file>` | Check a decision table for uncovered input combinations and, under `unique`, overlapping rules; or print one criterion with a row per combination for a parameterized test. Exit 1 on defects |
| `story init --from answers.json` / `story config` | Write / show the project config (`.story-time/settings.json`) |

The [planning request contract](./references/planning.md) defines missing inputs,
capacity arithmetic, forecasts, freshness, and safe output. Plans never change
story status or tracker sprint fields. WIP limits pull, not total plan size.

The backlog lives in `./backlog/` (override with `--dir` or `$STORY_TIME_DIR`),
one markdown file per story. Ids default to `ST-001`; set `--prefix` or
`$STORY_TIME_PREFIX` for team prefixes. The file format is in
[story-format.md](references/story-format.md). Edit section prose directly in the
file and log the edit with `story note`. Change frontmatter fields with `story set` so
the change is validated and logged.

If the team keeps its backlog in GitHub Issues, Jira, or Linear, follow
[backends.md](references/backends.md). The skills stay the same; only the
storage adapter changes.

## Project config

`.story-time/settings.json` at the project root (committed) records what
the project uses: the tracker (`files`, `gitea`, `github`, `ado`, `jira`, `linear`)
and its coordinates, whether to push to it (`sync`: `off`, `ask`, `auto`),
the id prefix, the backlog folder, the team's sprint length, velocity, and
WIP limit, the stack and test command, and any extra Definition of Ready
or Done rules. The **story-setup** skill writes it.

Tokens never go in settings. They live in `.story-time/.github.env` /
`.story-time/.ado.env` (git-ignored, mode 600), written only by the user
running `story secret set`. Never ask for a token, and never handle one.

Run `story config --json` at the start of any story-time work and use what
it says. Never ask the user for something the config already answers. If no
file exists, the defaults apply (files backlog, `ST` ids, WIP 1). Offer
the **story-setup** skill once, then carry on.

## Decisions: code or Jev

`story lint` is rules. `story judge` is judgement, through one
`DecisionEngine` port with two strategies:

- **No TypeSafe key: code-driven.** Deterministic heuristics. Nothing leaves
  the machine.
- **A key (`story secret set jev`, or `TYPESAFE_API_KEY`): Jev-based.** Each
  story's decisions go to Jev in one batched request, and come back as
  probabilities. If Jev can't give a trustworthy answer, the code heuristics
  decide and the judgement says `fallback` and why. That covers an outage, a
  rejected key, a failed contradiction check, a story carrying personal data
  or a secret, or a malformed answer.

Treat a judgement as evidence, not a verdict. Low confidence (under 0.5),
or a `fallback`, means you apply judgement yourself, per the
**principle-invest** skill. Never present a code-driven decision as Jev's.

## Statuses

`draft` → `refined` → `ready` → `in-progress` → `done`. Side exits: `split`
(replaced by children) and `archived` (dropped). Only the **story-ready** skill
moves a story to `ready`. Never pass `--force` to `story set` unless the user
explicitly asks to bypass the gate.

## Scoring scale

Size, value, time criticality, and risk reduction all use the modified
Fibonacci scale `1 2 3 5 8 13 20`. They are relative within a backlog, not
hours. WSJF = (value + time_criticality + risk_reduction) / size.

## Rules for every story-time skill

- Write the story in the user's domain language, never the codebase's.
- Do not make up business value or deadlines. Ask. If nobody can answer,
  propose a number, label it as an assumption in the changelog note, and say so.
- Show the user what you will write before a batch of writes, splits,
  archives, or merges. Single-story creates and edits can go ahead without asking.
- For every tracker, run `story lint` on what you touched and report the
  readiness verdict. For Gitea, also run the Rust `story-push --dry-run` with
  the changed Markdown paths. Report parsing and rendering separately from
  INVEST readiness; the dry run does not replace local lint.
