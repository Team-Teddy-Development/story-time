---
name: story-setup
description: One-time project setup for story-time. Detects the repo, task tracker (GitHub Issues, Gitea Issues, Azure DevOps, Jira, Linear, or plain files), tech stack, and test tooling, asks only what it cannot detect (velocity, WIP limit, process template, Definition of Done, whether to push to the tracker automatically), sets up tracker auth safely, and writes .story-time/settings.json, which every other story-time skill reads. Use when the user says "set up story time", "configure story-time", starts using story-time in a new repo, or when `story config` shows no config file.
---

# Story Setup

`story` means the CLI from the **story-time** skill. Setup writes
`.story-time/settings.json` at the project root. Commit it, so the whole team and
every agent use the same settings.

## Gitea setup branch

Read `.story-time/settings.json` directly before running `story config`.
If `tracker` is `gitea`, or the selected tracker is Gitea, follow
[Gitea Story Time](../../docs/gitea-story-time.md) instead of steps 1 through 6.
That reference owns the settings schema, Rust binary installation, authentication,
explicit-path dry run, push, and failure recovery. For a copied skill
installation, locate the Story Time source checkout to read that reference.
The installer copies skills only. From the checkout, install the binary with
`cargo install --offline --path gitea-gateway`.

Write `tracker: "gitea"`, `sync`, and `gitea: {baseUrl, owner, repo}` directly
as JSON. For example:

```json
{"tracker":"gitea","sync":"ask","gitea":{"baseUrl":"https://gitea.example.com","owner":"acme","repo":"shop"}}
```

Preserve unrelated settings. Use only `GITEA_TOKEN` in the environment.
Run `gitea-gateway story-push --root <project> --dry-run backlog/ST-001.md`.
For an authorized real push, remove `--dry-run`. Repeat pushes use the saved
issue number. Terminal unlinked stories are skipped; linked terminal stories
are closed. Preserve `remote` and `remote_url` when editing stories.
Complete setup when the Rust dry run parses and renders one actual story
without credentials or file changes. Report that this checks local setup,
not authentication or live server compatibility. An authorized push verifies
the issue and local `remote` / `remote_url` links.

Upstream also provides Node Gitea setup and push commands. They are separate
from this Rust path. The following legacy setup steps remain outside the
Gitea Rust conversion scope.

## 1. Check for an existing config

```bash
story config --json
```

If the first line says a config file was found, show the user the current
values and ask what to change. Otherwise, start from scratch.

## 2. Detect before asking

Find out as much as you can without asking. Note where each answer came
from:

| What | How to detect |
|---|---|
| Tracker and repo | `git remote get-url origin`. `github.com/<owner>/<repo>` means GitHub. A self-hosted host with an SSH port such as `ssh://git@<host>:<port>/<owner>/<repo>.git`, or a `/api/v1/version` endpoint on the web host, means Gitea (the web URL, not the SSH remote, is `gitea.baseUrl`). `dev.azure.com/<org>/<project>/_git/...` or `<org>.visualstudio.com` means Azure DevOps |
| Tracker tools on hand | `gh --version`, `az boards -h`, `tea --version`, and any GitHub, Azure DevOps, Jira, or Linear MCP tools available to you |
| Existing backlog | A `backlog/` folder of `ST-*.md` files, or another folder of story files |
| Stack | `package.json` (and its `dependencies`), `tsconfig.json`, `*.csproj` / `*.sln`, `pyproject.toml` / `requirements.txt`, `go.mod`, `Cargo.toml`, `pom.xml` / `build.gradle` |
| Test framework and command | `package.json` `scripts.test` and devDependencies (vitest, jest, mocha, playwright); `dotnet test` with xUnit/NUnit/MSTest; pytest; `go test`; `cargo test` |
| Definition of Done hints | `CONTRIBUTING.md`, the PR template, `CLAUDE.md` / `AGENTS.md` |

A git remote tells you where the code lives, not necessarily where the
work items live. Teams often keep code on GitHub and items in Azure DevOps
or Jira. Always confirm the tracker.

## 3. Ask what's left

Ask in one round, with your detected answer as the default for each
question so the user can just confirm. In Claude Code, use
`AskUserQuestion`; elsewhere, use a short numbered list.

1. **Tracker.** Where do work items live? `files` (markdown in the repo),
   `github`, `gitea`, `ado`, `jira`, or `linear`.
2. **Tracker coordinates.**
   - GitHub: `owner/repo` for issues, if different from the code repo.
   - Gitea: the web base URL (`https://gitea.example.com`, http allowed for
     a LAN host), the owner (user or organization), and the repo.
   - ADO: organization URL, project, process template (`agile` = User
     Story + Story Points, `scrum` = Product Backlog Item + Effort, `basic`
     = Issue + Effort), plus an optional area path and iteration path.
3. **Id prefix**, e.g. `ST`, or the team's key like `PAY`.
4. **Team.** Sprint length in working days, velocity in points per sprint
   (skip if unknown, since `backlog-groom` asks later), and WIP limit per person.
5. **Definition of Done / Ready.** Any team rules beyond the defaults in
   the **story-accept** and **story-ready** skills, one line each.
6. **Sync** (GitHub, Gitea, or ADO only). Should story-time write to the tracker?
   - `off` (the default): never. Skills hand back copyable cards, and
     `story push` refuses to run. Pick this when the project must not
     change the tracker on its own.
   - `ask`: after each create, edit, or split, ask before pushing.
   - `auto`: push right after each change.

   For Jira, Linear, or files, sync is always `off`.
