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
| `project-discovery` | "start a project" | Writes `docs/project-brief.md`: validated versus assumed facts, open decisions with owners, and proposed epics traced to an outcome or marked unresolved. No estimates or schedules; epics go to story-map. |
| `story-create` | "write a story for…" | Turns ideas, bugs, PRDs, or notes into INVEST stories with persona, benefit, criteria, and scope. |
| `story-edit` | "update ST-012" | Changes a story safely: classifies the change, guards done and in-progress work, catches scope creep, logs why. |
| `story-split` | "this is too big" | Splits with SPIDR and the classic patterns into vertical slices, and writes them as an all-or-nothing saga. |
| `acceptance-criteria` | "write the ACs" | Testable Given/When/Then criteria covering happy, failure, boundary, permission, and non-functional cases. |
| `story-estimate` | "planning poker" | Three independent estimators anchored on done stories, converged on one size; WSJF value scoring. |
| `story-ready` | "is this ready?" | Definition of Ready gate: automated INVEST lint plus judgment checks. The only way into `ready`. |
| `refinement` | "refine ST-012", "refine the backlog" | One workflow for single-story depth or complete open-story inventory, epic questions, preserved estimates, and ordered ready candidates. |
| `backlog-groom` | "groom the backlog" | Compatibility alias to refinement in backlog mode. |
| `gap-sweep` | "what are we missing?" | Finds work with no story in code, tests, docs, stories, and open GitHub or Gitea issues, plus up to 2 evidence-backed features. Proposes at most 10 rows and hands approved ones to story-create. |
| `sprint-planning` | "plan a sprint" | Goal-based local JSON and Markdown proposals with explicit capacity, completed dependencies, deferred work, and forecast limits. |
| `flow-review` | "what is stuck?" | Read-only report of blocked, aging, and over-WIP work, each with an owner and a next action, plus scope swaps with their capacity effect. Missing sprint and flow data stays `unknown`; pulls go to story-pick. |
| `sprint-review` | "close the sprint" | Records the goal outcome and demo evidence, links each piece of feedback to a backlog item, and gives every unfinished story a disposition: carry over, return to backlog, split, or drop. Keeps original estimates and sprint membership, and marks no story done. |
| `retrospective` | "run a retro" | Revisits every previous improvement action first, then proposes at most three owned actions with the observed problem, proposed change, expected effect, follow-up date, and outcome measure. Writes `docs/retrospectives/<date>.md` and changes no policy; stories go through story-create with approval. |
| `release-planning` | "plan a release" | Writes `docs/releases/<release>.md`: planned, accepted, and released work kept apart, a rollout and recovery plan, and delivery evidence or a specific handoff. Uses only deployment capabilities the repository or user names and claims no release without evidence. |
| `agile-metrics` | "what is our cycle time?" | Throughput, WIP, and cycle time for a date window from the native `story metrics` command. Every figure names its source, window, and units; missing history and velocity without a sprint record stay `unknown`. |
| `project-status` | "project status report" | Writes `docs/status/<date>.md`: accepted, released, blocked, and forecast work kept apart, with risks, decisions, owners, and next actions. Every status claim links to its record (story section, release manifest, `story metrics`, PR); a claim with no record is `unknown`. Changes no story. |
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
project-discovery writes a project brief for a project with no direction and hands proposed epics to story-map.
refinement runs in single-story or backlog mode; backlog-groom is its alias.
gap-sweep finds work with no story and hands approved rows to story-create.
sprint-planning consumes ordered ready stories; flow-review reports stuck work; sprint-review closes the sprint; release-planning ships accepted work with evidence; retrospective turns delivery problems into at most three owned actions; agile-metrics reports delivery figures; project-status reports status with a record for every claim; story-edit applies anywhere.
```

Every skill depends on `story-time`, which holds the CLI. Install it with any
of them.

## Install

Native `story init`, `story push`, `story adopt preview`, `story plan`,
`story config`, `story show`, `story list`, `story lint`, `story stats`, `story secret`, `story issues`, `story note`, `story set`, `story split`, `story new`, `story rank`, `story machine`, `story mcp`, `story export`, `story table`, `story judge`, `story help`, and `story metrics` require Rust and Cargo. No command requires Node.

To install a release on a machine in one step, run this from the repository root:

```bash
cargo run --locked --manifest-path native/story-push/Cargo.toml --bin story-install -- --agent all
```

It exports the checkout's HEAD commit to `~/.local/share/story-time/releases/<commit>`.
It saves the current `~/.local/bin/story` and `~/.config/story-time/runtime.json`.
Then it builds `story` from the export, installs the skills for the chosen
agents, and removes `runtime.json`, which no command reads. Uncommitted changes are not
installed. Use `--dry-run` to print the plan. Use `--rollback` to restore the
saved binary and `runtime.json`.

To install by hand instead, install the native executable:

```bash
cargo install --path native/story-push --locked
```

`story list` runs in Rust and needs no Node.
Use `story push`, not `node story.mjs push`. Push planning, tracker operations,
and local linking run in Rust. An empty `story push --all --dry-run --json`
returns an empty plan without credentials or network access. Sync off still
rejects push. See [native push](docs/native-story-push.md) for verification,
installation recovery, and the migration boundary.

`story init --from answers.json --config settings.json --json` validates
answers before writing settings or secret-ignore metadata. Omit `--from` to
use defaults. Use `--force` to replace existing settings, including malformed
settings. The target is `--config`, then `STORY_TIME_CONFIG`, then
`.story-time/settings.json`. Relative paths start at the current directory.
A target directly inside `.story-time` uses that directory's parent as the
project root. Other targets use the settings directory. Init creates `.story-time/.gitignore`
at that root and preserves existing ignore rules. Init and its help work
without Node or retained-runtime configuration.

### Installer (Claude Code, Codex, or both)

The skill installer is the Rust binary `story-install-skills`. Run it from a
checkout with Cargo; it needs no Node.

```bash
git clone https://github.com/team-teddy-development/story-time
cd story-time
cargo run --locked --manifest-path native/story-push/Cargo.toml --bin story-install-skills -- --agent codex   # Codex       -> ~/.agents/skills
cargo run --locked --manifest-path native/story-push/Cargo.toml --bin story-install-skills -- --agent claude  # Claude Code -> ~/.claude/skills
cargo run --locked --manifest-path native/story-push/Cargo.toml --bin story-install-skills -- --agent all     # both
```

Add `--help` after `--` to print its usage.

| Flag | Effect |
|---|---|
| `--scope project --project DIR` | Install into a repo instead: `DIR/.agents/skills` (Codex), `DIR/.claude/skills` (Claude Code) |
| `--codex-home DIR` | Use the older Codex location `DIR/skills` (`$CODEX_HOME/skills`) |
| `--dry-run` | Print the plan, write nothing |
| `--force` | Replace story-time skills that are already installed. Without it, the installer refuses and writes nothing |
| `--uninstall` | Remove the story-time skills, and nothing else |
| `--source DIR` | Copy the skills from the story-time checkout or release at `DIR` instead of the checkout the binary was built from |

The installer copies the skills without the test suite. It does not install the
native executable. Install native `story` separately before using planning skills.

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

Or with the installer, from a checkout:
`cargo run --locked --manifest-path native/story-push/Cargo.toml --bin story-install-skills -- --agent claude`.

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
respects your WIP limit, `refinement` records the ready-buffer evidence or its
missing inputs, `story-kickoff` writes tests for your framework, and
`story-ready` / `story-accept` add your team's rules.

## Refinement and sprint proposals

Use `refinement` for a named story or the backlog. `backlog-groom` reaches the
same workflow. `story-map` owns epic definitions; `story-ready` owns readiness.
Use `sprint-planning` for a supplied goal, calendar, point budget, and reserve.

```bash
story plan --snapshot > request.json
story plan --from request.json
story plan --from request.json --output docs/planning/sprint.json
```

Snapshot emits a complete runnable request with current repository/config/backlog
hashes. Review its planning fields before selection. Missing capacity produces a
provisional plan with no commitments. WIP limits concurrent pull, not total plan
size. Later groups remain forecasts. Output is local-only; no sprint starts or
tracker writes occur. Replacing an artifact requires exact reviewed hashes of both
JSON and Markdown outputs. Source freshness and safe-path checks fail before
replacement. See [generate a local proposal](docs/planning/README.md) for the JSON
schema, runnable request example, arithmetic, overwrite usage, and recovery limits.

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

## MCP server

`story mcp` serves a repository's stories over MCP on stdio. Register it with your agent. Replace `<repo>` with the repository path. The server is read-only unless you pass `--write`.

Claude Code, read-only:

```sh
claude mcp add --transport stdio --scope local story-time -- story mcp --root <repo>
```

Claude Code, with writes:

```sh
claude mcp add --transport stdio --scope local story-time -- story mcp --root <repo> --write
```

Claude Code asks before each write tool unless you allow it.

Codex, read-only:

```sh
codex mcp add story-time -- story mcp --root <repo>
```

Codex, with writes:

```sh
codex mcp add story-time -- story mcp --root <repo> --write
```

Codex asks before a write only when `default_tools_approval_mode = "writes"` is set. With its default mode, a write ran with no prompt (verified on Codex 0.157.1). `codex mcp add` creates the `[mcp_servers.story-time]` table in `~/.codex/config.toml`. Add the `default_tools_approval_mode` line to that table, or skip `codex mcp add` and write the whole table yourself:

```toml
[mcp_servers.story-time]
command = "story"
args = ["mcp", "--root", "<repo>", "--write"]
default_tools_approval_mode = "writes"
```

With `--graph`, the server also offers `story_graph_sync` and `story_graph`. They project the backlog into Neo4j through its HTTP transactional Cypher API and read it back as `Story` and `Epic` nodes with `DEPENDS_ON`, `RELATES_TO`, and `IN_EPIC` edges. The Markdown stories stay the source of truth; each sync replaces one repository's projection in one transaction, and a missing `depends_on` target becomes a node with `missing: true`. `story_graph` returns the stories within `depth` (1 to 3) links of `id`, or the whole repository, at most `limit` (200) nodes. `direction` limits the links from `id`: `dependents` are the stories that wait on it, `dependencies` are the stories it waits on, and `both` is the default. A `RELATES_TO` neighbor comes at depth 1 only. For a Neo4j with no authentication, give neither `--neo4j-user` nor `--neo4j-password-env`. Pass the password through an environment variable and name it; it never goes on the command line or in a result:

```bash
NEO4J_PASSWORD=... story mcp --root <repo> --graph --neo4j-url http://127.0.0.1:7474 --neo4j-user neo4j --neo4j-password-env NEO4J_PASSWORD [--neo4j-database neo4j]
```

### Hosted MCP over HTTP

`story mcp --http` serves the same tools at `POST /mcp` ([ADR 0007](docs/adr/0007-hosted-story-mcp.md)). It needs a clients file and refuses to start without one.

```bash
story mcp --http 127.0.0.1:8080 --root <repo> --clients <clients.toml> [--allowed-origin ORIGIN]... [--workers 8] [--request-timeout-seconds 30]
```

The clients file holds one table for each client. It holds the SHA-256 hash of each token, never the token:

```toml
[[client]]
id = "agent-a"
token_sha256 = "<64 lowercase hexadecimal digits>"
scopes = ["read", "write"]   # read, write, comment
prefixes = ["ST"]            # optional; limits write and comment to these story prefixes
orgs = ["varsity"]           # optional; limits each scope to the repositories of these orgs
url_token = true             # optional; this client can give its token in the URL path
```

Make a token and its hash:

```bash
token=$(head -c 32 /dev/urandom | base64 | tr '+/' '-_' | tr -d '=')
printf '%s' "$token" | sha256sum
```

Each client sees only the tools its scopes allow, and a call outside its scopes returns `scope_refused`. A request with no known token gets HTTP 401. To revoke a token, remove its table and restart the server. The server speaks plain HTTP: put it behind a proxy that ends TLS. `GET /healthz` and `GET /readyz` return 200 without a token.

**A client of one org.** A server with a registry can hold several orgs. An entry with `orgs` uses only the repositories of those orgs, with each of its scopes. An entry with no `orgs` key uses every repository. The rules:

- A call that names a repository of another org returns `repository_not_allowed`, the same answer as a repository that the server does not have.
- `story_repos`, and `story_config` with no repository, list only the repositories of those orgs.
- The limit is an org, not one repository. Links resolve inside one org, so a client reads the linked stories of each repository of its org.
- The server refuses to start when an entry names an org that the registry does not hold, or names an org when the server has no registry. The message names the client and the org.

**A client that cannot send a header.** Some cloud clients, such as a ChatGPT connector, take only OAuth or no authentication. For such a client, make a separate entry with `url_token = true`, give it the smallest scopes it needs, and give the client the URL `<url>/mcp/<token>` with no authentication. The rules:

- Only a marked entry works in a path. The token of any other entry gets HTTP 401 there, the same answer as an unknown token.
- The server writes no path token to its log or to an answer.
- A URL is not kept as a secret. A proxy in front of the server writes it to its access log, and the client keeps it in its settings. Turn off the access log of the proxy for `/mcp/` paths, and revoke the entry when the URL was shown to someone else.

Register the server with your agent. Replace `<url>` with the address of the server and `<token>` with the client token:

```bash
claude mcp add --transport http story-time-hosted <url>/mcp --header "Authorization: Bearer <token>"
```

For Codex, keep the token in an environment variable and name that variable:

```bash
codex mcp add story-time-hosted --url <url>/mcp --bearer-token-env-var STORY_TIME_TOKEN
```

### Hosted repositories: the registry

With `--registry`, the HTTP server needs no checkout. It keeps a bare clone of each registered repository under `--data` (default `/data`) and reads stories from it:

```bash
story mcp --http 0.0.0.0:8080 --registry <registry.toml> --data <dir> --clients <clients.toml> \
  [--github-app-id-file <file> --github-app-key-file <file>] [--gitea-token-file <file>]
