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
| `story-setup` | "set up story time" | One-time setup: detects repo, tracker, stack, and test tooling, asks the rest (including whether to push to the tracker), and writes `.story-time/settings.json`. |
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
| `state-machine` | "what states can X be in?" | Models an entity lifecycle in `model/state/<entity>.toml` with a design note, checks it for unreachable states and dead ends, and generates a criterion per transition and per refused event. |
| `decision-table` | "decision table" | Writes a rule as a table with finite inputs and a hit policy, checks it for gaps and overlaps, and generates one rule table for a parameterized test. |
| `principle-invest` | (referenced) | The quality bar: INVEST, vertical slices, the 3 Cs, and anti-patterns. |

```
story-setup (once) → idea → story-create → acceptance-criteria → story-estimate → story-ready → story-pick → story-kickoff → build → story-accept
          ├─ too big → story-split      ├─ epic → story-map      └─ unknown → spike
          ├─ a lifecycle → state-machine      └─ a multi-input rule → decision-table   (both feed acceptance-criteria)
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
rules, and whether story-time may push to the tracker), then writes a
validated `.story-time/settings.json`. Commit it.

```
.story-time/
  settings.json   committed: tracker, sync mode, team, stack
  .gitignore      written by story-time: *.env
  .github.env     GH_TOKEN=…              (mode 600, never committed)
  .ado.env        AZURE_DEVOPS_EXT_PAT=…  (mode 600, never committed)
