# story-time

Agent skills for agile story planning. They work in Claude Code, Codex, and any
harness that reads [Agent Skills](https://agentskills.io)
(`skills/<name>/SKILL.md`).

The layout follows [tstack](https://github.com/t3dotgg/tstack) and
[pstack](https://github.com/backnotprop/pstack): small skills that each do one
job, call each other by name, and hand the deterministic parts to a tested
script. Here the subject is the story lifecycle, from idea to accepted.

## Skills

| Skill | Say | What it does |
|---|---|---|
| `story-time` | "story time" | Entry point. Lifecycle, file format, the `story` CLI, and routing to the right skill. |
| `story-setup` | "set up story time" | One-time setup: detects repo, tracker, stack, and test tooling, asks the rest, and writes `.story-time.json`. |
| `story-create` | "write a story for…" | Turns ideas, bugs, PRDs, or notes into INVEST stories with persona, benefit, criteria, and scope. |
| `story-edit` | "update ST-012" | Changes a story safely: classifies the change, guards done and in-progress work, catches scope creep, logs why. |
| `story-split` | "this is too big" | Splits with SPIDR and the classic patterns into vertical slices, and writes them as an all-or-nothing saga. |
| `acceptance-criteria` | "write the ACs" | Testable Given/When/Then criteria covering happy, failure, boundary, permission, and non-functional cases. |
| `story-estimate` | "planning poker" | Three independent estimators anchored on done stories, converged on one size; WSJF value scoring. |
| `story-ready` | "is this ready?" | Definition of Ready gate: automated INVEST lint plus judgment checks. The only way into `ready`. |
| `backlog-groom` | "groom the backlog" | Health stats, stale, duplicate, oversized, unscored, and blocked stories, and re-ordering. Proposes, then applies on approval. |
| `story-pick` | "what's next?" | WSJF-ranked pick, checked against WIP, capacity, dependencies, and risk, with alternatives. |
| `story-map` | "map this epic" | User story map (backbone → steps → stories) sliced into a walking skeleton and outcome-named releases. |
| `spike` | "we need to investigate" | A time-boxed, single-question spike with exit criteria, which ends in estimated follow-up stories. |
| `story-kickoff` | "start ST-012" | Turns criteria into an ordered red-green-refactor test list, finds ports, adapters, and saga seams, and writes a Test Plan. |
| `story-accept` | "accept ST-012" | Verifies each criterion and the Definition of Done with evidence. Accepts, or rejects with gaps. |
| `principle-invest` | (referenced) | The quality bar: INVEST, vertical slices, the 3 Cs, and anti-patterns. |

```
story-setup (once) → idea → story-create → acceptance-criteria → story-estimate → story-ready → story-pick → story-kickoff → build → story-accept
          ├─ too big → story-split      ├─ epic → story-map      └─ unknown → spike
backlog-groom runs on a cadence; story-edit applies anywhere.
```

Every skill depends on `story-time`, which holds the CLI. Install it with any
of them.

## Install

Requires Node 20+. No npm install: the CLI has zero dependencies.

### Installer (Claude Code, Codex, or both)

```bash
git clone https://github.com/team-teddy-development/story-time
cd story-time
node scripts/install.mjs --agent codex       # Codex       -> ~/.agents/skills
node scripts/install.mjs --agent claude      # Claude Code -> ~/.claude/skills
node scripts/install.mjs --agent all         # both
```

| Flag | Effect |
|---|---|
| `--scope project --project DIR` | Install into a repo instead: `DIR/.agents/skills` (Codex), `DIR/.claude/skills` (Claude Code) |
| `--codex-home DIR` | Use the older Codex location `DIR/skills` (`$CODEX_HOME/skills`) |
| `--dry-run` | Print the plan, write nothing |
| `--force` | Replace story-time skills that are already installed. Without it, the installer refuses and writes nothing |
| `--uninstall` | Remove the story-time skills, and nothing else |

The installer copies the skills without the test suite.

### Codex

The installer above is the simplest route. Restart Codex afterwards, then
call a skill with `$story-time`, `$story-pick`, and so on, or just describe
the task. Each skill carries `agents/openai.yaml` with its Codex display
name, short description, and default prompt. `principle-invest` is
explicit-only, so it loads only when another skill or you reference it.

As a Codex plugin:

```bash
codex plugin marketplace add team-teddy-development/story-time
codex plugin add story-time@story-time
```

Or from inside Codex with the built-in installer: ask
`$skill-installer install the skills under skills/ from team-teddy-development/story-time`.
Install all of them: every skill calls the `story` CLI in `story-time`.

### Claude Code

As a plugin (inside Claude Code):

```text
/plugin marketplace add team-teddy-development/story-time
/plugin install story-time@story-time
```

Or with the installer: `node scripts/install.mjs --agent claude`.

### Any agent

With the [skills CLI](https://skills.sh), which also supports Cursor, Pi,
OpenCode, and others:

```bash
npx skills add team-teddy-development/story-time
```

## Setup: tell it what the project uses

Run the `story-setup` skill once per repo ("set up story time"). It reads the
git remote, `package.json` / `*.csproj` / `pyproject.toml`, and whether `gh` or
`az` is installed. It asks only what it can't detect (where work items live,
ADO process template, velocity, WIP limit, extra Definition of Done or Ready
rules), then writes a validated `.story-time.json`. Commit it.

```json
{
  "tracker": "github",
  "github": { "repo": "acme/shop" },
  "prefix": "ST",
  "team": { "sprintDays": 10, "velocity": 21, "wipLimit": 1 },
  "project": { "stack": ["typescript", "react"], "testFramework": "vitest", "testCommand": "npm test" },
  "definitionOfDone": ["Release note added"]
}
```

Every skill reads it: cards come out in your tracker's format, `story-pick`
respects your WIP limit, `backlog-groom` sizes the ready buffer from your
velocity, `story-kickoff` writes tests for your framework, and
`story-ready` / `story-accept` add your team's rules.

## Copy-paste cards

Skills that create or change a story end with a copyable card. You can
also run it yourself:

```bash
story export ST-012                    # format follows the tracker in .story-time.json
story export ST-012 --format ado       # one copyable block per ADO field
story export ST-012 --command          # the gh issue create / az boards work-item create command
```

| Format | What you get |
|---|---|
| `github` | Title, labels (`story`, `size:3`, `value:8`, `epic:EP-2`), and an issue body with a criteria checklist |
| `ado` | Work Item Type, Title, Description, Acceptance Criteria, Story Points or Effort, Tags, and Area/Iteration Path. Mapped to the Agile, Scrum, or Basic process template; bugs use Repro Steps |
| `markdown` | A neutral card for Jira, Linear, or anywhere else |

Nothing is pushed to a tracker unless you say so. Pasting the card, or
running the printed command with your own signed-in `gh` / `az`, is the
default.

## The backlog

Stories are markdown files in `./backlog/` of the project you're planning,
one per story, with flat YAML frontmatter
([format](skills/story-time/references/story-format.md)). They diff, review,
and merge like code. Teams on GitHub Issues, Jira, or Linear keep their
tracker; the skills act as the adapter
([backends](skills/story-time/references/backends.md)).

```bash
node skills/story-time/scripts/story.mjs help
```

```
story new --title "Reset password via emailed link" --statement "As a …, I want …, so that …." --ac "Given …, when …, then …"
story lint                   # INVEST / Definition of Ready, exit 1 on errors
story rank --limit 5         # WSJF over ready, unblocked stories
story set ST-001 size=3 value=8 time_criticality=5 risk_reduction=3 --note "poker 3/3/5"
story set ST-001 status=ready  # refused unless lint passes
story split ST-004 --from children.json   # saga: all children or none
story stats
story export ST-001 --format github
story init --from answers.json   # usually done for you by story-setup
story config
```

## Design

The CLI under `skills/story-time/scripts/` is a small hexagonal codebase:

```
src/
  domain/        story.mjs (format), invest.mjs (lint rules), wsjf.mjs (ranking). Pure, no I/O.
  ports/         story-repository.mjs. The StoryRepository interface the use cases depend on.
  adapters/      memory-story-repository.mjs, fs-story-repository.mjs.
  saga/          saga.mjs. Ordered steps with compensations, rolled back in reverse on failure.
  config/        config.mjs. Parses and validates .story-time.json (pure; the CLI reads the file).
  domain/export.mjs  Card and create-command exporters (github, ado, markdown) in a strategy registry.
  application/   create, update, note, split (a saga), backlog-stats use cases.
story.mjs        Composition root and CLI boundary. Parsing, wiring, output only.
```

- **Single responsibility.** Each INVEST rule is its own object.
  `lintStory` only composes them.
- **Open/closed.** Exporters are a registry: a new tracker format is a new
  entry, and `renderCard` doesn't change. Lint rules work the same way: pass extra rules to `lintStory({ rules: [...DEFAULT_RULES, mine] })`
  instead of editing the linter.
- **Liskov.** Both adapters run the same repository contract suite
  (`test/repository.contract.mjs`). A new adapter (GitHub, Jira) must pass it too.
- **Interface segregation.** The port is five methods. Use cases take only
  the repository.
- **Dependency inversion.** Use cases depend on the port. Only `story.mjs`
  picks a concrete adapter.
- **Repository pattern.** Storage stays behind `StoryRepository`. The
  filesystem adapter writes atomically (temp file, then rename) and rejects
  path-escaping ids.
- **Saga pattern.** `split` creates N children, then updates the parent.
  Any failure compensates the completed steps in reverse. A compensation
  that fails is reported, and does not stop the others from running. Tracker
  backends follow the same discipline by hand.

## Development

The library was built red-green: each behaviour's test was committed failing
before its implementation (see `git log`). Keep it that way.

```bash
npm test
```

Two suites:

- `skills/story-time/scripts/test/`: domain, repository contract, saga,
  use cases, CLI end-to-end.
- `test/install.test.mjs`: the installer, against a temp `$HOME`, for both agents.
- `test/skills.test.mjs`: the skill library itself. Expected skill set, valid
  frontmatter with a "Use when" trigger, Codex `agents/openai.yaml`, links that resolve, cross-references
  to skills that exist, length limits, README coverage, Claude and Codex plugin manifests.

To add a skill, add its name to `EXPECTED` in `test/expected-skills.mjs` first
(red), then write `skills/<name>/SKILL.md` and `skills/<name>/agents/openai.yaml` (green).

## License

[MIT](LICENSE)
