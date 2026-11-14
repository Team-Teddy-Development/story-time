# backlog-discover: find missing work and propose features as stories

*   **Author:** Eric Jernigan
*   **Status:** Proposed
*   **Date:** 2026-11-13

--------------------------------------------------------------------------------

## Overview

`backlog-discover` is a new story-time skill. It looks outside the backlog for
work that has no story yet, and it proposes a small number of new features that
the repository itself gives a reason for. It shows the results as one table of
candidate stories, each with evidence a reviewer can open and check. Approved
rows become stories through the skills that already exist: `story-create`,
`acceptance-criteria`, and `story-estimate`, and then `story push`.

It works next to `backlog-groom`. Groom asks "are the stories we have in good
shape?" Discover asks "what work do we have no story for?"

## The Problem

Every story-time skill today starts from something a person already wrote down.

*   `backlog-groom` reads only existing stories. It finds stale, duplicate,
    oversized, unclear, unscored, and blocked stories. It cannot see work that
    was never filed.
*   `story-map` finds empty cells only inside one epic that a person brings.
*   `story-accept` and `spike` file new stories, but only for the one story or
    question they are closing.

So work that nobody wrote down stays invisible to the backlog. Examples are a
`TODO` left in code, a test marked to be skipped, a README that promises a
command the CLI does not have, a spike's Findings that name follow-up work, or
a story's Out of Scope item that never became its own story. Open tracker
issues that never became stories have the same problem. The backlog then looks
healthier than the project is, and the WSJF ranking from `story-pick` orders
only the work someone remembered to file.

The second part of the problem is new features. The owner wants the sweep to
suggest features that would help. With no rule, an agent will invent features
from general knowledge. Those ideas cannot be checked, and they fill the
backlog with speculation. Story-time also forbids agents to invent business
value or deadlines. A feature proposal therefore needs a reason that lives in
the repository or tracker, not in the agent's opinion.

## Goals

*   **Find unfiled work from checkable evidence.** Each candidate cites at
    least one file and line, test name, issue number, or story section that a
    reviewer can open. A candidate with no citation is not shown.
*   **Propose features only when the repository gives a reason.** Each feature
    candidate cites an artifact (for example, a workaround in code, a manual
    step repeated in docs, or an issue comment asking for it) and names the
    persona who benefits.
*   **Do not file duplicates.** Each candidate is compared with the open
    stories from `story list` before it is shown. A candidate that matches an
    open story is dropped or shown as "add to ST-xxx" instead of as a new story.
*   **Keep one sweep small enough to review.** One sweep shows at most 10
    candidates, ordered by evidence tier. The report states how many
    candidates were found in total and how many were cut.
*   **Reuse the existing pipeline.** Approved candidates go through
    `story-create`, `acceptance-criteria`, `story-estimate`, and `story push`,
    so they meet the same Definition of Ready as stories written by hand.

## Non-Goals

*   **No value scores.** The skill does not set business value, time
    criticality, or risk reduction. `story-estimate` scores those later, with
    the user.
*   **No web or competitor research.** The first version reads the repository
    and the configured tracker only.
*   **No changes to existing stories.** Merging, splitting, archiving, and
    re-scoring stay with `backlog-groom`, `story-split`, and `story-edit`.
*   **No new `story` CLI command.** Duplicate checks use `story list` and the
    agent's comparison, as `backlog-groom` does today.
*   **No implementation of the found work.** The skill files stories. It does
    not fix the `TODO` or the skipped test it found.
*   **No full scan of very large repositories in one pass.** The 10-row cap is
    deliberate. More candidates come from asking for the next batch.

## Decisions Log

The owner delegated these decisions to Jev on 2026-11-13. Each was asked as a
separate typed question with the evidence shown.

| Decision | Result | Jev probability / confidence | Note |
|---|---|---|---|
| Separate skill or groom step | Separate skill | 1.00 / 1.00 | |
| Sources | Repo plus tracker | 0.86 / 0.79 | |
| Duplicate check | Agent compare, Jev `noul` per close pair | 0.95 / 0.92 | |
| Memory between sweeps | None | owner choice | Rows that are not filed come back. |
| Filing | File only approved rows | 0.64 / 0.46 | Flipped from split policy (0.64) once "no memory" was added: auto-filed rows could never be declined. |
| Row order | Tier, then number of independent sources | 0.80 / 0.70 | No agent overrides. |
| Feature rows | Up to 2 of 10 rows reserved | owner request over Jev near-tie | Jev: lowest tier 0.50, reserved 0.46, confidence 0.25. Lowest tier can show zero features in any repo with 10 or more gaps, which defeats the owner's request for feature ideas. |

