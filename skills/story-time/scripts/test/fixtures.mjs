// Shared builders for tests. Keep fixtures minimal and explicit.
export const READY_MD = `---
id: ST-001
title: Reset password via email
status: ready
type: story
epic: EP-1
persona: registered user
value: 8
time_criticality: 5
risk_reduction: 3
size: 3
depends_on: []
created: 2026-11-01
updated: 2026-11-02
---

## Story
As a registered user, I want to reset my password via an emailed link, so that I can regain access without contacting support.

## Acceptance Criteria
- [ ] Given a registered email, when I request a reset, then a single-use link valid for 30 minutes is emailed to me.
- [ ] Given an expired link, when I open it, then I see "link expired" and can request a new one.

## Notes
Uses existing mailer.

## Out of Scope
- SMS reset

## Changelog
- 2026-11-01 created
`;

export function story(overrides = {}) {
  return {
    id: 'ST-100',
    title: 'A story',
    status: 'ready',
    type: 'story',
    value: 5,
    time_criticality: 3,
    risk_reduction: 1,
    size: 3,
    depends_on: [],
    created: '2026-11-01',
    updated: '2026-11-01',
    sections: {
      Story: 'As a shopper, I want to save my cart, so that I can finish checkout later.',
      'Acceptance Criteria':
        '- [ ] Given items in my cart, when I sign out and back in, then the same items are in my cart.',
    },
    ...overrides,
  };
}

// A lifecycle with a guard, two terminal states and a way back.
export const ENROLLMENT = `entity = "enrollment"
states = ["draft", "submitted", "approved", "rejected"]
initial = "draft"
terminal = ["approved", "rejected"]
events = ["submit", "approve", "reject", "withdraw"]
invariants = ["An approved enrollment has a coverage start date."]

[guards]
complete = "every required field is filled in"

[[transitions]]
from = "draft"
to = "submitted"
event = "submit"
guard = "complete"

[[transitions]]
from = "submitted"
to = "approved"
event = "approve"

[[transitions]]
from = "submitted"
to = "rejected"
event = "reject"

[[transitions]]
from = "submitted"
to = "draft"
event = "withdraw"
`;

// A complete unique-hit table: 3 x 2 combinations, 4 rules.
export const PROMOTION = `name = "promotion"
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

[[rules]]
when = { role = "member", verified = false }
then = { promote = "no" }

[[rules]]
when = { role = "guest", verified = "-" }
then = { promote = "no" }
`;
