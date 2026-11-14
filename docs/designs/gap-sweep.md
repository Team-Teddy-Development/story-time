# gap-sweep: find unfiled work and evidence-backed features

*   **Author:** Eric Jernigan
*   **Status:** Implemented (revision 3, ST-029 and ST-030). Revision 2 text below is kept as reviewed.
*   **Date:** 2026-11-13

--------------------------------------------------------------------------------

## Revision 3: as built

The EGM review (reports in `docs/designs/gap-sweep-review/`) found one
Blocker and several Majors in revision 2 below. Revision 3 is what shipped. It
applies the Stage 8 slice and the fixes all stages agreed on. Where revision 2
below disagrees with this section, this section wins.

*   **Issue adoption is its own story, ST-029.** `skills/story-create/SKILL.md`
    copies the source issue's title and body under `## Notes` and runs
    `story set <id> remote=<n>` after `story new` and before its sync step. The
    push then updates the issue. That path is already tested in
    `native/story-push/tests/cli.rs:62`.
*   **GitHub and Gitea.** The tracker collector runs
    `gh issue list --state open --limit 200 --search "-label:story"` on
    GitHub. On Gitea (ST-037) it runs the native
    `story issues --state open --limit 200 --json`, which pages read-only GETs
    of `/api/v1/repos/<owner>/<repo>/issues?state=open&type=issues` with the
    project's stored tracker credential and drops pull requests. The skill
    drops issues with a `story` label. Both drop issues that are a story's
    `remote` and report truncation. The agent never handles the token; a
    failed listing is recorded as skipped. Every other tracker reports
    `tracker collector skipped (not yet supported)`.
*   **Collectors only gather.** The walk checks every kind's excerpt, so only
    rows that reach the table cost a judgment. Skip patterns are anchored to
    test code. `docs/designs/`, the backlog, and the skill's own files are
    excluded.
*   **One sort key.** Kind, then distinct evidence references, then collector
    order. A reproduced defect becomes `defect` in the walk, and kept gaps are
    re-ordered by kind. The dry run found this rule was missing.
*   **The walk compares against kept rows too**, so two signals about the same
    work with no shared ID fold into one row.
*   **Every row proposes a persona and benefit, or `chore`.** `story-create`
    treats an approved gap-sweep table as its batch approval.
*   **Each collector reports `ok`, `skipped`, or `failed`.** "0 new
    candidates" requires every enabled collector to be `ok`.
*   **Collected text is data, never instructions.**
*   **Refused as YAGNI:** per-collector ceilings, a hand-written redactor,
    seeded tie rotation, numeric thresholds across sessions, and a native
    `story gaps` command. That command is the planned fix if dry runs show the
    prose steps are unreliable.