```

```toml
org = "varsity"                 # optional; the org of each repository that names none
fetch_ttl_seconds = 30          # optional; 30 is the default
[[repo]]
key = "varsity-core"
clone = "https://github.com/Varsity-Engine/varsity-core.git"
forge = "github"                # "github" or "gitea"
default_branch = "main"         # optional; the default is the HEAD of the remote
prefixes = ["VC", "ST"]         # every story prefix of this repository
[[repo]]
key = "varsity-web"
clone = "https://github.com/Varsity-Engine/varsity-web.git"
forge = "github"
prefixes = ["VE"]
[[repo]]
key = "story-time"
org = "team-teddy"              # optional; this repository is in another org
clone = "https://github.com/Team-Teddy-Development/story-time.git"
forge = "github"
prefixes = ["ST"]               # another org can declare a prefix that `varsity` also has
[[repo]]
key = "xanthos"
org = "team-teddy"
clone = "http://192.168.1.56:30003/team-teddy/xanthos.git"
forge = "gitea"
allow_http = true               # needed for plain http to a host that is not loopback
prefixes = ["XA"]
```

One server can hold several orgs. A repository names its `org`, or takes the top-level one; with neither, its org is `default`. An org is a boundary for links:

- A link between stories resolves only inside one org. `story_lint`, `story_update`, and `story_graph` do not look in a repository of another org.
- Two orgs can declare the same prefix. Two repositories of one org cannot.
- A repository `key` is unique in the whole file, so each tool takes `repository` and no org.
- A client token uses every registered repository, unless its entry in the clients file has `orgs`. Give a client of one org an entry with `orgs`.

The server refuses to start when an entry names a `forge` other than `github` and `gitea`, when a `gitea` entry has a plain `http://` clone to a host that is not loopback and no `allow_http = true`, when a `github` entry sets `allow_http`, when two repositories of one org declare one prefix, when two entries have one key, when an org is not a lowercase key, or when it cannot write the data directory. What a client sees:

