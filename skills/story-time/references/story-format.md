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

Spikes replace `## Story` / `## Acceptance Criteria` with `## Question` and
`## Exit Criteria`, and need `timebox:` (e.g. `2d`) instead of `size`.

Bugs keep the user statement, written as the behaviour the user expects, and add
`## Steps to Reproduce`, `## Expected`, and `## Actual`.

Frontmatter supports scalars, numbers, and inline lists only. No nesting.
