---
name: story-create
description: Turn an idea, request, bug report, PRD, or meeting notes into well-formed user stories in the backlog, with persona, benefit, acceptance criteria, and scope boundaries. Use when the user says "write a story", "create a story/ticket", "turn this into stories", or describes a need that should land in the backlog.
---

# Story Create

Turn raw input into draft stories that pass `story lint` or are one estimate
away from passing. `story` means the CLI described in the **story-time** skill.

## 1. Find the user and the why

Pull these out of the input before writing anything:

- **Persona.** A specific role: "billing admin", not "user".
- **Goal.** What they want to be able to do, in their words.
- **Benefit.** Why it matters to them or the business. Ask "so that…?" until
  the answer is something a stakeholder would pay for.
- **Evidence of done.** How would they know it works?

If the input doesn't give you persona or benefit, ask. Ask at most three
short questions at once and offer your best guess for each so the user can
just confirm. Do not invent a benefit to fill the slot.

## 2. Check for duplicates

Run `story list --json` and compare titles and statements. If an existing
story covers the same need, hand off to the **story-edit** skill instead of
creating a near-duplicate. Tell the user which story it is.

## 3. Draft

- **Title.** Outcome-first, under 60 characters: "Reset password via emailed link".
- **Statement.** `As a <persona>, I want <goal>, so that <benefit>.`
  No solution words. See the **principle-invest** skill.
- **Acceptance criteria.** Write them with the **acceptance-criteria** skill:
  Given/When/Then, the happy path first, then the failure and boundary cases
  that matter.
- **Out of scope.** Name the nearby things this story does not do. This is
  the cheapest scope control there is.
- **Notes.** Context, links, open questions.

One need is one story. If the input holds several needs, write several
stories. If one need is still too big (more than ~8 criteria, or the user
says it is big), create it and then run the **story-split** skill. For a
whole feature or epic, start with the **story-map** skill instead.

### Bugs

Use `--type bug`. The statement describes the behaviour the user expects.
Add `## Steps to Reproduce`, `## Expected`, and `## Actual`. The first
acceptance criterion is the reproduction case, inverted.

### Technical work

If there is no user-facing outcome, look for the one it enables and write
that. If there really is none (dependency upgrades, CI changes), use
`--type chore` and say plainly why it is worth doing.

### Check the type

After writing, run `story judge <id>`. If its `type` disagrees with what
you chose, re-read the input and pick deliberately: for example, a "story"
that is really a broken behaviour is a `bug`. Fix the type with `story set
<id> type=...`.

### Unknowns

If the team can't estimate it because of an open question, create a spike
with the **spike** skill and link the story with `depends_on`.

## 4. Write

```bash
story new --title "Reset password via emailed link" \
  --persona "registered user" --epic EP-002 \
  --statement "As a registered user, I want to reset my password via an emailed link, so that I can regain access without contacting support." \
  --ac "Given a registered email, when I request a reset, then a single-use link valid for 30 minutes is emailed" \
  --ac "Given an expired link, when I open it, then I see 'link expired' and can request a new one" \
  --json
```

Then open the file it reports and add `## Out of Scope` and `## Notes`.
Leave scores empty unless the user gave them. Estimation happens in the
**story-estimate** skill.

If the input is an open tracker issue that no story's `remote` names, the new
story takes it over. Copy the issue's original title and body under `## Notes`,
then run `story set <id> remote=<n>`. The next push updates that issue,
replacing its title and body, instead of creating a second one. Tell the user
before that push.

When creating several stories from one source, show the list of titles and
statements first and wait for approval before writing. Rows the user already
approved in a **gap-sweep** table count as that approval.

## 5. Report

Run `story lint <ids>`. Report each new id and title, the lint verdict, and
what it still needs to become ready (usually: estimate and value score).

## Hand-off card

Unless the tracker is `files` (see `story config`), finish with the
story as a copyable card in the tracker's format, so the user can paste it
straight into GitHub, Azure DevOps, Jira, or Linear:

```bash
story export <id>             # card: GitHub issue, ADO fields, or neutral markdown
story export <id> --command   # or: the gh / az boards command that creates it
```

Put the output in a fenced block in your reply (it gets a copy button).
For ADO, each field is its own block. For several stories, one card each.

Then follow `sync` from `story config`:

- `off`: stop at the card. Never push, even if `gh` / `az` are signed in.
- `ask`: ask "Push these stories to the tracker now?" On yes, run `story push <ids>`
  and report the URLs it prints.
- `auto`: run `story push <ids>` without asking, and report the URLs.

If a push fails, the CLI has already rolled back what it created. Report its
error, keep the card, and suggest `story push <ids> --dry-run` to check the plan.
