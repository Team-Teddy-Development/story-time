---
id: ST-001
title: 'Reset password: email'
status: ready
type: story
epic: EP-1
persona: registered user
value: 8
time_criticality: 5
risk_reduction: 3
size: 3
depends_on:
  - ST-000
children: []
created: 2026-11-01
updated: 2026-11-02
# Keep this local comment.
custom: "literal: metadata # retained"
---

## Story
As a registered user, I want to reset my password via email.

## Acceptance Criteria
- [ ] Given an email, when I request a reset, then I receive one link.
- [x] Given an expired link, when I open it, then I see "link expired".

## Notes
Uses existing mailer.

## Out of Scope
- SMS reset

## Test Plan
Local test commands stay local.

## Changelog
- 2026-11-01 created
