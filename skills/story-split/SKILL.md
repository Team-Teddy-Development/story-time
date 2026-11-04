---
name: story-split
description: Split a story that is too large, too vague, or hides several needs into small, independent, valuable vertical slices using SPIDR and the classic splitting patterns, then write the children atomically (all or nothing). Use when a story is 8+ points, has more than ~8 acceptance criteria, contains "and", the user says "too big" or "break this down", or lint flags size.small / ac.count.
---

# Story Split

`story` means the CLI from the **story-time** skill.

A good split gives children that are each valuable on their own. A bad split
gives "frontend" and "backend". Every child must pass the **principle-invest**
skill on its own.

## 1. Understand the parent

Run `story show <id>`. List its acceptance criteria. They are the coverage
you must preserve across the children.

## 2. Pick a pattern

Try these in order. Stop at the first one that gives slices that are each
worth shipping.

| Pattern | Split by | Example |
|---|---|---|
| **Workflow steps** | Each step of the user's journey | Browse → add to cart → checkout |
| **Paths** (SPIDR) | Happy path first, then alternates | Card payment, then PayPal |
| **Business rule variations** | One rule per story | Flat shipping, then by-weight shipping |
| **Data variations** | One data type or source at a time | US addresses, then international |
| **Interfaces** (SPIDR) | One channel or device at a time | Web, then mobile, then API |
| **Simple / complex** | The simplest version, then the hard cases | Exact search, then fuzzy |
| **Operations (CRUD)** | Create, read, update, delete | View invoices before editing them |
| **Defer quality** | Make it work, then make it fast, accessible, or localized | Search, then sub-200 ms search |
| **Rules relaxed** (SPIDR) | Ship without a constraint, add it next | No rate limit, then rate limit |
| **Spike** (SPIDR) | Pull the unknown out into a spike | See the **spike** skill |

The first child should be the **walking skeleton**: the thinnest end-to-end
slice that proves the path works.

## 3. Check the split

Before writing anything, show the user a table:

| Parent criterion | Covered by |
|---|---|
| Given …, when …, then … | ST-new-1 |

Then check:

- Every parent criterion is covered by at least one child, or is listed as
  deliberately dropped and confirmed by the user.
- Each child has its own persona, goal, and benefit. Repeated benefits are
  fine, made-up ones are not.
- Each child is smaller than 8 points, as far as you can tell.
- Children can ship in any order, or the order is recorded with `depends_on`.
- Two to five children. More than five usually means you need the
  **story-map** skill.

Wait for approval.

## 4. Write atomically

Write the children as JSON (shape:
[children.example.json](../story-time/assets/children.example.json)) to a
temp file, then:

```bash
story split ST-004 --from /tmp/st-004-children.json --json
```

This runs as a saga. It creates each child, then marks the parent `split`
with `children:` links. If any step fails, the steps already done are
undone in reverse, so the backlog is never left half-split. If it reports a
rollback failure, tell the user exactly which story is inconsistent.

Children start as `draft` with `parent:` set and the epic inherited. They
still need the **acceptance-criteria** and **story-estimate** skills.

## 5. Report

The parent id, now `split`. Each child id with its title and the pattern
used. What each child needs next.

## Hand-off card

Unless the tracker is `files` (see `story config`), finish with each
child as a copyable card in the tracker's format, so the user can paste it
straight into GitHub, Azure DevOps, Jira, or Linear:

```bash
story export <id>             # card: GitHub issue, ADO fields, or neutral markdown
story export <id> --command   # or: the gh / az boards command that creates it
```

Put the output in a fenced block in your reply (it gets a copy button).
For ADO, each field is its own block. With sync `off`, remind the user to close or re-link the parent item in the tracker.

Then follow `sync` from `story config`:

- `off`: stop at the card. Never push, even if `gh` / `az` are signed in.
- `ask`: ask "Push the children to the tracker now?" On yes, run `story push <parent> <child ids>`
  and report the URLs it prints.
- `auto`: run `story push <parent> <child ids>` without asking, and report the URLs.

If a push fails, the CLI has already rolled back what it created. Report its
error, keep the card, and suggest `story push <parent> <child ids> --dry-run` to check the plan.

Pushing the parent along with its children closes the parent's tracker item
as not planned, because its status is now `split`.