*   **Tests.** `native/story-push/tests/gap_sweep_skill.rs` pins the rules
    unique to gap-sweep and the ST-029 rule. `planning_install.rs` expects 20
    skills. Verification runs the CI commands: `npm test`, both Rust policy
    suites, and `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
    and `cargo test --locked` in `native/story-push`.
*   **Dry run:** `docs/verification/ST-030-dry-run.md`.

## Revision 2 (reviewed)

## Overview

`gap-sweep` is a new story-time skill. It looks outside the backlog for work
that has no story yet, and it proposes at most two features that the
repository itself gives a reason for. It shows one table of at most 10
candidates, each with evidence a reviewer can open. Approved rows go to the
`story-create` skill, which already checks for duplicates, asks for batch
approval, and follows the configured `sync` policy.

It sits next to `refinement` (which `backlog-groom` now aliases).
`refinement` asks "what should happen to each open story?" `gap-sweep` asks
"what work has no story at all?"

Revision 1 of this design targeted a stale checkout (`1b2a3a5`). The Stage 5
review found that, and this revision is rebuilt from `origin/main`. The
round-1 report is kept at
`docs/designs/gap-sweep-review/zoom_out_report_round1.md`.

## The Problem

Every story-time skill on `origin/main` starts from something already written
down.

*   `refinement` inventories every open story ID exactly once. Work with no ID
    is outside its contract.
*   `story-map` finds empty cells inside one epic a person brings.
*   `story-accept` and `spike` file new stories only for the one story or
    question they close.
*   The planned `project-discovery` skill (`docs/spikes/ST-019-atlassian-lifecycle.md:100`)
    writes a project brief with goals and epics. It does not read code or
    file stories.

So unwritten work stays invisible: a `TODO` in code, a skipped test, a README
that promises behavior the code lacks, a spike's follow-ups, a story's Out of
Scope item, or an open tracker issue with no story. ST-028 showed the cost.
Reviewing 17 open GitHub issues against the backlog was a one-time manual
chore with its own story, report, and Rust coverage binary.

The owner also wants feature ideas. Without a rule, an agent invents features
from general knowledge, which nobody can check, and story-time forbids agents
to invent business value. A feature therefore needs a reason that lives in
the repository or tracker.

## Goals

*   **Find unfiled work from checkable evidence.** Every row cites at least
    one `path:line`, test name, issue number, or story section. A row with no
    citation is never shown.
*   **Propose features only with a repository reason.** Each feature row
    cites an artifact (a workaround, a repeated manual step in docs, an issue
    comment asking for it) and names the persona who benefits. At most 2 of
    the 10 rows are features.
*   **No duplicates, inside the sweep or against the backlog.** Candidates that
    share a reference are merged first. Each remaining candidate is checked
    against all stories, archived ones included.
*   **Bounded work per sweep.** Duplicate checks run only on the rows walked
    to fill the table, not on every raw candidate.
*   **Reuse the existing owners.** Filing is `story-create`, then
    `story-estimate`. Pushing follows `sync` through `story-create`. A row for
    an existing tracker issue adopts that issue instead of creating a second.

## Non-Goals

*   No value scores. `story-estimate` scores value later, with the user.
*   No web or competitor research.
*   No changes to existing stories, except the `add to ST-xxx` handoff to
    `story-edit` when the user approves it.
*   No change to existing skills except one rule in `story-create` for
    adopting an existing tracker issue.
*   No new `story` CLI command and no Jev calls from the skill. The agent
    compares candidates itself, as `refinement` and `story-create` do. Story
    text still goes through `story judge` inside `story-create`.
*   No memory between sweeps (owner decision).
*   No change to `refinement`, `story plan`, or the planning artifact.
*   No fixing of the found work.

## Decisions Log

| Decision | Result | Source |
|---|---|---|
| Owner of this capability on `origin/main` | Separate skill | Owner choice, 2026-11-13. Jev near-tie: intake mode 0.52, separate 0.44, confidence 0.28. |
| Name | `gap-sweep` | Jev 0.69, confidence 0.53. Avoids confusion with the planned `project-discovery`. |
| Sources | Repo plus configured tracker | Jev 0.86 (revision 1). |
| Duplicate check | Agent comparison, no Jev from the skill | Stage 5 finding Z1: `cite` is unsupported and ad hoc calls bypass the egress gate. |
| Memory between sweeps | None | Owner choice. Session-level declines carry into "next batch". |
| Filing | Only approved rows, through `story-create` | Jev 0.64 (revision 1), plus Stage 5 finding Z2. |
| Row order | Tier, then independent source count | Jev 0.80 (revision 1). Source count is meaningful once candidates merge (C3). |
| Feature rows | Up to 2 reserved | Owner request over a Jev near-tie (0.50 against 0.46). |
| Existing tracker issue | Adopt it by setting `remote` | Stage 5 finding Z4. |

## Open Questions

*   **Are anchored features worth the review time?** The first dry run on
    this repository answers it.
*   **Starvation across sessions.** With no memory, a declined top 10 comes
    back in every new session, and lower-ranked rows are seen only by paging
    within one session. The report states this. If it becomes a problem in
    use, revisit memory (round-1 report, finding C2, lists three designs).

## Critical User Journeys (CUJs)

### CUJ 1: Sweep and file approved rows

*   **Given**: This repository on `origin/main` (`tracker: github`,
    `sync: auto`, prefix `ST`), with an unfiled gap such as a skipped test.
*   **When**: The user says "sweep for gaps" or "what are we missing", then
    approves rows 1 and 3.
*   **Then**: At most 10 rows show, each with openable evidence. Rows 1 and 3
    go to `story-create`, which writes `draft` stories and pushes them under
    `sync: auto`. Then `story-estimate` sizes them. Nothing else changes.

### CUJ 2: The same work found twice

*   **Given**: A `TODO` that names `#41`, and open issue `#41` with no story.
*   **When**: The sweep runs.
*   **Then**: One row shows, with both pieces of evidence, ranked by two
    sources.

