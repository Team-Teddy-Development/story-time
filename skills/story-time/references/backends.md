# Other backends

The CLI's filesystem adapter is the default. Teams that live in an issue
tracker keep using the skills unchanged. You act as the adapter: implement
the same five port operations with the tracker's own tools (GitHub MCP or `gh`,
Jira or Linear MCP), and keep the same fields.

| Port operation | GitHub Issues | Gitea Issues | Azure DevOps Boards | Jira / Linear |
|---|---|---|---|---|
| `list({status})` | issues with label `story`, filtered by `status:<s>` label | same as GitHub (`GET /repos/{owner}/{repo}/issues?labels=story`) | WIQL query on tag `story-time` and State | JQL / filter on status |
| `get(id)` | issue by number | issue by number | work item by id | issue by key |
| `save(story)` | create or edit issue; body = sections; fields as labels | same as GitHub; labels are sent by id | create or update work item; native fields (see below) | create or edit issue; fields as native fields |
| `remove(id)` | close as "not planned" (never delete) | close and add the `not-planned` label (never delete) | set State to Removed | move to Won't Do |
| `nextId` | tracker assigns | tracker assigns | tracker assigns | tracker assigns |

The tracker and its coordinates come from `.story-time/settings.json` (the
**story-setup** skill). For GitHub, Gitea, and ADO you don't hand-write any of this:

- `story export <id>` renders the exact card or create command.
- `story push <id...>` runs it for you through the `TrackerGateway` adapters
  (`GitHubIssuesGateway`, `GiteaIssuesGateway`, `AdoBoardsGateway`), as a saga,
  when `sync` allows.

The manual adapter steps below are for Jira and Linear.

ADO field mapping (by process template): Agile = User Story with Story
Points; Scrum = Product Backlog Item with Effort; Basic = Issue with
Effort, and acceptance criteria folded into the Description. Criteria go
in `Microsoft.VSTS.Common.AcceptanceCriteria`, bugs use
`Microsoft.VSTS.TCM.ReproSteps`, and tags are `story-time; <id>; epic:<epic>`.

Gitea (self-hosted) uses the REST API v1 directly, with no CLI. Settings:
`"gitea": { "baseUrl": "https://gitea.example.com", "owner": "acme", "repo": "shop" }`.
`baseUrl` is the web URL (http or https), not the SSH remote. The token
comes from `GITEA_TOKEN` in the environment, or else from
`story secret set gitea` (`.story-time/.gitea.env`), and goes only in the
`Authorization` header. Cards and labels are the same as GitHub
(`story export --format gitea`). Gitea needs label ids, so the gateway reads
the repository labels and creates missing ones. Gitea has no close reason:
`done` closes the issue, and `archived` / `split` close it and add the
`not-planned` label. An issue links as `<baseUrl>/<owner>/<repo>/issues/<n>`.

Field mapping for GitHub labels: `status:<s>`, `type:<t>`, `size:<n>`,
`value:<n>`, `tc:<n>`, `rr:<n>`, `epic:<id>`. Put `depends_on` in a
`Depends on: #12, #14` line at the top of the body.

To lint or rank a tracker backlog, export it into a temp backlog directory in
the file format, run `story lint` / `story rank` with `--dir <tmp>`, and
report against the tracker ids. The tracker is still the source of truth.

## Multi-write operations are sagas

A split, merge, or bulk re-prioritization takes several tracker writes, and
none of the trackers make that atomic. Run it the same way the CLI's
`split` saga does:

1. Before writing, list every step and what undoes it (create → close as not
   planned; edit → restore the previous body and labels you saved).
2. Do the steps in order, recording each one that finishes.
3. On the first failure, undo the finished steps in reverse order. If an undo
   fails, keep undoing the rest, then tell the user exactly what is left
   inconsistent.

Never leave a parent marked split without its children, or children without
the parent link.
