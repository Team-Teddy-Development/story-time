---
name: agile-metrics
description: Report delivery metrics (throughput, WIP, cycle time, velocity) for a date window from the Rust `story metrics` command. Every figure names its source, window, and units, and missing history stays `unknown`. Use when the user says "metrics", "throughput", "cycle time", "how much did we finish", "what is our velocity", or asks for delivery figures for a period.
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
- `velocity`: always `unknown` today. Units: points per sprint.

Never compute a figure yourself. Do not count stories from `story list`,
from prose, or from memory. Do not convert points to hours or days. If the
command fails, report the error and stop; do not estimate in its place.

## 3. Present the figures as given

Every figure names its source, window, and units. Copy them from the
command output into this table, one row per figure:

| Metric | Value | Units | Window | Source |
|---|---|---|---|---|
| throughput | 3 | stories | 2026-11-01..2026-11-14 | changelog `set status=` and `split into` lines |
| cycle_time_median | `unknown` | days | 2026-11-01..2026-11-14 | changelog `set status=` and `split into` lines |
| velocity | `unknown` | points per sprint | 2026-11-01..2026-11-14 | no sprint record is stored |

Then list each entry of every figure's `unknown` array with its story and
reason. Add the definition the command gives when the lead asks how a
figure was counted.

## 4. Keep gaps unknown

Report a gap as `unknown`, with the reason the command gave.
Never fill a gap. Do not infer a missing `in-progress` date from `created`, `updated`, a
pull request, or a commit. Do not drop a story with a gap to make a figure
look complete.

These facts are not stored. Do not invent them:

- No sprint record is stored, so velocity is `unknown`. Name the question
  that would fill it: which sprints, with which dates and members.
- No blocked or scope event history is stored. Lead time, blocked time, and
  scope change are `unknown`.
- A story whose status has no matching changelog event has a gap in its
  history. The command lists it as `unknown` for every figure.

## 5. Read the figures honestly

Figures describe delivery for this team and window only. Do not compare
people or teams with them, and do not treat them as a productivity score or
a value measure. Track user outcomes separately.

## Hand off

- To act on stuck or over-WIP work, use the **flow-review** skill.
- To close a sprint with its goal outcome, use the **sprint-review** skill.
- To plan the next sprint with these figures, use the **sprint-planning**
  skill, and name the window each figure came from.