## Open Questions

*   **Are anchored features worth the review time?** Jev gave 0.60 confidence
    to anchored features and 0.27 to "gaps only." The first dry run should
    show whether the 2 reserved rows earn their place.
*   **Repeat rows**: With no memory, a declined row returns on every sweep.
    If that becomes noise in practice, revisit the ledger option in
    Alternatives Considered.

## Critical User Journeys (CUJs)

### CUJ 1: Sweep a repository and file approved gaps

*   **Given**: A repository with `.story-time/settings.json`, a backlog, and
    at least one unfiled gap (for example `#[ignore]` on a test, or a spike
    Findings item with no story).
*   **When**: The user says "discover", "find gaps", or "what are we
    missing", and later approves rows 1 and 3.
*   **Then**: The skill shows at most 10 rows, each with a source reference
    the user can open. Only rows 1 and 3 become `draft` stories. Each passes
    through `acceptance-criteria` and `story-estimate`, and is published with
    `story push` when the tracker is mirrored. No other story changes.

### CUJ 2: A candidate already has a story

*   **Given**: An open or archived story already covers the work behind a
    `TODO`.
*   **When**: The sweep finds that `TODO`.
*   **Then**: The candidate is not shown as a new story. If the match is open
    and the `TODO` adds something the story lacks, the row reads
    "add to ST-xxx" and points to `story-edit`. If the match is archived, the
    row is dropped.

### CUJ 3: An open tracker issue has no story

*   **Given**: A mirrored tracker with an open issue whose number is not in
    any story's `remote` field.
*   **When**: The sweep runs.
*   **Then**: The issue appears as a candidate citing its issue number. If
    approved, the new story is created and pushed, and the original issue is
    left as it is. The report names the original issue so the user can close
    or link it.

### CUJ 4: An anchored feature

*   **Given**: Code or docs show a repeated manual step (for example, a README
    section that asks users to run three commands in sequence each time).
*   **When**: The sweep runs.
*   **Then**: The feature row cites that section, names the persona who runs
    the steps, and uses one of the at most 2 reserved feature rows. It has no
    value score.

### CUJ 5: A sweep with nothing to show

*   **Given**: Every candidate matches an existing story.
*   **When**: The sweep runs.
*   **Then**: The skill reports "0 new candidates", the count it found and
    dropped as duplicates, and files nothing.

## Technical Plan (Architecture)

The skill is instructions for the agent. It adds no CLI code. The agent
collects raw signals, turns each into one candidate record, drops duplicates,
orders and cuts the list, shows the table, and runs the existing skills on the
approved rows.

```text
 repo files ──┐
 backlog   ───┼─► collect ─► candidates ─► dedup ─► order ─► cap 10 ─► table
 tracker   ───┘                              │                          │
                                     story list (all statuses)    user approves
                                                                         │
             story-create ─► acceptance-criteria ─► story-estimate ─► story push
```

### Data shape: the candidate

Every signal becomes one record of this shape before any judgment runs.

```text
Candidate {
  kind:      defect | doc_gap | follow_up | feature
  title:     short outcome phrase
  evidence:  [ { ref: "path:line" | "#issue" | "ST-xxx §Findings", excerpt } ]  (1 or more)
  persona:   required when kind = feature, else optional
  type:      bug | story | chore | spike   (proposed story type)
  match:     none | add_to(ST-xxx) | duplicate(ST-xxx)
}
```

`kind` is set by the collector that found the signal, not by later judgment.
A candidate with an empty `evidence` list is invalid and is never shown.

### Collectors

Each collector is a read-only command. `kind` and the default `type` come from
the collector.

