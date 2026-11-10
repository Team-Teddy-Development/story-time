---
name: state-machine
description: Define an entity's lifecycle as a checked state machine (states, initial and terminal states, events, guards, transitions, invariants) in model/state/<entity>.toml with a design note in docs/design/state/<entity>.md, then generate one acceptance criterion per transition and per refused transition. Use when a story creates or changes an entity's status (an enrollment, an order, a cutover stage, an employment), when criteria read "can only … when …", or when the user says "state machine", "lifecycle", or "what states can X be in".
---

# State Machine

`story` means the CLI from the **story-time** skill.

A status field is a state machine whether anyone wrote it down or not.
Write it down once per entity, let the CLI check it, and link every story
that moves the status to it. Each transition and each refused event becomes
a test.

## 1. Find or start the model

The model lives at `model/state/<entity>.toml`, with `<entity>` in
lowercase kebab-case (`enrollment`, `cutover-stage`). If it exists, read it:
this story changes or uses it. If not, create it.

Take the states from the domain: the PRD, the status values the code
already stores, the people who run the process. Use their words, not the
column names.

## 2. Write the model

```toml
entity = "enrollment"
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
```

| Field | Rule |
|---|---|
| `states` | Every status the entity can hold. |
| `initial` | The state it is created in. |
| `terminal` | States nothing leaves. Every other state needs a way out. |
| `events` | What happens to the entity, as verbs (`submit`, `approve`). Not UI actions. |
| `guards` | `name = "condition, as a sentence"`. A guarded transition fires only when the condition holds. Write the sentence so it reads after "Given an enrollment in draft and …". |
| `[[transitions]]` | One table per arrow: `from`, `to`, `event`, optional `guard`. |
| `invariants` | Sentences true in every state, or in the state they name. The **story-kickoff** skill turns them into property tests. |

The file is a small TOML subset: strings, integers, booleans, arrays,
inline tables, `[table]` and `[[array]]`. Dates, floats, dotted keys, and
multi-line strings are rejected with a line number.

## 3. Check

```bash
story machine check model/state/enrollment.toml
```

It exits 1 and lists every defect. Fix them all and re-run until it prints
`OK`.

| Code | Means | Usual fix |
|---|---|---|
| `unreachable` | No path from the initial state | Add the missing transition, or delete the state |
| `dead-end` | A non-terminal state with no way out | Add a transition out, or mark it terminal |
| `missing-event` | A transition with no event | Name what causes it |
| `unknown-state`, `unknown-event`, `unknown-guard` | A name that is not declared | Declare it, or fix the spelling |
| `terminal-exit` | A terminal state with a way out | It isn't terminal. Move it out of `terminal` |
| `ambiguous` | Two transitions from one state on one event with the same guard | Give them different guards, or merge them |
| `schema` | A required field is missing or the wrong type | Add it |

## 4. Write the design note

Write `docs/design/state/<entity>.md` for the people who will maintain the
lifecycle:

1. What the entity is, and who or what moves it.
2. The diagram, from `story machine diagram model/state/<entity>.toml`, in a
   mermaid code fence. Regenerate it after every model change. Never edit it
   by hand.
3. One line per state: what it means to the user.
4. One line per event: who or what sends it.
5. Each guard and invariant, with why it exists.
6. A link to the model file. The TOML is the source of truth. The note
   explains it and never restates it differently.

## 5. Generate the criteria

```bash
story machine criteria model/state/enrollment.toml
```

It prints `- [ ] Given …, when …, then ….` lines:

- one per transition;
- one per failing guard, when every transition for that event is guarded;
- one per refused transition: an event the state does not allow, including
  every event in a terminal state.

Paste the ones this story covers into `## Acceptance Criteria` and log it
with `story note <id> "criteria from model/state/<entity>.toml"`. If the
wording is wrong, fix the model (names, guard sentences) and regenerate.
Don't hand-edit generated criteria.

More than 8 criteria (`ac.count`) means the story is several. Split it with
the **story-split** skill by state or by event. Each child keeps the
criteria for its own transitions, and the refused-event criteria go with
the story that introduces the state.

## 6. Link the story

```bash
story set <id> state_machine=model/state/enrollment.toml --note "lifecycle model"
```

That writes `state_machine: model/state/enrollment.toml` to the story's
frontmatter. The path is relative to the repo root. `story lint` reports
`state_machine.exists` as an error while the file is missing, and
`story set <id> status=ready` refuses.

## Report

The model path and its check result, the design note path, the criteria
count by kind (transitions, guards, refused), and the stories linked.
