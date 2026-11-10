---
name: decision-table
description: Write a business rule as a checked decision table (inputs with finite values, condition rows with "-" for any value, outputs, and a unique or first hit policy), prove every input combination gets exactly the intended outcome, and turn it into one Given/When/Then rule table for a parameterized test. Use when an outcome depends on several inputs at once (eligibility, promotion, routing, conflict resolution, scope), when criteria multiply into near-identical variants, or when the user says "decision table", "rules table", or "what happens when both".
---

# Decision Table

`story` means the CLI from the **story-time** skill.

Prose rules hide gaps ("what if they are a guest and verified?") and
contradictions (two rules that both apply). A decision table lists every
combination, so the CLI can find both before anyone writes code.

## 1. Frame the rule

- **One decision.** Name it for what it decides (`promotion`,
  `conflict-resolution`, `team-scope`).
- **Outputs.** What the decision returns. Usually one value, sometimes a
  value and a reason.
- **Inputs.** Everything the outcome depends on, and nothing else.

The table lives at `model/rules/<rule>.toml`, in lowercase kebab-case.

## 2. Give every input a finite domain

List every value an input can take. Turn continuous values into named
bands (`total = ["under 50", "50 or more"]`), and write the boundary as its
own criterion with the **acceptance-criteria** skill, because the table
can't test where a band starts.

## 3. Write the table

```toml
name = "promotion"
hit_policy = "unique"
outputs = ["promote"]

[inputs]
role = ["admin", "member", "guest"]
verified = [true, false]

[[rules]]
when = { role = "admin", verified = "-" }
then = { promote = "yes" }

[[rules]]
when = { role = "member", verified = true }
then = { promote = "yes" }
```

- Every rule names every input in `when`. `"-"` means any value.
- Every rule gives every output in `then`.
- `hit_policy = "unique"`: exactly one rule matches each combination, so
  rule order doesn't matter. Prefer it.
- `hit_policy = "first"`: the first matching rule wins, so order matters.
  Use it only when precedence is the real rule, for example a final
  all-`"-"` default row.

The file is the same TOML subset as the **state-machine** skill's models.

## 4. Check

```bash
story table check model/rules/promotion.toml
```

It exits 1 and lists every defect. Re-run until it prints `OK`.

| Code | Means | Usual fix |
|---|---|---|
| `uncovered` | No rule matches this combination (each one is listed) | Ask what should happen, then add a rule. Never guess an outcome |
| `overlap` | Two rules match the same combination under `unique` (the pair is named) | Narrow one rule. Switch to `first` only if precedence is the business rule |
| `unreachable` | Under `first`, earlier rules match everything this one covers | Delete it, or move it above the rule that hides it |
| `schema` | A missing field, a value outside its domain, or more than 4096 combinations | Fix it, or split the decision into two tables |

## 5. Generate the criteria

```bash
story table criteria model/rules/promotion.toml
```

It prints one criterion followed by a table with one row per input
combination and the rule that decides it. `"-"` is expanded, so every row
is a concrete test case. Paste it into `## Acceptance Criteria` and log it
with `story note <id> "criteria from model/rules/<rule>.toml"`. The
**story-kickoff** skill turns it into one parameterized test. `--json`
gives the same `columns` and `rows` to generate that test from.

Record the table's path under `## Notes` so later edits find it. When the
rule changes, change the table, re-check, and regenerate. Don't edit the
generated rows.

## Report

The table path and its check result, the hit policy and why, the number of
combinations and rules, and any outcome you had to ask about.