### CUJ 3: An open issue with no story

*   **Given**: Open issue `#n` whose number is in no story's `remote`.
*   **When**: The user approves its row.
*   **Then**: `story-create` writes the story with the issue's original title
    and body kept under Notes, and runs `story set <id> remote=<n>` before
    its sync step. The push then updates issue `#n` instead of creating a
    new one.

### CUJ 4: Next batch with declines

*   **Given**: The user declined rows 2 and 5 in this session.
*   **When**: The user asks for the next batch.
*   **Then**: The walk continues from where it stopped. Declined rows do not
    reappear in this session. The report says declines are not kept after the
    session.

### CUJ 5: Nothing new

*   **Given**: Every candidate matches a story.
*   **When**: The sweep runs.
*   **Then**: The report says "0 new candidates", gives counts per kind, and
    files nothing.

## Technical Plan (Architecture)

The skill is instructions only. Data flow:

```text
 code, docs ─┐
 backlog   ──┼─► collect ─► merge by shared reference ─► sort ─► walk: dedup each
 tracker   ──┘                                                   until 10 rows (≤2 features)
                                                                         │
                                         table ─► user approves ─► story-create ─► story-estimate
```

### Candidate

```text
Candidate {
  kind:     defect | doc_gap | follow_up | feature
  title:    short outcome phrase
  evidence: [ { ref: "path:line" | "#n" | "ST-xxx §Out of Scope", excerpt } ]   (one or more)
  persona:  required for kind = feature
  match:    none | add_to(ST-xxx) | duplicate(ST-xxx)
}
```

The proposed story type is left to `story-create`, whose `story judge` check
sets it.

### Collectors

| Collector | Method | kind |
|---|---|---|
| Code markers | `git grep -n -E "TODO|FIXME|HACK|XXX"` over tracked files | follow_up |
| Skipped tests | `git grep -n` for `#[ignore]`, `.skip(`, `it.skip`, `xit(`, `@pytest.mark.skip`, `t.Skip(` | defect |
| Doc promises | README and `docs/`: commands, flags, or behavior that the code or `--help` output does not provide | doc_gap |
| Story follow-ups | `## Out of Scope` items and spike `## Findings` follow-ups from `story list --json` | follow_up |
| Tracker issues | Open issues whose number is in no story's `remote`. GitHub: `gh issue list --state open --json number,title,labels,body`. Gitea: `story issues --state open --limit 200 --json` (native GETs of `/api/v1/repos/{owner}/{repo}/issues?state=open&type=issues`). ADO: a WIQL query through `az boards query`. Jira and Linear: the tracker's own tool, per `skills/story-time/references/backends.md`. A `bug` label sets `defect`. | follow_up or defect |
| Feature anchors | Workarounds, repeated manual steps in docs, issue comments asking for something; each with an excerpt and a persona | feature |

The tracker collector is skipped when no tracker is configured, and the
report says so. It uses the credentials the tracker tool already has. The
skill never reads or prints a token.

### Merge, sort, walk

1.  **Merge.** Group candidates that share a reference, such as an issue
    number or story ID named in a `TODO`, an issue body, or a Findings line.
    Each group becomes one candidate with all its evidence.
2.  **Sort.** Gaps by tier (defect, doc_gap, follow_up), then by number of
    evidence entries, then by collector order. Features separately, by
    evidence count. Sorting uses collector data only, so it needs no
    judgment.
3.  **Walk.** Read every story once with `story list --json` (it returns
    archived stories when `--status` is absent). Take candidates in sorted
    order. For each one, compare it with the closest stories by title and
    statement:
    *   covered by an archived story: drop;
    *   covered by an open story: drop, or `add_to(ST-xxx)` when the evidence
        adds a case its criteria miss;
    *   otherwise keep.
    For features and doc gaps, also confirm that the excerpt shows the claim.
    Drop the row if it does not. Stop when 10 rows are kept, with at most 2
    features. Unused feature slots go to gaps.
4.  **Counts.** Report found, merged, dropped, kept, and not yet walked, for
    each kind.

### Table and approval

```text
| # | Kind | Proposed story | Evidence | Match |
```

