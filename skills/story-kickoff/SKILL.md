---
name: story-kickoff
description: Start work on a picked story by turning its acceptance criteria into an ordered red-green-refactor test list, identifying the ports and adapters the change touches, naming the branch, and writing a Test Plan into the story — before any production code. Use when the user says "start ST-xxx", "kick off", "let's build this story", or right after story-pick.
---

# Story Kickoff

`story` means the CLI from the **story-time** skill.

The criteria are the spec. Kickoff turns them into a plan where every step
starts with a failing test. Do not write production code during kickoff.

A local sprint-planning artifact does not start a story or a sprint. Recheck
current readiness, completed dependencies, and the pull decision from **story-pick**
before implementation. Return stale planning inputs to **sprint-planning** skill.

## 1. Confirm the story

Read the story through the configured Story Time interface. It must be ready
or an in-progress story you are authorized to resume. A status value alone
is not execution ownership.

For a local repository that uses native assignments, manual kickoff and the
coordinator use the same repository reservation. For a named story, use:

```bash
story assignment next --owner <same-or-new-run-id> --pid <agent-process-id> --story <id>
```

Use the persistent agent PID and reuse the run ID supplied by story-pick or
the coordinator when resuming its reservation. Keep tokens private. Proceed
only on `claimed` or `resumed` and a matching `claim.story_id`. On `recovered`,
inspect the recorded work and establish safe resumption before continuing.
If capacity is full, the request is denied, the owner is uncertain, or the
CLI cannot check the reservation, stop without changing the story or starting
work. Do not retry without `--story` or use a status-only update as a fallback.

After confirming ownership, mark a ready story in progress through the
configured Story Time interface. For repositories that do not use native
assignments, the manual story-pick workflow remains available, but it cannot
share capacity with this coordinator. Keep coordinator dispatch disabled in
those repositories.

## 2. Map criteria to tests

For each acceptance criterion, write one or more test names in the
codebase's test style. Use `project.testFramework` from `story config`, or
look at the existing tests. One Given/When/Then is usually one test, and a
rule table is one parameterized test. If the story has `state_machine:`,
each invariant in that model is one property test, run after every
transition.

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

Record the test-plan note through the configured Story Time interface. Do not
use a status change here to start work without the admission from step 1.

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
