# Velocity, burndown and forecast

2026-11-25. Asked by Eric: build velocity and burndown into story-time as a Rust command, a skill and an MCP tool, so completion dates come closer to when work ships (ST-193).

## Today

- `story metrics` (`native/story-push/src/metrics.rs`) reports throughput, WIP and median cycle time from each story's `## Changelog` status events (`- DATE set status=S`).
- Velocity is always `unknown` with "no sprint record is stored": story-time has no record of what a sprint committed to.
- There is no burndown and no date forecast.

## Data shape

Sprint membership is an attribute on the story, so every surface that reads a story (CLI, MCP, hosted server, tracker sync) sees it with no new artifact. Decided by Eric, 2026-11-25.

```
---
id: ESS-269
status: in-progress
size: 2
sprint: 2026-S23        # new, optional
---
```

- **Sprint calendar.** It comes from settings, not files: `team.sprintDays` (it exists) plus a new `team.sprintAnchor` (the start date of sprint 1, and the time zone). A sprint id `YYYY-Snn` maps to fixed start and end dates. An optional `team.sprints` list overrides the dates and adds a goal for a single sprint (holidays, a short sprint).
- **Committed set.** This is the stories whose changelog has `set sprint=<id>` on or before the sprint's start date. The changelog is the history, so nothing is snapshotted.
- **Scope changes.** A `set sprint=<id>` dated after the start is an add. A later `set sprint=<other>` or `set sprint=` is a drop. These are the same changelog lines `story set` and `story_update` already write.
- **Points at commitment.** This is the last `set size=` event on or before the date the story entered the sprint, so a later re-estimate does not rewrite the burndown.
- **Done dates.** These come from the existing `set status=done` changelog events.
- **Carry-over.** When a sprint closes, an unfinished story gets `set sprint=<next>`. That is a drop from the old sprint and an add to the new one, and both are visible.

MCP: `story_show`, `story_list` and `story_update` read and set `sprint` like any other field, `story_list` gains a `sprint` filter, and a sprint is "started" by setting the attribute on its stories. This needs no `story sprint` file commands. `story sprint show <id>` remains as a read-only view of the calendar dates, the committed set, and the changes.

## Figures

- **Velocity.** Completed points for each closed sprint (stories done by `end` from `committed` plus `add` changes), then the mean, minimum and maximum over the last 3 and last 6 closed sprints. Below 3 closed sprints it is `unknown`, with the count.
- **Burndown.** For each calendar day of the sprint: remaining points (scope minus points done on or before that day), scope (so a scope change shows as a step), the ideal line, and today. Weekends are kept, with a flag, and are never silently dropped.
- **Burnup** for an epic or a filtered set when there is no sprint: cumulative points done against total scope over a date window.
- **Forecast.** A Monte Carlo (10,000 runs, seeded so results are reproducible) that samples historical sprint velocity, or daily throughput when there are fewer than 3 sprints. It reports the 50% and 85% completion dates for the sprint remainder, an epic, or any set of story ids. ST-193 later replaces the raw samples with calibrated ones.

Every figure keeps the `metrics.rs` contract: it names its source, window and units, and missing history is `unknown` with a reason, never zero.

## Surfaces

| Surface | What |
|---|---|
| Rust CLI | `sprint` attribute via `story set`, `story sprint show`, `story velocity`, `story burndown [--sprint ID] [--json/--text/--svg]`, `story forecast (--sprint ID / --epic ID / --ids ...)` |
| MCP (local `story mcp` and hosted) | read-only `story_velocity`, `story_burndown`, `story_forecast`; `sprint` is set through `story_update`, and `story_list` filters by it |
| Skills | `agile-metrics` reports all three; `sprint-planning` sizes the commitment from the velocity range, not the mean; `sprint-review` shows the burndown and the committed-against-done delta; `story-time` routes "burndown", "velocity", "when will it be done" |

The SVG burndown is a small static chart drawn by hand in Rust (no charting dependency), so it can go in a PR body, an issue or a report.

## Stories, in order

1. The `sprint` story attribute, the sprint calendar in settings, and `story sprint show`. Velocity in `story metrics` stops being `unknown`.
2. `story velocity` and `story burndown` (JSON and text), plus burnup.
3. `story forecast`, the Monte Carlo.
4. MCP tools, local and hosted.
5. Skills: agile-metrics, sprint-planning and sprint-review.
6. `--svg` burndown.

ST-193 (estimates learn from shipped time) builds on 1 to 3: it adds the estimate-and-outcome ledger and calibrates the forecast samples.

## Not in scope

- Tracker sprint fields (GitHub Projects iterations, Jira sprints). `planning.rs` already treats those as an adapter contract; a later story can publish to them.
- Hours or time tracking. The units are points and calendar days.