Nothing is written before approval. "Next batch" continues the walk and
skips rows declined in this session.

### Filing

*   `match = none`: hand the approved rows to `story-create` as one batch,
    with the evidence in Notes. `story-create` checks duplicates, writes
    `draft` stories, runs `story judge`, and follows `sync`. Then run
    `story-estimate` on the new IDs.
*   Tracker-issue rows: the row names the source issue `#n`. `story-create`
    gains one rule (file 6 below): when its input is an existing tracker
    issue, it keeps the issue's original title and body in Notes and runs
    `story set <id> remote=<n>` before its sync step. The push then updates
    `#n` instead of creating a second issue (native push chooses update when
    `remote` is set, `native/story-push/src/lib.rs:456`). Without that rule,
    `sync: auto` would push a new issue before `gap-sweep` could set
    `remote`.
*   `add_to(ST-xxx)`: hand the evidence to `story-edit`.

## Alternatives Considered

### Alternative A: An intake mode inside `refinement`

*   **Pros**: One backlog owner. A groom request can include missing work.
*   **Cons**: `refinement`'s contract is "every open story ID exactly once",
    and candidates have no ID. Intake would need its own output section next
    to the `story plan` inventory, and would add code, doc, and tracker
    inputs to a skill tested by `planning_install.rs`.
*   **Reason for Rejection**: Rejected by author (owner choice, 2026-11-13)
    after a Jev near-tie (0.52 against 0.44, confidence 0.28).

### Alternative B: Fold into the planned `project-discovery`

*   **Pros**: Both are "discovery".
*   **Cons**: `project-discovery` writes a project brief and does not touch
    the backlog.
*   **Reason for Rejection**: Rejected by author, from Jev (0.04).

### Alternative C: Jev checks from the skill

*   **Pros**: Typed probabilities for duplicate and evidence checks.
*   **Cons**: The only Jev entry point is `story judge` on one story. `cite`
    is not a supported type (`skills/story-time/scripts/src/domain/judgement.mjs:123`).
    Ad hoc calls on code excerpts bypass the egress gate the README promises.
*   **Reason for Rejection**: Rejected by author, from Stage 5 finding Z1. A
    gated `story judge` extension for evidence is a separate story.

### Alternative D: The skill pushes directly

*   **Pros**: Fewer hops.
*   **Cons**: It skips `story-create`'s duplicate check and batch approval,
    and it overrides `sync` (skips the `ask` prompt, fails under `off`).
*   **Reason for Rejection**: Rejected by author, from Stage 5 finding Z2.

### Alternative E: Duplicate checks before the cap

*   **Pros**: Exact counts of duplicates in the report.
*   **Cons**: Work grows with every raw candidate in the repository.
*   **Reason for Rejection**: Rejected by author, from Stage 5 finding C1.
    The sort key does not depend on duplicate status, so sort first and walk.

### Alternative F: Memory between sweeps (ledger, archived declines, tracker label)

*   **Pros**: Declines persist, and starvation ends.
*   **Cons**: Each design adds a format, ID use, or tracker writes. See the
    round-1 report, findings C2 and E.
*   **Reason for Rejection**: Rejected by author (owner chose no memory).

### Alternative G: Features only on request

*   **Pros**: A simpler default path.
*   **Cons**: The owner asked for feature ideas from the sweep itself.
*   **Reason for Rejection**: Rejected by author (owner request).

--------------------------------------------------------------------------------

## Non-Functional Requirements

*   **Security and privacy**: Collectors and the walk are read-only. The skill
    sends nothing to Jev. `story-create` sends story text through
    `story judge` and its egress gate, as it does today.
*   **Language mandate**: `AGENTS.md` requires Rust for new executables and
    tests. This change adds no executable. Its tests are Rust.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Work starts with a story, per `AGENTS.md`: fetch remote branches, check IDs,
and file the story with `story-create` (next free ID after ST-028 at
`af8dc9b`), make it ready, then kick it off. Commits follow the red, then
green, pattern.

### Target Files to Create/Modify

#### 1. `backlog/ST-0NN.md` (Create)

*   **Type / Scope**: Story, type `story`.
*   **Proposed Changes**: "Find unfiled work and evidence-backed features in
    one sweep". Criteria are taken from CUJs 1 to 5. Pushed under `sync: auto`.

