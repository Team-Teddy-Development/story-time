---
name: story-estimate
description: Size stories with relative story points through a simulated planning-poker round of independent estimators anchored on reference stories, and score WSJF value inputs (business value, time criticality, risk reduction) so stories can be ranked. Use when the user says "estimate", "size", "point", "planning poker", "score value", or when lint flags size.estimated or value.scored.
---

# Story Estimate

`story` means the CLI from the **story-time** skill. The scale for every
number is `1 2 3 5 8 13 20`, relative within this backlog.

## Part A: size (effort, complexity, uncertainty)

### 1. Find anchors

Run `story list --status done --json`. Pick up to three finished stories
the team sized 1–2, 3, and 5 as reference points. If the backlog has none,
ask the user for one story everyone agrees is a "3" and anchor on it.

### 2. Get a first read

```bash
story judge <id> --json
```

`decisions.size` is a suggested size with a confidence. Jev-based with a
key, it reads the statement and criteria. Code-driven, it only counts the
criteria (confidence 0.3). Treat it as one more voice in the round. Never
treat it as the answer. A size of 13 or more, or a `splitPattern`, means
the story should go to the **story-split** skill before poker.

### 3. Play planning poker

For each story, spawn three estimators in parallel, one message, three
subagent calls (Claude Code: `Agent`, `subagent_type: general-purpose`;
Codex: `spawn_agent`).
Give each the story file, the anchors, and the codebase if there is one.
Give each a different lens:

| Estimator | Lens |
|---|---|
| Builder | Code to write and change, unfamiliar areas, migrations |
| Tester | Criteria count, edge cases, test setup, data fixtures |
| Operator | Deployment, flags, observability, rollout and rollback |

Each returns `{ size, confidence: low|med|high, drivers: [..], risks: [..] }`
and must not see the others' answers. If your harness has no subagents, write
each estimate yourself, one after another, before reading the last.

### 4. Converge

- All three within one step of each other: take the higher of the two
  closest values.
- Spread of two or more steps: the high and low estimators each give their
  strongest driver. Usually one of them has found missing scope or a missing
  criterion. Fix the story (the **acceptance-criteria** skill), then
  re-estimate once.
- Still split, or any estimate of 13 or more: don't average. Recommend the
  **story-split** skill. If it's low confidence everywhere, recommend the
  **spike** skill.

### 5. Record

```bash
story set ST-012 size=3 --note "poker 3/3/5; anchors ST-003(2) ST-007(5); driver: token expiry edge cases"
```

Put the drivers and risks under `## Notes`.

## Part B: value (WSJF cost-of-delay inputs)

These are business judgments. **Ask the product owner or user.** Score a
batch together, because the numbers are only meaningful relative to each
other: give the lowest story in the batch a 1 on each dimension and score
the others against it.

| Field | Question |
|---|---|
| `value` | How much do users or the business gain once this ships? |
| `time_criticality` | How fast does that value decay if we wait? Deadlines, seasonality, competitors |
| `risk_reduction` | Does it retire a risk or unlock other work? |

If nobody can answer, propose scores with a one-line rationale each, write
them with `--note "PROPOSED by agent: <rationale>"`, and say so in your report.

```bash
story set ST-012 value=8 time_criticality=5 risk_reduction=3 --note "PO: support ticket volume; Q4 SSO deadline"
```

## Report

A table: id, size, value, time criticality, risk reduction, the resulting
WSJF (`story rank` shows it), and which numbers are proposed and which are
confirmed.
