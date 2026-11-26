---
name: story-ready
description: Run the Definition of Ready gate on one or more stories — the automated INVEST lint plus the judgment checks a linter cannot make — and move passing stories to ready. Use when the user asks "is this ready?", "can we pull this into the sprint?", before sprint planning, or before story-pick.
---

# Story Ready

`story` means the CLI from the **story-time** skill. Only this skill moves a
story to `ready`.

## 0. Take the refined review queue first

With no story named, start from the refined review queue. It holds stories
that failed or skipped an earlier review and now wait in `refined`.

```bash
story rank --json
```

`refinedQueue` lists them oldest first, with `days` in refined (absent when
unknown). Review each one through steps 1 to 3 before any draft. Every review
ends one of two ways: promote the story to `ready`, or name the remaining gap.

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
| `independent.foreign_unresolved` | A warning: the link names a story of another repository. Check it with `story mcp --root <this repo> --root <that repo>` |
| `links.ambiguous` | Two repositories declare the prefix of a linked id. Give each repository its own `prefix` |
| `links.prose_mention` | A warning: add the named story to `depends_on` or `relates_to` with **story-edit** |
| `links.undeclared_prefix` | Only on a hosted server: add the prefix of the story to the `prefixes` of its repository in the registry file |
| `spike.unblocks` | Link the waiting story (`story set <story> depends_on=<spike>`), or archive the spike. A spike nobody waits on buys nothing |
| `spike.*` | **spike** skill |
| `state_machine.exists` | **state-machine** skill |
| `refined.stale` | A warning: the story has waited in `refined` past `team.refinedStaleDays`. Review it now |

Spikes and chores with exactly one acceptance criterion need no size.
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
- **Fail on a refined story:** `story set <id> status=refined --note "Gap: <what remains>"`.
  The note names the remaining gap, and the new changelog line restarts its
  clock in the refined review queue.

Never use `--force`. If the user wants to pull an unready story anyway, they
can say so explicitly. Record their reason in the note.

## Report

| Story | Verdict | Blocking gaps | Next skill |
|---|---|---|---|