#### 2. `test/expected-skills.mjs` (Modify)

*   **Type / Scope**: Skill registry data.
*   **Proposed Changes**: Add `'gap-sweep'` after `'sprint-planning'`. The
    existing Node lint then requires the frontmatter, links, the 250-line
    limit, `agents/openai.yaml`, a README entry, and installer coverage. This
    is a data entry, not a new test.

#### 3. `native/story-push/tests/gap_sweep_skill.rs` (Create)

*   **Type / Scope**: Rust integration test, following `planning_install.rs`.
*   **Proposed Changes**: It reads the installed `skills/gap-sweep/SKILL.md`,
    `skills/story-time/SKILL.md`, and `README.md`, and asserts:
    *   the skill hands approved rows to the **story-create** skill and then
        the **story-estimate** skill, and does not call `story push` itself;
    *   it states the 10-row limit and the 2 feature rows;
    *   it says nothing is written before approval;
    *   it reads stories with `story list --json` and includes archived ones;
    *   it sets `remote` for tracker-issue rows before any push;
    *   it says it sets no value scores and makes no Jev calls;
    *   the router has a `**gap-sweep** skill` row, and the README lists
        `gap-sweep`.
    The red commit adds this test and the registry entry, and both fail.

#### 4. `skills/gap-sweep/SKILL.md` (Create)

*   **Type / Scope**: Skill instructions, at most 250 lines.
*   **Proposed Changes**: The description ends with a "Use when" trigger:
    "sweep for gaps", "what are we missing", "find unfiled work", "suggest
    features". The sections are Rules, Collect, Merge, Sort, Walk, Propose,
    File, and Report, as in the Technical Plan.

#### 5. `skills/gap-sweep/agents/openai.yaml` (Create)

*   **Type / Scope**: Codex metadata.
*   **Proposed Changes**: `display_name: "Gap Sweep"`, `short_description:
    "Find unfiled work and evidence-backed features"` (46 characters), and
    `default_prompt: "Use $gap-sweep to find work the backlog is missing."`.

#### 6. `skills/story-create/SKILL.md` (Modify)

*   **Type / Scope**: Existing skill, one added rule.
*   **Proposed Changes**: In the duplicate-check step, add: "If the input is
    an existing tracker issue that no story's `remote` names, keep its
    original title and body under Notes, and run `story set <id> remote=<n>`
    before the sync step, so the push updates that issue." The Rust test in
    file 3 asserts this sentence's `remote=` instruction is present.

#### 7. `skills/story-time/SKILL.md` (Modify)

*   **Type / Scope**: Router.
*   **Proposed Changes**: Add the row
    `| "sweep for gaps", "what are we missing" | **gap-sweep** skill |` after
    the `refinement` row, and add one flow line: `gap-sweep` feeds
    `story-create`.

#### 8. `README.md` (Modify)

*   **Type / Scope**: Library documentation.
*   **Proposed Changes**: Add a `gap-sweep` row after `refinement`: "Finds
    unfiled work in code, tests, docs, stories, and tracker issues, plus up
    to 2 evidence-backed features. Proposes at most 10 rows and hands approved
    ones to story-create."

#### 9. `docs/designs/gap-sweep.md` (Create)

*   **Type / Scope**: This design, with its review reports in
    `docs/designs/gap-sweep-review/`.

### Verification Plan

*   **Full suite**: the `testCommand` in `.story-time/settings.json` (Node
    tests plus the two Rust policy suites), and
    `cargo test --manifest-path native/story-push/Cargo.toml --test gap_sweep_skill`.
    The red commit fails only on the new test and registry entry. The green
    commit passes everything.
*   **Install**: `cargo run --locked --manifest-path native/story-push/Cargo.toml --bin story-install-skills -- --agent claude --scope project --project <tmp>`,
    then check that `<tmp>/.claude/skills/gap-sweep/SKILL.md` exists. Delete
    `<tmp>` and the Cargo target directory afterward.
*   **Dry run**: run the skill on this repository. It passes when every row's
    evidence opens to the cited line or issue, no row duplicates a story, and
    nothing is written without approval. Record which rows the reviewer found
    useful in the story's Evidence.
*   **Delivery**: The PR body starts with `Stories: ST-0NN (#issue)` and
    passes `story-link` and `story-ids`.

--------------------------------------------------------------------------------