| Collector | Source and method | kind | type |
|---|---|---|---|
| Code markers | `git grep -n -E "TODO|FIXME|HACK|XXX"` over tracked files | follow_up | chore |
| Skipped tests | `git grep -n` for the stack's skip markers: `#[ignore]`, `.skip(`, `it.skip`, `xit(`, `@pytest.mark.skip`, `t.Skip(` | defect | bug |
| Doc promises | README and `docs/`: commands, flags, or behavior the docs describe that the code does not provide. The agent checks each claim against the code or `--help` output. | doc_gap | story |
| Story follow-ups | Each story's `## Out of Scope` items and spike `## Findings` follow-ups, read from the backlog directory | follow_up | story |
| Tracker issues | Open issues: `gh issue list --state open --json number,title,labels,body` for GitHub, or `GET /api/v1/repos/{owner}/{repo}/issues?state=open&type=issues` for Gitea. Drop issues whose number matches any story's `remote`. A `bug` label sets kind `defect`. | follow_up or defect | story or bug |
| Feature anchors | Workarounds in code, repeated manual steps in docs, and issue comments that ask for something. Each needs a cited excerpt and a persona. | feature | story |

The tracker gateway port in `scripts/src/ports/tracker-gateway.mjs` has only
`create`, `update`, and `remove`, so the agent reads issues with the tracker's
own tool. This follows the agent-as-adapter pattern in
`skills/story-time/references/backends.md`. With no tracker, or `sync: off`,
the tracker collector is skipped and the report says so.

### Duplicate check

The agent reads every story, archived ones included, with `story list` and
`story show`. For each candidate it picks the closest stories by title and
statement. For each close pair it asks one Jev `noul`: "Does ST-xxx already
cover the work in this evidence?" The result sets `match`:

*   Yes, and the story is archived: drop the candidate.
*   Yes, and the story is open: drop it, unless the evidence adds a case the
    story's criteria miss. Then set `add_to(ST-xxx)`.
*   No: `match = none`.

Archived stories are included so that work someone already declined does not
return. This partly offsets having no sweep memory.

### Order and cap

1.  Gap rows are sorted by tier: defect, then doc_gap, then follow_up. Inside
    a tier, rows with more independent evidence entries come first. Ties keep
    collector order.
2.  Feature rows are sorted by number of evidence entries.
3.  The table takes up to 2 feature rows, then fills the rest of the 10 with
    gap rows. Unused feature slots go to gap rows.
4.  The report states found, dropped as duplicate, and cut by the cap, for
    each kind.

### Table and approval

```text
| # | Kind | Proposed story | Evidence | Match | Type |
```

The user approves rows (all, some, or edited). Nothing is filed before that.
Rows that are not approved are not recorded, and they can appear on the next
sweep.

### Filing approved rows

For each approved row with `match = none`:

1.  Run `story-create` with the title, persona, and evidence in Notes. The
    story starts as `draft`.
2.  Run `acceptance-criteria`, then `story-estimate` for size. Value inputs
    stay with the user in `story-estimate`.
3.  Run `story push <id> --dry-run`, check the target, then `story push <id>`.

For `add_to(ST-xxx)` rows, hand the evidence to `story-edit` on that story.
Filing several rows is a sequence of independent creates. A failure on one row
does not undo the others; the report lists which rows were filed and which
failed.

### Where Jev is used

| Decision | Jev call |
|---|---|
| Does an existing story cover this candidate? | `noul` per close pair |
| Does a feature's cited artifact show the need it claims? | `noul` per feature candidate; drop on "no" |
| Is a doc claim unmet by the code? | `cite` with the doc excerpt and the code or `--help` output |

When Jev is unreachable, the agent still runs the sweep. It marks each
affected row "unchecked" and states this in the report.

## Alternatives Considered

### Alternative A: Add a gaps step to `backlog-groom`

*   **Pros**: One sweep, one table, one trigger phrase to remember.
*   **Cons**: Groom reads the backlog only, and discovery reads code, docs,
    and the tracker. Groom changes existing stories, while discovery creates
    new ones. One skill would carry two different jobs and a much longer
    run.
*   **Reason for Rejection**: Rejected by author, from a Jev judgment (1.00):
    separate inputs and outputs make it a separate skill. Groom stays fast
    enough to run before every sprint planning.

### Alternative B: Add web and competitor research as a source

*   **Pros**: Can propose features the repository gives no hint of.
*   **Cons**: Such evidence cannot be checked in the repo or the tracker. It
    returns the unchecked agent opinion that the Goals rule out, and it sends
    project context to outside services.
*   **Reason for Rejection**: Rejected by author, from a Jev judgment (0.86
    for repo plus tracker, 0.01 for adding web research).

### Alternative C: A new `story` CLI command for similarity scoring

*   **Pros**: Repeatable duplicate checks that tests can cover.
*   **Cons**: The skill would wait on new CLI work. Text similarity also
    misses stories that cover the same work in different words, and that is
    the case that matters most.
