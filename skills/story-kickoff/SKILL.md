---
name: story-kickoff
description: Start work on a picked story by turning its acceptance criteria into an ordered red-green-refactor test list, identifying the ports and adapters the change touches, naming the branch, and writing a Test Plan into the story — before any production code. Use when the user says "start ST-xxx", "kick off", "let's build this story", or right after story-pick.
---

# Story Kickoff

`story` means the CLI from the **story-time** skill.

The criteria are the spec. Kickoff turns them into a plan where every step
starts with a failing test. Do not write production code during kickoff.

## 1. Confirm the story

Run `story show <id>`. It should be `in-progress` (set by the **story-pick**
skill) or `ready`. If it is `ready`, set it: `story set <id>
status=in-progress --note "kickoff"`. If it isn't ready, stop and use the
**story-ready** skill.

## 2. Map criteria to tests

For each acceptance criterion, write one or more test names in the
codebase's test style. Use `project.testFramework` from `story config`, or
look at the existing tests. One Given/When/Then is usually one test, and a
rule table is one parameterized test.

Order them so each step is the smallest move from green to green:

1. The happy path at the outermost useful boundary (an acceptance test that
   stays red until the slice is done).
2. The domain rules as fast unit tests, simplest case first.
3. Failure and boundary paths.
4. Adapter and integration tests at the edges (repository, external API).
5. Non-functional checks last (latency budget, accessibility).

## 3. Find the seams

Read the code the story touches. Name:

- **Domain logic** that should be a pure function or entity. It's tested
  directly, with no I/O.
- **Ports** the domain needs (repository, gateway, clock, queue), and
  whether each one exists. New ports get an interface and an in-memory fake
  first, with a contract test every adapter must pass.
- **Multi-step writes** across services or stores. Plan them as a saga, with
  each step paired with its compensation and a test that injects a failure
  mid-way and asserts the system is restored.
- **What not to touch.** Keep the diff to this story's criteria.

## 4. Write the Test Plan into the story

Add a `## Test Plan` section to `backlog/<id>.md`:

```
## Test Plan
Branch: story/ST-012-reset-password-link
Seams: PasswordResetService (domain), ResetTokenRepository (new port + in-memory fake), Mailer (existing port)
- [ ] AC1 → e2e: "requesting a reset emails a single-use link" (red until done)
- [ ] AC1 → unit: "token expires after 30 minutes"
- [ ] AC1 → unit: "token is single-use"
- [ ] AC2 → unit: "expired token yields LinkExpired"
- [ ] contract: ResetTokenRepository (in-memory, postgres)
```

Then `story set <id> status=in-progress --note "test plan written"`.

## 5. Hand off

Create the branch. Write the **first** test from the plan, run it with
`project.testCommand` from `story config` (or the repo's usual command), and show
that it fails for the right reason (an assertion, not a typo or import
error). Then stop and hand over, or continue the red-green-refactor loop if
the user asked you to build it:

- **Red.** One failing test.
- **Green.** The least code that passes it.
- **Refactor.** With everything green, clean up. Keep SOLID boundaries.
- Tick the plan item. Commit. Next test.

When every plan item is ticked, use the **story-accept** skill.