```

```json
{
  "tracker": "github",
  "sync": "ask",
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

## Decisions: code-driven, or Jev with your own key

`story lint` checks rules. `story judge` makes the judgement calls:
- Does the statement describe a need rather than a solution?
- Is the benefit real?
- Is each acceptance criterion testable?
- What type is it (story, bug, spike, chore), and what size?
- If it's too big, which split pattern fits?

```bash
story judge ST-012        # engine: code (no TypeSafe key; ...)  or  engine: jev (jev-1.13.0)
story judge --json        # every open story, structured, with source and confidence
```

| Setup | Decisions |
|---|---|
| No key (default) | **Code-driven.** Deterministic heuristics. Nothing leaves the machine. |
| `story secret set jev`, or `TYPESAFE_API_KEY` exported | **Jev-based.** One batched request per story to Jev (TypeSafe AI's System One model for typed decisions). Each answer comes back as a probability. |
| `STORY_TIME_JEV=off` | Code-driven, even with a key. |

The `story-setup` skill asks whether to bring a key. The key is stored like
the PATs (`.story-time/.jev.env`, mode 600, git-ignored), and the agent never
handles it. You can pin the model with `"jev": { "model": "jev-1.13.0" }` in
`settings.json`, and the CLI rejects any other `jev` key, so a key can't end
up in that file.

The Jev client follows the contract your org already runs in production
(`varsity-engine`, `lib/pdca/jev-*`):
- **Endpoint.** It talks only to `https://api.typesafe.ai`, with a Bearer key. It refuses redirects,
  because a redirect would re-send the request body to another origin.
- **Retries and auth.** Requests time out after 10 s. 408, 429, 5xx, timeouts and transport errors get three attempts, honouring `retry-after`.
  A 401 or 403 switches Jev off for the rest of the run, with one warning.
- **Strict decoding.** A missing or extra answer, a wrong type, or an out-of-range probability
  rejects the whole response. Nothing is defaulted.
- **Canary.** A contradiction canary runs before the first real call, so a model that answers
  everything alike is caught.
- **What is sent.** Only a story's title, persona, statement and criteria. Any story containing an
  email address, a phone number, or a token-shaped string is not sent at all.
- **Fallback.** Whenever Jev can't answer, the code engine decides, and the judgement says
  `fallback` and why. A judgement is never left half-made.

## Copy-paste cards

Skills that create or change a story end with a copyable card. You can
also run it yourself:

```bash
story export ST-012                    # format follows the tracker in .story-time/settings.json
story export ST-012 --format ado       # one copyable block per ADO field
story export ST-012 --command          # the gh issue create / az boards work-item create command
```

| Format | What you get |
|---|---|
| `github` | Title, labels (`story`, `size:3`, `value:8`, `epic:EP-2`), and an issue body with a criteria checklist |
| `ado` | Work Item Type, Title, Description, Acceptance Criteria, Story Points or Effort, Tags, and Area/Iteration Path. Mapped to the Agile, Scrum, or Basic process template; bugs use Repro Steps |
| `gitea` | The same card as `github` (Gitea renders the same markdown and labels) |
| `markdown` | A neutral card for Jira, Linear, or anywhere else |

## Pushing to GitHub, Gitea, and Azure DevOps

Pushing is opt-in per project, with `sync` in `settings.json`:

| `sync` | Behaviour |
|---|---|
| `off` (default) | Never push. Skills hand back cards, and `story push` refuses to run |
| `ask` | After each create, edit, or split, the skill asks before pushing |
| `auto` | Skills push right after each change |

```bash
story push ST-012 ST-013     # create, or update if already linked
story push --all --dry-run   # show the plan; touch nothing
```

- **Linking.** Created items are linked back into the story (`remote`,
  `remote_url`), so the next push updates the same item instead of making a duplicate.
- **Gitea.** Set `"gitea": { "baseUrl", "owner", "repo" }` (the web URL, not
  the SSH remote). story-time calls the Gitea REST API v1 directly. Gitea has
  no close reason, so "not planned" is the `not-planned` label.
- **Status.** `done` closes the item (completed / Closed / Done).
  `archived` and `split` close it as not planned or Removed. Active
  statuses never change the ADO board state.
- **Labels.** GitHub and Gitea labels that story-time owns (`story`, `size:*`, `value:*`,
  `epic:*`, and on Gitea `not-planned`) are kept current. Your team's own labels are left alone.
- **Rollback.** A push is a saga: if any step fails, items created in this
  run are closed (GitHub, Gitea) or sent to the recycle bin (ADO), and local files
  are restored.

### Tokens (PATs)

An existing `gh auth login` / `az login`, or `GH_TOKEN` / `GITEA_TOKEN` /
`AZURE_DEVOPS_EXT_PAT` already in your environment, is used first, and no
file is needed. Otherwise, run this yourself in a terminal:

```bash
story secret set github      # or: story secret set gitea / ado (hidden prompt, or pipe it on stdin)
story secret status          # shows only the last 4 characters
```

- The token is saved to `.story-time/.<tracker>.env` with mode 600, and
  `.story-time/.gitignore` ignores `*.env`.
- `story` refuses to load a token file that is group- or world-readable, or
  that is tracked by git. In the git case, it tells you to rotate the token.
- Tokens reach `gh` / `az` only through environment variables, never as
  arguments, so they don't appear in process lists or shell history. The
  Gitea token goes only in the REST `Authorization` header.
- The agent never sees the token: the skills tell it not to ask for one or
  accept one in chat.

Use a fine-grained PAT. For GitHub: Issues read/write on the one repo.
For ADO: Work Items (Read & write), with an expiry date.

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
story push ST-001 --dry-run
story secret status
story judge ST-001 --json
story machine check model/state/enrollment.toml      # exit 1 naming each defect
story machine criteria model/state/enrollment.toml   # one Given/When/Then per transition and per refused event
story table check model/rules/promotion.toml         # uncovered combinations, overlapping rules
story table criteria model/rules/promotion.toml      # one rule table for a parameterized test
story init --from answers.json   # usually done for you by story-setup
story config
```

## Running a whole project on it

[docs/playbook.md](docs/playbook.md) shows how to run a repository so that every change, by a
person or an agent, starts as a story and ends with evidence. It covers the mandates to paste into
`CLAUDE.md` or `AGENTS.md`, the CI checks that enforce them (`story-link`, `story-ids`), intake
stories from feeder repositories, and how agents run batches against the backlog.

## Design

The CLI under `skills/story-time/scripts/` is a small hexagonal codebase:

```
src/
  domain/        story.mjs (format), invest.mjs (lint rules), wsjf.mjs (ranking), judgement.mjs
                 (typed questions and answers), code-decision-engine.mjs (heuristics), toml.mjs (the
                 TOML subset the models use), state-machine.mjs and decision-table.mjs (checks,
                 criteria, diagram). Pure, no I/O.
  ports/         story-repository.mjs (StoryRepository), tracker-gateway.mjs (TrackerGateway),
                 decision-engine.mjs (DecisionEngine).
  adapters/      memory/fs story repositories; memory, GitHub (gh), Gitea (REST), and ADO (az boards) tracker
                 gateways; process-exec.mjs (no shell); file-secret-store.mjs (0600, git checks);
                 jev/ (client, egress gate, JevDecisionEngine).
  saga/          saga.mjs. Ordered steps with compensations, rolled back in reverse on failure.
  config/        config.mjs parses .story-time/settings.json (pure); secrets.mjs holds token helpers.
  domain/export.mjs  Card and create-command exporters (github, ado, gitea, markdown) in a strategy registry.
  application/   create, update, note, split (a saga), push (a saga), backlog-stats use cases.
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
- **Decision strategies.** `DecisionEngine` has two implementations,
  `CodeDecisionEngine` and `JevDecisionEngine`. `selectDecisionEngine` picks
  one by whether a key is present. The Jev engine wraps the code engine as its
  fallback, so callers never branch on which one they got.
- **Tracker gateways.** `TrackerGateway` (create / update / remove) is a second,
  narrower port. The memory, GitHub, and ADO gateways all pass one contract
  suite (`test/tracker-gateway.contract.mjs`). Adapters get an injected
  `exec`, so tests drive a `gh` / `az` simulator. The same simulator also
  runs as fake binaries on PATH for the CLI end-to-end tests.
- **Saga pattern.** `push` runs the remote creates and updates first, then the
  local links. A failure removes the remote items this run created and restores
  the local files. `split` creates N children, then updates the parent.
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