*   **Reason for Rejection**: Rejected by author, from a Jev judgment (0.95
    for agent comparison). Revisit if the dry runs show that agent
    comparison misses duplicates.

### Alternative D: File evidence-backed gaps without asking (split policy)

*   **Pros**: Defects and unfiled follow-ups reach the backlog without a
    review step.
*   **Cons**: Because there is no memory between sweeps, a row filed without
    asking can only be removed by archiving it afterward. Each such row also
    creates a tracker issue that nobody reviewed. `backlog-groom` asks before
    every change, so the two sweeps would act differently.
*   **Reason for Rejection**: Rejected by author, from a Jev judgment (0.64
    for approve-only, 0.31 for split policy, once "no memory" was settled).

### Alternative E: A committed ledger of past candidates

*   **Pros**: Declined rows stay declined. Reviewers can see past verdicts.
*   **Cons**: Adds a file format that only this skill reads. Fingerprints
    break when code moves, so candidates come back anyway. Two sweeps at the
    same time conflict on the file.
*   **Reason for Rejection**: Rejected by author: the owner chose no memory
    for the first version. Revisit if repeat rows become noise.

### Alternative F: File declined rows as archived stories

*   **Pros**: Reuses the backlog, so the duplicate check drops declined work
    with no new format.
*   **Cons**: Each declined row uses a story ID and, when mirrored, a closed
    tracker issue. Frequent declines fill the backlog and tracker with noise.
*   **Reason for Rejection**: Rejected by author: the owner chose no memory,
    and Jev gave this filing option 0.05.

### Alternative G: Features fill only rows left after all gaps

*   **Pros**: Gaps with stronger evidence always come first.
*   **Cons**: Any repo with 10 or more gap candidates shows zero features on
    every sweep. The owner asked for feature ideas.
*   **Reason for Rejection**: Rejected by author: Jev returned a near-tie
    (0.50 against 0.46 for reserved rows, confidence 0.25), and the owner's
    request decided it.

### Alternative H: The agent scores WSJF value and orders by it

*   **Pros**: The table would match the order `story-pick` uses later.
*   **Cons**: Story-time forbids agents to invent business value or
    deadlines. The order would rest on guesses presented as numbers.
*   **Reason for Rejection**: Rejected by author, from a Jev judgment (1.00
    for evidence-tier order).

--------------------------------------------------------------------------------

## Non-Functional Requirements

*   **Security and privacy**: Collectors are read-only. Tracker reads use the
    credentials story-time already uses (`gh` auth, or the Gitea token from
    `GITEA_TOKEN` or `.story-time/.gitea.env`). The skill never prints a
    token. Jev receives only candidate evidence excerpts and story text.
*   **Cost**: Jev calls grow with candidates. The cap applies before the
    feature `noul` and the doc `cite` calls, so at most 10 of those run per
    sweep. Duplicate `noul` calls run on every candidate that has a close
    story, before the cap, because the cap needs the duplicates removed.
*   **Language mandate**: The change adds no executable code. It adds one
    instruction file, one metadata file, and assertions in the existing test
    suite. The Rust mandate for new executables does not apply.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

The repository convention is a red commit (tests and the expected-skills
entry), then a green commit (the skill), as in
`test/expected-skills.mjs:1`. Plugin versions stay at `0.1.0`; the last skill
additions (`state-machine`, `decision-table`) did not change them.

### Target Files to Create/Modify

#### 1. `test/expected-skills.mjs` (Modify)

*   **Type / Scope**: Test registry.
*   **Proposed Changes**: Add `'backlog-discover'` after `'backlog-groom'`.
    This makes the existing tests require the directory, valid frontmatter,
    a "Use when" trigger, resolving links and skill references, at most 250
    lines, `agents/openai.yaml` metadata, a README entry, and installer
    coverage (`test/install.test.mjs:52`).

#### 2. `test/skills.test.mjs` (Modify)

*   **Type / Scope**: Library lint tests.
*   **Proposed Changes**: Add one test,
    `backlog-discover proposes from evidence and files only approved rows`,
    that reads `skills/backlog-discover/SKILL.md` and asserts that it:
    *   hands approved rows to the **story-create**, **acceptance-criteria**,
        and **story-estimate** skills;
    *   uses `story push` and `--dry-run`;
    *   names the 10-row cap and the 2 reserved feature rows;
    *   says that nothing is filed before the user approves;
    *   includes archived stories in the duplicate check (`story list`);
    *   says the skill sets no value scores.
    Add the same skill to the existing
    `story-time routes ... ` style test, as a new assertion that
    `skills/story-time/SKILL.md` contains `**backlog-discover** skill`.

