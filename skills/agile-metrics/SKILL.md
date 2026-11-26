---
name: agile-metrics
description: Report delivery metrics (throughput, WIP, cycle time, velocity), the sprint burndown, and the completion forecast from the Rust `story` commands or the `story_velocity`, `story_burndown` and `story_forecast` MCP tools. Every figure names its source, window, and units, and missing history stays `unknown`. Use when the user says "metrics", "throughput", "cycle time", "how much did we finish", "what is our velocity", "burndown", "forecast", "when will it be done", or asks for delivery figures for a period.
---

# Agile metrics

`story` means the CLI from the **story-time** skill. This skill only reads.
It changes no story, estimate, or tracker item.

## 1. Agree the window

Ask the lead for the window as two dates, `YYYY-MM-DD`, start and end
inclusive. With no answer, use the last 14 days and say so in the report.

## 2. Run the Rust command

```bash
story metrics --from 2026-11-01 --to 2026-11-14 --json
```

`story metrics` is native Rust and read-only. It reads the `set status=`
and `split into` lines in each story's Changelog, at day resolution, and
returns:

- `throughput`: stories with a `done` event in the window. Units: stories.
- `wip`: stories whose last status event at the window end is
  `in-progress`. Units: stories.
- `cycle_time_median`: median days from the first `in-progress` event to the
  `done` event, with a per-story list. Units: days.
- `velocity`: mean completed points per closed sprint over the last 3 closed
  sprints, with `min` and `max`, and the same over the last 6. It is `unknown`
  until 3 sprints have closed with known points. Units: points per sprint.

Never compute a figure yourself. Do not count stories from `story list`,
from prose, or from memory. Do not convert points to hours or days. If the
command fails, report the error and stop; do not estimate in its place.

## 3. Run the burndown and the forecast

When the lead asks where a sprint stands or when work will finish, run the two
read-only commands after the metrics. Take today's date from the lead or the
system, and pass it with `--today` so the report names it.

```bash
story burndown --sprint 2026-S05 --today 2026-11-06 --json
story forecast --ids ST-201,ST-202 --today 2026-11-06 --json
```

- `story burndown` gives `rows` with the `scope`, `remaining` and `ideal`
  points of each day of the sprint. `remaining` is null after today. A story
  with no size is listed under `unknown` and counts as nothing.
- `story forecast` takes exactly one of `--sprint`, `--epic` or `--ids`. Its
  `rows` hold the 50th and 85th percentile completion dates. Quote both and say
  the 85th is the safer date. With fewer than 3 closed sprints the dates are
  `unknown` with the reason. Never turn them into a promise.

When the `story-time` MCP server is connected and lists the repository, call
`story_velocity` (`from`, `to`), `story_burndown` (`sprint`, `today`) and
`story_forecast` (one of `sprint`, `epic`, `ids`, plus `today` and `seed`)
instead, naming the `repository`. Each returns the JSON of its command on that
repository's default branch. `ref` reads a proposal branch instead and the
result names its `sha`. These tools only read.

## 4. Present the figures as given

Every figure names its source, window, and units. Copy them from the
command output into this table, one row per figure:

| Metric | Value | Units | Window | Source |
|---|---|---|---|---|
| throughput | 3 | stories | 2026-11-01..2026-11-14 | changelog `set status=` and `split into` lines |
| cycle_time_median | `unknown` | days | 2026-11-01..2026-11-14 | changelog `set status=` and `split into` lines |
| velocity | 10 | points per sprint | 2026-09-07..2026-11-14 | changelog `set sprint=`, `set size=` and `set status=` lines, with sprint dates from settings |
| burndown remaining | 7 | points | 2026-11-02..2026-11-15 | changelog `set sprint=`, `set size=` and `set status=` lines |
| forecast 85th percentile | 2026-12-13 | date | 2026-09-07..2026-11-15 | closed-sprint velocity, 10000 seeded runs |

For velocity also give the `last_3` minimum and maximum, since planning uses the
range and not the mean. Then list each entry of every figure's `unknown` array with its story and
reason. Add the definition the command gives when the lead asks how a
figure was counted.

## 5. Keep gaps unknown

Report a gap as `unknown`, with the reason the command gave.
Never fill a gap. Do not infer a missing `in-progress` date from `created`, `updated`, a
pull request, or a commit. Do not drop a story with a gap to make a figure
look complete.

These facts are not stored. Do not invent them:

- Velocity needs at least 3 closed sprints with known points, from the
  `sprint` attribute and the sprint calendar in settings. Below that it is
  `unknown`. Name the question that would fill it: which sprints, with which
  dates and members.
- No blocked or scope event history is stored. Lead time, blocked time, and
  scope change are `unknown`.
- A story whose status has no matching changelog event has a gap in its
  history. The command lists it as `unknown` for every figure.

## 6. Read the figures honestly

Figures describe delivery for this team and window only. Do not compare
people or teams with them, and do not treat them as a productivity score or
a value measure. Track user outcomes separately.

## Hand off

- To act on stuck or over-WIP work, use the **flow-review** skill.
- To close a sprint with its goal outcome, use the **sprint-review** skill.
- To plan the next sprint with these figures, use the **sprint-planning**
  skill, and name the window each figure came from.
- To show the burndown at the end of a sprint, use the **sprint-review** skill.
