---
name: story-ready
description: Run the Definition of Ready gate on one or more stories — the automated INVEST lint plus the judgment checks a linter cannot make — and move passing stories to ready. Use when the user asks "is this ready?", "can we pull this into the sprint?", before sprint planning, or before story-pick.
---

# Story Ready

`story` means the CLI from the **story-time** skill. Only this skill moves a
story to `ready`.

## 1. Automated checks

```bash
story lint <id...> --json
```

Every `error` blocks. Route each one to the skill that fixes it:

| Rule | Fix with |
|---|---|
| `statement.*` | **story-edit** skill |
| `ac.present`, `ac.gherkin`, `ac.vague` | **acceptance-criteria** skill |
| `size.estimated`, `value.scored` | **story-estimate** skill |
| `size.small`, `ac.count` | **story-split** skill |
| `independent.deps` | Resolve the dependency, or sequence the work |
| `spike.*` | **spike** skill |

Warnings don't block, but report them.

## 2. Judgment checks

Lint can't see these. Start with:

```bash
story judge <id...> --json
```

It answers Negotiable (`negotiable`), Valuable (`valuable`), and Testable
(`criteria[].testable`), each with a confidence and its source (`jev`,
`code`, or `fallback`). A `no` with confidence of 0.5 or more is a gap to
fix. Anything lower is a prompt to look closely, not a verdict. Then check
each item below and give a one-line reason:

- **Negotiable.** The statement describes a need, not a solution.
- **Valuable.** The benefit is real to the named persona. A stakeholder
  would recognise it.
- **Independent.** Any unfinished `depends_on` is scheduled ahead of this
  story, or the story can ship behind a flag without it.
- **Inputs exist.** The designs, API contracts, test data, access, and
  third-party accounts the story needs are linked in Notes or not needed.
- **Understood.** Someone could start tomorrow without a meeting. No open
  questions left in Notes.
- **Agreed scope.** `## Out of Scope` exists if anything nearby could be
  confused for part of this story.
- **Team rules.** Every line in `definitionOfReady` from `story config`.

## 3. Decide

- **Pass:** `story set <id> status=ready --note "DoR passed"`. The CLI
  re-runs lint and refuses if anything regressed.
- **Fail:** set `status=refined` if it was `draft` and only small gaps remain,
  and list the gaps.

Never use `--force`. If the user wants to pull an unready story anyway, they
can say so explicitly. Record their reason in the note.

## Report

| Story | Verdict | Blocking gaps | Next skill |
|---|---|---|---|
