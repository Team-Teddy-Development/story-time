---
name: acceptance-criteria
description: Write or harden a story's acceptance criteria as testable Given/When/Then statements covering the happy path, failure paths, boundaries, permissions, and non-functional thresholds, each of which maps directly to an automated test. Use when the user asks for ACs, edge cases, or "how will we know it's done", or when lint flags ac.present, ac.gherkin, or ac.vague.
---

# Acceptance Criteria

`story` means the CLI from the **story-time** skill.

Acceptance criteria are the contract for done. Each one becomes at least
one failing test in the **story-kickoff** skill, so write them so they can
fail.

## Shape

```
- [ ] Given <context>, when <action>, then <observable outcome>.
```

- **One behaviour per criterion.** Two "then"s means two criteria.
- **Observable outcomes.** What the user, an API client, or an operator can
  see. Not internal state.
- **Concrete values.** "within 30 minutes", "at most 5 attempts", "sorted by
  newest first". `story lint` flags vague words like fast, easy, intuitive,
  properly, and etc.
- **The domain's language, not the UI's.** "When I submit the order", not
  "when I click the blue button", unless the button is the requirement.
- **Examples for rule tables.** If a rule varies by input, add a small table
  under the criterion instead of five near-identical criteria:

  ```
  - [ ] Given an order total, when shipping is calculated, then the fee follows:
    | total   | fee   |
    | < $50   | $5.99 |
    | >= $50  | free  |
  ```

## Coverage prompts

Go through these and keep only the ones that matter for this story. Fewer,
sharper criteria beat exhaustive lists.

1. **Happy path.** The main success scenario. Always first.
2. **Invalid input.** Empty, malformed, too long, wrong type.
3. **Boundaries.** Zero, one, max, max+1, first and last page.
4. **Permissions.** Signed out, wrong role, someone else's data.
5. **State.** Already done, expired, deleted, concurrent edit.
6. **Failure of dependencies.** Email provider down, timeout, partial failure.
   What does the user see, and is anything lost?
7. **Non-functional.** Latency, accessibility, audit logging, localisation.
   Only with a measurable threshold.

If this produces more than ~8 criteria, the story is probably too big. Offer
the **story-split** skill and list which criteria would form each child.

## Write and verify

Edit the `## Acceptance Criteria` section of `backlog/<id>.md` directly, then
log it with `story note <id> "<what changed>"`. If the story was `ready` and
the criteria changed its scope, follow the **story-edit** skill: it goes
back to `refined` for a re-estimate. Run
`story lint <id>` and fix every `ac.*` finding before reporting.

Report the criteria and, for each one you added, a single line on why it
matters.