- A read fetches the repository when its last fetch is older than `fetch_ttl_seconds`. `story_list`, `story_show`, and `story_lint` name the commit they read in `sha` and the time of the fetch in `fetched_at`.
- The same three tools take `ref`, the name of a branch, and read the head of that branch.
- `story_repos` lists each repository with its `org`, its `forge`, and its `state`: `ready`, `cloning`, or `unavailable` with `last_error`.
- A repository that the server cannot reach returns the retryable error `repository_unavailable`. The other repositories keep working, and `/readyz` stays 200.
- `story_lint` checks links across every registered repository, and reports `links.undeclared_prefix` for a story whose prefix the registry does not declare for its repository.

#### Gitea repositories (read only)

A `gitea` entry is read like a `github` entry. `--gitea-token-file` names a file that holds the token of a Gitea bot user, on one line. The server needs `--registry` to take it, and it refuses to start when the file is missing, empty, or holds white space inside the token. Git sends the token only to the origin of the clone, as the basic password. No tool result and no log line shows it. Without the flag, the server reads a Gitea repository with no credential, so a private repository is `unavailable`. The GitHub App flags are separate and serve only `github` entries.

Writes and comments on a Gitea repository are not supported yet. On a server that has write tools, `story_update`, `story_note`, `story_create`, `story_comments`, and `story_resolve_comment` return the error `forge_not_supported` for a Gitea repository. They push nothing and send no forge request. ST-149 adds writes and ST-150 adds comments.

