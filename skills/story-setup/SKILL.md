---
name: story-setup
description: One-time project setup for story-time. Detects the repo, task tracker (GitHub Issues, Azure DevOps, Jira, Linear, or plain files), tech stack, and test tooling, asks only what it cannot detect (team velocity, WIP limit, process template, Definition of Done), and writes a validated .story-time.json that every other story-time skill reads. Use when the user says "set up story time", "configure story-time", starts using story-time in a new repo, or when `story config` shows no config file.
---

# Story Setup

`story` means the CLI from the **story-time** skill. Setup writes one file,
`.story-time.json`, at the project root. Commit it, so the whole team and
every agent use the same settings.

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
| Tracker and repo | `git remote get-url origin`. `github.com/<owner>/<repo>` means GitHub. `dev.azure.com/<org>/<project>/_git/...` or `<org>.visualstudio.com` means Azure DevOps |
| Tracker tools on hand | `gh --version`, `az boards -h`, and any GitHub, Azure DevOps, Jira, or Linear MCP tools available to you |
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
   `github`, `ado`, `jira`, or `linear`.
2. **Tracker coordinates.**
   - GitHub: `owner/repo` for issues, if different from the code repo.
   - ADO: organization URL, project, process template (`agile` = User
     Story + Story Points, `scrum` = Product Backlog Item + Effort, `basic`
     = Issue + Effort), plus an optional area path and iteration path.
3. **Id prefix**, e.g. `ST`, or the team's key like `PAY`.
4. **Team.** Sprint length in working days, velocity in points per sprint
   (skip if unknown, since `backlog-groom` asks later), and WIP limit per person.
5. **Definition of Done / Ready.** Any team rules beyond the defaults in
   the **story-accept** and **story-ready** skills, one line each.

Don't ask about anything you detected with certainty unless the user
wants to change it.

## 4. Write

Write the answers to a temp file, then let the CLI validate and save them.
Nothing is written if any value is invalid:

```json
{
  "tracker": "ado",
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

```bash
story init --from /tmp/story-time-answers.json        # add --force to replace an existing config
story config
```

If `init` reports an error, fix that value with the user and run it again.

## 5. Confirm and hand off

Show the effective config from `story config` and say how it will be used:

- `story export <id>` now prints cards in the tracker's format (`github`,
  `ado`, or neutral `markdown` for Jira, Linear, and files). `--command`
  prints the `gh` or `az boards` command that creates the item.
- **story-pick** uses `team.wipLimit` and `team.sprintDays`.
  **backlog-groom** uses `team.velocity` for the ready buffer.
- **story-ready** and **story-accept** add `definitionOfReady` and
  `definitionOfDone` to their checklists.
- **story-kickoff** writes test names for `project.testFramework` and runs
  `project.testCommand`.

Suggest committing `.story-time.json`. Then offer the next step: the
**story-create** skill for a new backlog, or the **backlog-groom** skill
for an existing one.
