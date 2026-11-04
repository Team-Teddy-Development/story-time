# Other backends

The CLI's filesystem adapter is the default. Teams that live in an issue
tracker keep using the skills unchanged. You act as the adapter: implement
the same five port operations with the tracker's own tools (GitHub MCP or `gh`,
Jira or Linear MCP), and keep the same fields.

| Port operation | GitHub Issues | Azure DevOps Boards | Jira / Linear |
|---|---|---|---|
| `list({status})` | issues with label `story`, filtered by `status:<s>` label | WIQL query on tag `story-time` and State | JQL / filter on status |
| `get(id)` | issue by number | work item by id | issue by key |
| `save(story)` | create or edit issue; body = sections; fields as labels | create or update work item; native fields (see below) | create or edit issue; fields as native fields |
| `remove(id)` | close as "not planned" (never delete) | set State to Removed | move to Won't Do |
| `nextId` | tracker assigns | tracker assigns | tracker assigns |

The tracker and its coordinates come from `.story-time.json` (the
**story-setup** skill). `story export <id>` renders the exact card or
create command for GitHub and ADO, so use it rather than formatting items by hand.

ADO field mapping (by process template): Agile = User Story with Story
Points; Scrum = Product Backlog Item with Effort; Basic = Issue with
Effort, and acceptance criteria folded into the Description. Criteria go
in `Microsoft.VSTS.Common.AcceptanceCriteria`, bugs use
`Microsoft.VSTS.TCM.ReproSteps`, and tags are `story-time; <id>; epic:<epic>`.

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
