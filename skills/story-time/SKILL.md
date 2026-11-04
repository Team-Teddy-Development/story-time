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

backlog-groom runs on a cadence over the whole backlog. story-edit changes any story at any stage.
```

| Request | Skill |
|---|---|
| "set up story time", first use in a repo | **story-setup** skill |
| "write a story for…", "turn this into stories" | **story-create** skill |
| "change / update / rewrite ST-012" | **story-edit** skill |
| "this is too big", "break this down" | **story-split** skill |
| "write the ACs", "what are the edge cases" | **acceptance-criteria** skill |
| "size this", "planning poker", "score value" | **story-estimate** skill |
| "is this ready?" | **story-ready** skill |
| "groom / refine the backlog" | **backlog-groom** skill |
| "what should I work on next?" | **story-pick** skill |
| "map out this epic / feature" | **story-map** skill |
| "we don't know enough to estimate" | **spike** skill |
| "start ST-012" | **story-kickoff** skill |
| "is ST-012 done?", "accept this story" | **story-accept** skill |

The **principle-invest** skill holds the quality bar all of them apply.

## The `story` tool

Every skill drives the backlog through one zero-dependency Node CLI (Node 20+):

```bash
node <story-time>/scripts/story.mjs <command>
```

`<story-time>` is this skill's directory. Other story-time skills are its
siblings, so from any of them it is `../story-time`. Below, `story` means
that full command. Pass `--json` when you need to parse output.

| Command | Does |
|---|---|
| `story new --title T [--statement S] [--ac A]... [--type story\|spike\|bug\|chore]` | Create a draft with the next id |
| `story list [--status S]` | List stories |
| `story show <id>` | Print one story |
| `story lint [id...]` | INVEST / Definition of Ready check. Exit 1 if any story has errors |
| `story rank [--limit N] [--include-refined]` | WSJF ranking of ready, unblocked stories, plus why others were excluded |
| `story set <id> key=value... [--note N]` | Update fields with a changelog entry. `status=ready` is lint-gated |
| `story note <id> "why"` | Log a prose edit (sections) in the changelog |
| `story split <id> --from children.json` | Split into children atomically. A failure rolls everything back |
| `story stats` | Counts, points, unestimated and stale stories |
| `story export <id> [--format github\|ado\|markdown] [--command]` | Copy-paste card for the tracker, or the `gh` / `az boards` command that creates it |
| `story init --from answers.json` / `story config` | Write / show the project config (`.story-time.json`) |

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

`.story-time.json` at the project root records what the project uses: the
tracker (`files`, `github`, `ado`, `jira`, `linear`) and its coordinates,
the id prefix, the backlog folder, the team's sprint length, velocity, and
WIP limit, the stack and test command, and any extra Definition of Ready
or Done rules. The **story-setup** skill writes it.

Run `story config --json` at the start of any story-time work and use what
it says. Never ask the user for something the config already answers. If no
file exists, the defaults apply (files backlog, `ST` ids, WIP 1). Offer
the **story-setup** skill once, then carry on.

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
- Finish by running `story lint` on what you touched and reporting the verdict.