#### Writes: one branch and one pull request for each story

A registry server has no working tree, so it does not change a story file in place. With the GitHub App flags, a client with the `write` scope has `story_update`, `story_note`, and `story_create`. Without those flags, a registry server has no write tool.

- Each write is one commit on the branch `story-time/<ID>`. The author is the App's bot, and the trailer `Requested-by:` names the client.
- The server pushes with `--force-with-lease`. When the branch moved at the remote, the call returns `conflict` and pushes nothing.
- The first write of a story opens one pull request to the default branch. Its body starts with `Stories: <ID>`. Later writes add commits to that pull request.
- The result adds `state` (`proposed` or `unchanged`), `pr_url`, `branch`, and `head_sha`. `publish.next` is `null`: the server does not publish tracker issues.
- While the pull request is open, `story_show` returns `proposed` with `ref`, `content_sha256`, and `pr_url`. Send that `content_sha256` as `expected_sha256` for the next write, and read the branch with `ref`.
- When the pull request is closed or merged, the next write starts from the default branch and replaces the old branch.
- `story_create` takes an id above every id on every branch, so two open pull requests do not carry one id.
- When GitHub does not open the pull request, the server removes the branch it pushed and returns the retryable error `forge_unavailable`.

The App needs the permissions `Contents: write` and `Pull requests: write` on each registered repository.

