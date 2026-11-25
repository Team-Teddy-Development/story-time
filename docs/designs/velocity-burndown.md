# Velocity, burndown and forecast

2026-11-25. Asked by Eric: build velocity and burndown into story-time as a Rust command, a skill and an MCP tool, so completion dates come closer to when work ships (ST-193).

## Today

- `story metrics` (`native/story-push/src/metrics.rs`) reports throughput, WIP and median cycle time from each story's `## Changelog` status events (`- DATE set status=S`).
- Velocity is always `unknown` with "no sprint record is stored": story-time has no record of what a sprint committed to.
- There is no burndown and no date forecast.

## Data shape

A sprint record is the one new artifact. One JSON file per sprint, committed with the backlog:

```
.story-time/sprints/<sprint-id>.json
{
  "id": "2026-S23",
  "start": "2026-11-24", "end": "2026-12-05", "tz": "America/Chicago",
  "goal": "...",
  "committed": [{"id": "ESS-269", "points": 2}, ...],   // snapshot at start
  "changes": [{"date": "2026-11-26", "op": "add" | "drop", "id": "ESS-410", "points": 5, "why": "..."}],
  "closed": null | {"date": "2026-12-05", "done": ["ESS-269", ...], "carried": ["ESS-410"]}
}
```

- The commitment is a snapshot taken at start, so a later re-estimate does not rewrite history. Scope changes are logged, not edited in.
- Done dates come from the story changelog, which is the existing single source; the sprint file never copies them.
- Points are the story `size` at the moment the story enters the sprint.

## Figures

- **Velocity.** Completed points for each closed sprint (stories done by `end` from `committed` plus `add` changes), then the mean, minimum and maximum over the last 3 and last 6 closed sprints. Below 3 closed sprints it is `unknown`, with the count.
- **Burndown.** For each calendar day of the sprint: remaining points (scope minus points done on or before that day), scope (so a scope change shows as a step), the ideal line, and today. Weekends are kept, with a flag, and are never silently dropped.
- **Burnup** for an epic or a filtered set when there is no sprint: cumulative points done against total scope over a date window.
- **Forecast.** A Monte Carlo (10,000 runs, seeded so results are reproducible) that samples historical sprint velocity, or daily throughput when there are fewer than 3 sprints. It reports the 50% and 85% completion dates for the sprint remainder, an epic, or any set of story ids. ST-193 later replaces the raw samples with calibrated ones.

Every figure keeps the `metrics.rs` contract: it names its source, window and units, and missing history is `unknown` with a reason, never zero.

## Surfaces

| Surface | What |
|---|---|
| Rust CLI | `story sprint start/add/drop/close/show`, `story velocity`, `story burndown [--sprint ID] [--json/--text/--svg]`, `story forecast (--sprint ID / --epic ID / --ids ...)` |
| MCP (local `story mcp` and hosted) | read-only `story_velocity`, `story_burndown`, `story_forecast`; `story_sprint` for start/add/drop/close behind the same write rules as `story_update` |
| Skills | `agile-metrics` reports all three; `sprint-planning` sizes the commitment from the velocity range, not the mean; `sprint-review` shows the burndown and the committed-against-done delta; `story-time` routes "burndown", "velocity", "when will it be done" |

The SVG burndown is a small static chart drawn by hand in Rust (no charting dependency), so it can go in a PR body, an issue or a report.

## Stories, in order

1. Sprint records and `story sprint`. Velocity in `story metrics` stops being `unknown`.
2. `story velocity` and `story burndown` (JSON and text), plus burnup.
3. `story forecast`, the Monte Carlo.
4. MCP tools, local and hosted.
5. Skills: agile-metrics, sprint-planning and sprint-review.
6. `--svg` burndown.

ST-193 (estimates learn from shipped time) builds on 1 to 3: it adds the estimate-and-outcome ledger and calibrates the forecast samples.

## Not in scope

- Tracker sprint fields (GitHub Projects iterations, Jira sprints). `planning.rs` already treats those as an adapter contract; a later story can publish to them.
- Hours or time tracking. The units are points and calendar days.
