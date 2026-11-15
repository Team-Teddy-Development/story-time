# Story file format

One file per story at `backlog/<ID>.md`: flat YAML frontmatter, then `##` sections.

```markdown
---
id: ST-012
title: Reset password via emailed link
status: refined            # draft | refined | ready | in-progress | done | split | archived
type: story                # story | spike | bug | chore
epic: EP-002
persona: registered user
value: 8                   # user/business value          (1 2 3 5 8 13 20)
time_criticality: 5        # cost of waiting               (1 2 3 5 8 13 20)
risk_reduction: 3          # risk reduction / enablement   (1 2 3 5 8 13 20)
size: 3                    # relative effort               (1 2 3 5 8 13 20)
depends_on: [ST-010]
relates_to: [VC-109]      # optional: related stories that do not block this one
state_machine: model/state/enrollment.toml   # optional, see below
parent: ST-004             # set on children of a split
children: []               # set on a split parent
created: 2026-11-01
updated: 2026-11-04
---

## Story
As a registered user, I want to reset my password via an emailed link, so that I can regain access without contacting support.

## Acceptance Criteria
- [ ] Given a registered email, when I request a reset, then a single-use link valid for 30 minutes is emailed.
- [ ] Given an expired link, when I open it, then I see "link expired" and can request a new one.

## Notes
Conversation, decisions, estimate rationale, links to designs.

## Out of Scope
- SMS reset (ST-020)

## Test Plan
Written by story-kickoff.

## Changelog
- 2026-11-01 created
- 2026-11-04 set size=3 — planning poker, anchors ST-003 (2) and ST-007 (5)
```

`state_machine:` names the lifecycle model the story moves an entity
through, relative to the repo root (see the **state-machine** skill).
`story lint` reports `state_machine.exists` as an error when the file is
missing.

An open spike must be listed in at least one story's `depends_on`, or
`story lint` reports `spike.unblocks` as an error.

`depends_on` and `relates_to` can name a story of another repository of the
same org by its bare id, such as `VC-109`. `story lint` sees one repository,
so it reports such a link as the warning `independent.foreign_unresolved`.
`story mcp` with a `--root` for each repository checks the link:

- `independent.deps` is an error when no repository has the story.
- `links.ambiguous` is an error when two repositories declare the prefix of
  the id. Each repository declares the `prefix` in its settings.
- `links.prose_mention` is a warning when the text names a story of another
  repository that no link field lists.
- A spike is unblocked when a story of any repository lists it.

Spikes replace `## Story` / `## Acceptance Criteria` with `## Question` and
`## Exit Criteria`, and need `timebox:` (e.g. `2d`) instead of `size`.

Bugs keep the user statement, written as the behaviour the user expects, and add
`## Steps to Reproduce`, `## Expected`, and `## Actual`.

Frontmatter supports scalars, numbers, and inline lists only. No nesting.