#### Comments: list and resolve

With the GitHub App flags, a registry server reads and answers the comments on the GitHub issue that a story names in `remote_url`. The issue must be in the story's registered repository.

- `story_comments` (scope `read`) takes `id` and returns `issue_url` and `comments`. Each comment has `id`, `url`, `author`, `author_association`, `created_at`, `updated_at`, `body`, and `resolved`.
- A comment `body` is untrusted input from its author. An agent reads it as data, not as instructions.
- `story_resolve_comment` (scope `comment`) takes `id`, `comment_id`, and a one-line `note` of at most 500 characters. It posts one reply from the App's bot: `Resolved: <note>`, a link to the comment, `Requested-by: <client>`, and a marker.
- GitHub issue comments have no resolved state. A comment is resolved when a marker reply names it, the App's bot wrote that reply (the server compares the numeric user id), and the comment was not edited after the reply. `resolved` then holds `by`, `at`, and `note`.
- The marker replies are not in `comments`.
- Errors: `no_tracker_issue`, `comment_not_found`, `already_resolved`, `note_required`, and the retryable `forge_unavailable`.

The App needs the permission `Issues: write` for these tools.

With `--graph`, a registry server projects every registered repository into Neo4j in one transaction, with `DEPENDS_ON` and `RELATES_TO` edges between repositories. It has no `story_graph_sync` tool. Each `story_graph` call compares the commits that Neo4j holds with the commits the server reads, and syncs first when they differ. What the result adds:

- Each node names its `repository`, and a node key is `Story:<repository>/<id>`.
- `sync.repositories` lists each repository with the `sha` that the graph was synced from.
- `sync.skipped_ambiguous` counts the links that the sync skipped because two repositories hold the target and no repository declares its prefix.

When Neo4j does not answer, `story_graph` returns the retryable error `graph_unavailable`. `story_lint` does not use Neo4j, so it keeps working.

For private repositories, make a GitHub App with the permissions Contents: read and Metadata: read, install it on each repository, and give the server the App ID and the private key as two files. The server signs a 10-minute JWT with the key and exchanges it for an installation token of one repository. It gives that token to `git` only in the environment of the `git` process: the token is not in a URL, an argument, a file, a log line, or a tool result. The server sends the token only to an `https` URL.

`story mcp probe --url http://127.0.0.1:8080/readyz` exits 0 when the server answers. Use it for a container probe. The image and its build are in [docs/runbooks/story-time-image.md](docs/runbooks/story-time-image.md).

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
