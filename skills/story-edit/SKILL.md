---
name: story-edit
description: Change an existing story safely — clarify wording, add or rework acceptance criteria, change scope, re-score, or re-link dependencies — keeping the changelog honest and preserving existing estimates unless the user explicitly requests re-estimation. Use when the user says "edit/update/change/rewrite ST-xxx", "add a criterion to…", or "this story is wrong".
---

# Story Edit

`story` means the CLI from the **story-time** skill.

## 1. Load and classify

Run `story show <id>`. Decide what kind of change this is, because the kind
decides what happens next:

| Kind | Examples | Consequence |
|---|---|---|
| **Clarification** | Fix wording, add a note, link a design | No status change |
| **Scope change** | Add, remove, or rework a criterion; change the persona or goal | Keep the estimate unchanged. A `ready` story drops back to `refined` |
| **Re-score** | New value, deadline, or size | Fields only. Note the reason |
| **Re-link** | Add or remove `depends_on` | Re-run lint for independence |

## 2. Guard rails

- **Done stories are history.** Never edit the criteria of a `done` story.
  Create a follow-up with the **story-create** skill and link it in Notes.
- **In-progress stories belong to someone.** A scope change on an
  `in-progress` story needs the user to confirm the implementer knows.
  Prefer adding a new story over growing the current one.
- **Estimates are planning records.** Never change `size` or run the
  **story-estimate** skill unless the user explicitly asks. Do not describe
  re-estimation as required after a scope change.
- **Watch for creep.** If the edit adds a new "and", pushes criteria above ~8,
  or the size would reach 13 or more, stop and offer the **story-split**
  skill instead.
- **Keep the statement honest.** If the goal changed, the statement changes
  too. Re-check it against the **principle-invest** skill.

## 3. Apply

- **Frontmatter fields** always go through the CLI so they are validated and
  logged:

  ```bash
  story set ST-012 value=13 time_criticality=8 --note "contract deadline moved to Dec 1"
  story set ST-012 status=refined --note "added lockout criterion; needs readiness review"
  ```

- **Sections** (Story, Acceptance Criteria, Notes, Out of Scope): edit the
  file directly, then record why:

  ```bash
  story note ST-012 "added criterion: lockout after 5 failed resets"
  ```

Every edit leaves exactly one changelog line that says what changed and why.

## 4. Re-verify

Run `story lint <id>`. Report the diff in one or two lines: what changed,
what status it is in now, and what it needs next. Preserve the current estimate.
At handoff, offer re-estimation as an explicit choice instead of recommending it
or treating it as a readiness requirement.

## Hand-off card

Unless the tracker is `files` (see `story config`), finish with the
edited story as a copyable card in the tracker's format, so the user can paste it
straight into GitHub, Azure DevOps, Jira, or Linear:

```bash
story export <id>             # card: GitHub issue, ADO fields, or neutral markdown
story export <id> --command   # or: the gh / az boards command that creates it
```

Put the output in a fenced block in your reply (it gets a copy button).
For ADO, each field is its own block. Say which fields changed, so they can update the existing item
rather than create a new one.

Then follow `sync` from `story config`:

- `off`: stop at the card. Never push, even if `gh` / `az` are signed in.
- `ask`: ask "Push this change to the tracker now, keeping the current estimate?
  If you want re-estimation first, say so." On yes, run `story push <id>` and
  report the URLs it prints.
- `auto`: run `story push <id>` without asking, preserve the current estimate,
  and report the URLs.

If a push fails, the CLI has already rolled back what it created. Report its
error, keep the card, and suggest `story push <id> --dry-run` to check the plan.

A story that already has `remote:` is updated in place, never duplicated.