7. **Decisions: bring your own TypeSafe key?** (optional) story-time's
   judgement calls (need vs. solution, real benefit, testable criteria,
   type, size, split pattern) are code-driven by default: deterministic
   heuristics, nothing leaves the machine. With a TypeSafe API key they are
   Jev-based, with the code heuristics as the fallback.
   - Say exactly what is sent, and only to `https://api.typesafe.ai`: each
     judged story's title, persona, statement, and acceptance criteria. Never
     the changelog, notes, or files. A story containing an email address, a
     phone number, or a token-shaped string is never sent; code decides it.
   - Ask whether the backlog may go to that vendor. If not, or there's no
     key, skip this: everything still works, code-driven.
   - To pin the model for reproducible decisions, set `"jev": { "model": "jev-1.13.0" }`.
     The key itself never goes in settings.

Don't ask about anything you detected with certainty unless the user
wants to change it.

## 4. Write

Write the answers to a temp file, then let the CLI validate and save them.
Nothing is written if any value is invalid:

```json
{
  "tracker": "ado",
  "sync": "ask",
  "prefix": "PAY",
  "backlogDir": "backlog",
  "ado": {
    "organization": "https://dev.azure.com/acme",
    "project": "Shop",
    "process": "agile",
    "areaPath": "Shop\\Payments"
  },
  "team": { "sprintDays": 10, "velocity": 24, "wipLimit": 1 },
  "project": { "stack": ["csharp", "aspnetcore", "react"], "testFramework": "xunit", "testCommand": "dotnet test" },
  "definitionOfDone": ["Release note added to CHANGELOG.md"],
  "definitionOfReady": ["UX sign-off linked for UI stories"]
}
```

For GitHub, use this tracker configuration:

```json
{
  "tracker": "github",
  "sync": "off",
  "github": { "repo": "acme/shop" }
}
```

Set `github.repo` to the complete `owner/repo` coordinate. Separate `owner`
and bare `repo` fields apply to Gitea, not GitHub.
Use `story init --help` for the installed command's setup options.

For Gitea the tracker part is:

```json
{
  "tracker": "gitea",
  "sync": "ask",
  "gitea": { "baseUrl": "https://gitea.example.com", "owner": "acme", "repo": "shop" }
}
```

```bash
story init --from /tmp/story-time-answers.json        # add --force to replace an existing config
story config
```

If `init` reports an error, fix that value with the user and run it again.
`init` writes `.story-time/settings.json` and `.story-time/.gitignore`
(which ignores `*.env`).

## 5. Authentication (only when sync is `ask` or `auto`)

Check what already works, in this order, and stop at the first that does:

1. **An existing CLI login.** `gh auth status` for GitHub, or
   `az account show` plus `az devops configure --list` for ADO. This is the
   best option: gh keeps its token in the OS keychain. Gitea has no CLI
   login that story-time uses; skip to the next step.
2. **An environment variable** the user already exports (CI, a shell
   profile): `GH_TOKEN`, `GITEA_TOKEN`, or `AZURE_DEVOPS_EXT_PAT`.
3. **A project token file.** Ask the user to run this themselves, in
   their own terminal:

   ```bash
   story secret set github     # or: story secret set gitea / story secret set ado
   ```

   It prompts with hidden input and saves `.story-time/.github.env`,
   `.story-time/.gitea.env`, or `.story-time/.ado.env` with mode 600,
   git-ignored. Suggest a
   fine-grained PAT: for GitHub, Issues read/write on this repo only. For
   Gitea, an access token with the `write:issue` scope (labels are part of
   it). For ADO, the Work Items (Read & write) scope only, with an expiry date.

Never ask for the token, accept it if it is pasted into the chat, or pass
it to any command yourself. If the user pastes one anyway, tell them to
revoke it and create a new one, because the chat transcript now contains it.
Confirm with `story secret status`, which shows only the last four
characters.

Then prove it works without changing anything: `story push --all --dry-run`.

**Jev key (only if the user chose bring-your-own-key).** The same rules
apply. If `TYPESAFE_API_KEY` is already exported, nothing more is needed.
Otherwise the user runs this themselves:

```bash
story secret set jev
```

That stores `.story-time/.jev.env` (mode 600, git-ignored). Never take the
key in chat. Prove it with `story judge <any id>`: the first line must read
`engine: jev (...)`. If it reads `engine: code (...)`, it says why. A
rejected key falls back to code for the run and warns once, without the key.
`STORY_TIME_JEV=off` turns Jev off without deleting the key.

## 6. Confirm and hand off

Show the effective config from `story config` and say how it will be used:

- `story judge <id>` makes the judgement calls: Jev-based with a key,
  code-driven without. Each decision says its source (`jev`, `code`, or
  `fallback` with the reason) and a confidence.
- `story export <id>` now prints cards in the tracker's format (`github`,
  `gitea`, `ado`, or neutral `markdown` for Jira, Linear, and files). `--command`
  prints the `gh` or `az boards` command that creates the item.
- With sync `ask` or `auto`, `story push <id>` creates or updates the
  tracker item and records `remote` / `remote_url` in the story.
- **story-pick** uses `team.wipLimit` and `team.sprintDays`.
  **backlog-groom** uses `team.velocity` for the ready buffer.
- **story-ready** and **story-accept** add `definitionOfReady` and
  `definitionOfDone` to their checklists.
- **story-kickoff** writes test names for `project.testFramework` and runs
  `project.testCommand`.

Suggest committing `.story-time/settings.json` and `.story-time/.gitignore` (never the `.env` files). Then offer the next step: the
**story-create** skill for a new backlog, or the **backlog-groom** skill
for an existing one.
