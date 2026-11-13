---
name: refinement
description: Refine one story or inventory the backlog, preserve estimates, resolve epic ownership, and prepare ordered ready candidates. Use when the user says "refine ST-012", "refine the backlog", "groom", "backlog health", or wants a ready buffer before sprint planning.
---

# Refinement

Use one workflow for both modes. `backlog-groom` is an alias to this skill.
`story` means the CLI from the **story-time** skill. Read `story config --json`
first. Preserve configured paths, tracker links, sync policy, and existing estimates.

## Choose the focus

| Input | Mode | Depth |
| --- | --- | --- |
| Named story ID | Single-story | Deepen that story and inspect its dependencies and epic context. |
| Backlog or grooming request | Backlog | Inventory every open story; deepen only the near-term ordered candidates. |

Both modes use the inventory below. Every open ID gets one selection disposition
and one separate epic disposition. Done, archived, and split records remain source
evidence for dependencies; they do not reenter refinement.

## Inventory and propose

1. Run `story list --json`, `story stats --json`, `story lint --json`, and
   `story rank --include-refined --json`. Read findings even when lint exits 1.
   Use the configured backlog, not an assumed `backlog/` or `ST` prefix.
2. Review stale requests, duplicates, unclear criteria, oversized stories,
   missing estimates, and blocked dependencies. Record the source evidence and
   proposed action for each affected ID. Route ambiguous intent to a question;
   do not invent business value or deadlines.
3. Give every open story an epic disposition. Use a defined epic and cite its
   outcome evidence, justify standalone work, or record an unresolved question.
   The **story-map** skill owns epic definitions and links. Preserve source
   links until its owner makes a reviewed change.
4. Present a near-term candidate order with reasons and exclusions. Existing
   sizes stay unchanged unless re-estimation is requested. Missing velocity
   means the ready buffer is unknown, not zero. Do not infer availability from
   velocity or treat WIP as a planned-story count.

## Produce the local inventory

Install the native executable separately from the skills. From the source checkout,
run `cargo install --path native/story-push --locked`. The retained Node CLI has
no `plan` command. See [planning requests](../story-time/references/planning.md)
for the request schema, calendar, source freshness, and overwrite contract.

```bash
story plan --snapshot > request.json
story plan --from request.json
```

The first command captures current repository identity and source hashes in a
complete request. Change `plan_id` and `candidate_order` after review. In
single-story mode, put the named ID in `candidate_order`; retain the full source
manifest. Populate `epic_evidence` from the reviewed map or standalone reason.
Keep unknown goal, calendar, or capacity null. The second command emits JSON only.
No capacity means a provisional inventory with no commitments.

To save a reviewed artifact, use the same request:

```bash
story plan --from request.json --output docs/planning/refinement.json
```

This creates JSON and Markdown together. Exact reviewed hashes of both targets
are required before replacement. A stale source or conflict requires recapture
and review; never remove the hash guard to make a stale request pass.

## Route changes and hand off

Use the **story-edit** skill for individual changes, **acceptance-criteria** skill
for testable outcomes, **story-split** skill for oversized work, and **spike** skill
for a bounded unknown. Use the **story-estimate** skill for missing estimates or
explicit re-estimation. Use the **story-ready** skill to establish readiness.
The planning request cannot declare a story ready or change source status.

Apply authorized changes through those existing owners and the configured sync
policy. Recapture the inventory after any source change. Hand the ordered ready
candidates, unresolved epic questions, and local artifact to the
**sprint-planning** skill. Report disposition coverage against all open IDs,
remaining blockers, preserved estimates, and the exact artifact command.