#### 3. `skills/backlog-discover/SKILL.md` (Create)

*   **Type / Scope**: Agent skill instructions, at most 250 lines.
*   **Proposed Changes**: Frontmatter with `name: backlog-discover` and a
    description that ends in a "Use when" trigger: "find gaps", "what are we
    missing", "discover work", "suggest features", or after `backlog-groom`
    when the ready buffer is low. Body sections, in order:
    1.  **Rules**: evidence required for every row; nothing filed before
        approval; no value scores; at most 10 rows, up to 2 for features.
    2.  **Collect**: the six collectors from the Technical Plan, with the
        exact commands, and the rule to skip the tracker collector when no
        tracker is set or `sync` is `off`.
    3.  **Build candidates**: the Candidate shape, written as a field list.
    4.  **Drop duplicates**: `story list` over all statuses, the Jev `noul`
        per close pair, and the three outcomes (drop, `add_to`, none).
    5.  **Check features and doc claims**: Jev `noul` for feature anchors and
        `cite` for doc claims, run after the cap.
    6.  **Order and cap**: the four steps from the Technical Plan.
    7.  **Propose**: the table columns, and the counts line (found, dropped,
        cut, per kind).
    8.  **File approved rows**: the **story-create**, **acceptance-criteria**,
        **story-estimate** sequence, `story push <id> --dry-run` then
        `story push <id>`, and `add_to` rows handed to the **story-edit**
        skill.
    9.  **Report**: rows filed with IDs and issue URLs, rows that failed,
        rows marked "unchecked", and a line that declined rows can return on
        the next sweep.

#### 4. `skills/backlog-discover/agents/openai.yaml` (Create)

*   **Type / Scope**: Codex interface metadata.
*   **Proposed Changes**: `display_name: "Backlog Discover"`,
    `short_description: "Find unfiled gaps and evidence-backed features"`
    (45 characters, inside the 25 to 64 limit), and
    `default_prompt: "Use $backlog-discover to find work the backlog is missing."`.

#### 5. `skills/story-time/SKILL.md` (Modify)

*   **Type / Scope**: Router skill.
*   **Proposed Changes**: Add the row
    `| "find gaps", "what are we missing", "suggest features" | **backlog-discover** skill |`
    after the `backlog-groom` row. Change the line under the flow diagram to
    say that `backlog-groom` and `backlog-discover` run on a cadence:
    groom over existing stories, discover for work with no story.

#### 6. `README.md` (Modify)

*   **Type / Scope**: Library documentation.
*   **Proposed Changes**: Add a `backlog-discover` row after `backlog-groom`
    in the skill table: trigger "what are we missing?", and the summary
    "Finds unfiled work in code, tests, docs, stories, and tracker issues,
    plus up to 2 evidence-backed features. Proposes at most 10 rows, then
    files the approved ones." Update the flow text line the same way as in
    the router.

#### 7. `docs/designs/backlog-discover.md` (Create)

*   **Type / Scope**: Design document.
*   **Proposed Changes**: The final version of this draft, written at the
    finalization step.

### Verification Plan

*   **Unit tests**: `npm test` (runs `node scripts/run-tests.mjs`). The red
    commit must fail on the new `backlog-discover` tests and the expected
    list. The green commit must pass the full suite.
*   **Install check**: `node scripts/install.mjs --agent claude --scope project --project <tmp dir>`
    then confirm `<tmp dir>/.claude/skills/backlog-discover/SKILL.md`
    exists. Delete the temp directory after.
*   **Dry run (the real check)**: Run the skill on this repository. Pass
    when every shown row cites evidence that opens to the stated line or
    issue, no row duplicates an existing story, and nothing is filed
    without approval. Record the count of rows the reviewer found useful;
    this answers the open question about feature rows.
*   **Work tracking**: This repository has no `.story-time/settings.json`
    and no backlog directory. The `chore` story that Rule 1 requires needs a
    home. Either set up story-time here first with the **story-setup**
    skill, or track the change in the repository's GitHub issues and name
    it on the PR's `Stories:` line.

--------------------------------------------------------------------------------

*Drafted with EGM*
