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
