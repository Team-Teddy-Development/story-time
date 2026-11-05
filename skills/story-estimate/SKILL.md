---
name: story-estimate
description: Size stories with relative story points through a simulated planning-poker round of independent estimators anchored on reference stories, and score WSJF value inputs (business value, time criticality, risk reduction) so stories can be ranked. Use when the user says "estimate", "size", "point", "planning poker", "score value", or when lint flags size.estimated or value.scored.
---

# Story Estimate

`story` means the CLI from the **story-time** skill. Use the modified
Fibonacci scale `1 2 3 5 8 13 20`, relative within this backlog.

Estimate the smallest credible implementation of the written scope. Missing
detail lowers confidence; it does not add imagined work. Do not size every
possible interpretation, contingency, or future hardening task.

## Part A: size (effort, complexity, uncertainty)

### 1. Find anchors

Run `story list --status done --json`. Pick up to three finished stories the
team sized 1–2, 3, and 5. Prefer anchors with similar work, boundaries, and
test obligations. A diff's line count is context, not an estimate. If the
backlog has no useful anchors, ask for one story everyone agrees is a 3.

Before poker, check that the story identifies the outcome and its boundary.
If the title could reasonably mean different domain objects, or the criteria
do not identify what behavior proves completion, report `unestimated` and
list the specific missing product decisions. Route to **acceptance-criteria**
or **story-edit**. Use a **spike** only for a technical unknown that can be
answered by investigation or experiment.

### 2. Get a first read

```bash
story judge <id> --json
```

`decisions.size` is a suggested size with a confidence. Jev-based with a
key, it reads the statement and criteria. Code-driven, it only counts the
criteria (confidence 0.3). Treat it as one more voice in the round, never as
the answer. It cannot expand the written scope. A size of 13 or more, or a
`splitPattern`, is a signal to inspect the scope before poker, not an automatic
split. Split only when the clearly defined work is genuinely large.

### 3. Play planning poker

For each story, spawn three estimators in parallel, one message, three
subagent calls (Claude Code: `Agent`, `subagent_type: general-purpose`;
Codex: `spawn_agent`).
Give each the story, anchors, and relevant codebase context. Tell every
estimator to use only written scope and the simplest implementation consistent
with the repository. Give each a different lens:

| Estimator | Lens |
|---|---|
| Builder | Required code changes and migrations; exclude speculative redesigns |
| Tester | Tests needed to prove the written criteria; exclude unrelated hardening |
| Operator | Operational work explicitly required to ship safely in this repository |

Each returns `{ size, confidence: low|med|high, drivers: [..], risks: [..] }`
and must not see the others' answers. If your harness has no subagents, write
each estimate yourself, one after another, before reading the last.

Risks explain confidence. Do not convert every risk into points. Existing or
historical implementations may calibrate the estimate, but separate bundled
work, refactors, generated files, and adjacent stories before comparing.

### 4. Converge

- Use the median vote when all three estimates are within one scale step.
  Examples: `3/3/5 -> 3`, `3/5/5 -> 5`, `5/5/5 -> 5`.
- With a spread of two or more steps, compare assumptions. Remove work not
  required by the story, then re-estimate once. Do not average disagreement.
- If disagreement comes from a missing product decision, return `unestimated`
  and request that decision instead of inflating the estimate.
- Recommend **story-split** only when a clearly defined scope still earns 13
  or more. Recommend **spike** only when a technical unknown prevents sizing.
- If every estimate has low confidence, do not record points.

### 5. Record

```bash
story set ST-012 size=3 --note "poker 3/3/5; anchors ST-003(2) ST-007(5); driver: token expiry edge cases"
```

Record only when the scope is clear enough and the converged confidence is at
least medium. Put the drivers, material risks, poker votes, and anchors under
`## Notes`.

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
