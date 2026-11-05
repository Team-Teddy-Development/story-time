---
name: principle-invest
description: The quality bar for user stories — INVEST, vertical slicing, and the 3 Cs — plus the anti-patterns to reject. Other story-time skills apply it. Use when judging whether a story is well formed, or when a story-time skill points here.
disable-model-invocation: true
---

# INVEST

A story is a promise to have a conversation, written down well enough that
the team can finish it in one sprint and everyone agrees when it is done.

| Letter | Means | Test |
|---|---|---|
| **I**ndependent | Can be built and shipped in any order relative to its siblings | Could we ship this alone and get value? |
| **N**egotiable | States the need, not the solution | Does it name a table, endpoint, or widget the user never asked for? |
| **V**aluable | A real persona gets a real benefit | Would the persona notice if we skipped it? |
| **E**stimable | The team knows enough to size it | Can three engineers size it within one Fibonacci step of each other? |
| **S**mall | Fits comfortably in a sprint, ideally 1–3 days | Is it under 8 points? |
| **T**estable | Done is binary and observable | Can each criterion fail? |

`story lint` checks what a machine can check (form, benefit, criteria shape,
vague words, size, dependencies, spike timebox). Independent, Negotiable, and
Valuable still take judgment. `story judge` gives you a second opinion on
Negotiable, Valuable, and Testable, Jev-based when a TypeSafe key is set.
Weigh it, and apply your own judgment where its confidence is low.

## Vertical slices

Every story cuts through every layer it needs (UI, API, data) to deliver one
thin, observable outcome. "Build the database schema" is not a story. "A
shopper can see last week's orders" is.

## The 3 Cs

- **Card.** The statement and title. Short on purpose.
- **Conversation.** Notes, questions, decisions. Record them under `## Notes`.
- **Confirmation.** Acceptance criteria. The contract for done.

## Reject on sight

- **"As a developer, I want…"** Technical tasks pretending to be stories.
  Find the user outcome they enable and write that, or file a `chore`.
- **"As a user…"** Name the persona. "User" hides who benefits.
- **"so that I can use the feature"** A circular benefit. Ask why again.
- **Solution in the statement.** "I want a dropdown…" Put the need in the
  statement. Put a design constraint in a criterion, and only if it is a real
  requirement.
- **Horizontal slices.** "Backend for X", "Frontend for X".
- **Epics wearing a story costume.** More than ~8 criteria, or a size of 13 or
  more, is several stories.
- **Unfalsifiable criteria.** "Fast", "intuitive", "works correctly".
- **Hidden AND.** "…and export and email it". Each "and" is a split
  candidate.
