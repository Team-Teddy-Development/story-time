---
name: story-accept
description: Verify a finished story against each acceptance criterion and the Definition of Done with concrete evidence (tests, demo, PR), then accept it as done or reject it back with specific gaps; new scope found along the way becomes new stories. Use when the user says "is ST-xxx done?", "accept this story", "close the story", "review against the ACs", or when a story's PR merges.
---

# Story Accept

`story` means the CLI from the **story-time** skill.

Acceptance is binary for each criterion. "Mostly" is not done.
A refinement or sprint-planning artifact proves a local proposal, not story delivery,
sprint closure, or tracker assignment. Accept implementation only from criterion
and Definition of Done evidence. Future forecast groups remain forecasts.

## 1. Gather evidence

Run `story show <id>`. For each acceptance criterion, find evidence:

| Evidence | Counts when |
|---|---|
| A passing automated test named in the Test Plan | You ran it, or CI ran it on the final commit |
| A demo or manual check | You ran it and recorded the steps and the result |
| A screenshot or recording | For visual criteria, attached or linked |

Unproven is unmet. Do not accept a criterion because the code looks like it
handles it.

## 2. Definition of Done

The defaults below, plus every line in `definitionOfDone` from
`story config`. If the team has its own list elsewhere (`CLAUDE.md`,
`CONTRIBUTING.md`), use that too:

- [ ] Every acceptance criterion has evidence.
- [ ] Every Test Plan item is ticked. The suite is green on the final commit.
- [ ] The change is reviewed and merged, or the PR is ready with no
      unresolved review threads.
- [ ] No new lint, type, or security findings.
- [ ] User-facing docs, changelog, or release notes are updated if behaviour
      changed.
- [ ] Feature flags, migrations, and rollout steps are recorded in Notes.

## 3. Decide

**Accept.** Tick every criterion (`- [x]`) in the file, then:

```bash
story set ST-012 status=done --note "accepted: PR #123; all ACs verified (tests + manual check of AC2)"
```

If the story has `remote:` and `sync` is `ask` or `auto` (`story config`),
push it so the tracker item closes as completed: `story push ST-012`
(ask first when sync is `ask`).

**Reject.** Leave the story `in-progress`. List each unmet criterion and
exactly what is missing. Don't lower the bar by editing the criteria to fit
the implementation. If a criterion turns out to be wrong, that is a scope
change for the user to decide, via the **story-edit** skill.

**New scope found.** Behaviour that was discovered or requested along the
way and isn't in the criteria becomes a new story (the **story-create**
skill), linked in Notes. It never joins this story late.

## 4. Close the loop

- If other stories `depends_on` this one, tell the user they are now
  unblocked. `story rank` will include them.
- If the actual effort was far off the estimate (two or more Fibonacci
  steps), add one line to `## Notes` on why. The **story-estimate** skill
  uses done stories as anchors, so calibration notes make the next estimate
  better.

## Report

| Criterion | Evidence | Verdict |
|---|---|---|

Then the overall result: accepted or rejected, the changelog note, any
follow-up stories, and any stories this one unblocked.
