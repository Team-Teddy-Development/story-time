---
name: spike
description: Create and run a time-boxed research spike that answers one specific question blocking an estimate or decision, with explicit exit criteria, then turn the findings into estimated follow-up stories. Use when the team can't estimate because of an unknown, estimates diverge wildly with low confidence, a technology or vendor choice is open, or the user says "spike" or "we need to investigate".
---

# Spike

`story` means the CLI from the **story-time** skill.

A spike buys information, not features. It is worth doing only if its
answer changes a decision or an estimate.

## 1. Frame one question

Bad: "Investigate payments." Good: "Can provider X do 3-D Secure for EU
cards inside our hosted checkout, without a PCI SAQ-D?"

The question must be:
- **Answerable** with a yes/no, or a choice between named options.
- **Decision-linked.** Name the story or decision waiting for the answer.
- **Time-boxed.** Hours to two days. Past that, it is a project.

## 2. Create

```bash
story new --type spike --title "Spike: provider X 3-D Secure in hosted checkout" \
  --question "Can provider X do 3DS for EU cards in hosted checkout without SAQ-D?" \
  --timebox 1d \
  --ac "Decision recorded: X yes/no, with evidence links" \
  --ac "Follow-up stories created and estimated, or blocker documented" \
  --json
story set ST-031 depends_on=ST-040 --note "blocked on spike ST-040"
```

Link each blocked story to the spike with `depends_on`.

## 3. Run it (if asked to do the research)

- Mark it started: `story set <id> status=in-progress`.
- Prefer the cheapest evidence: docs and changelogs, then a throwaway
  prototype in a scratch directory, then asking a human. Never put a
  prototype in the product codebase.
- Keep a running log under `## Notes`: what you tried, what you found, links.
- At the timebox, stop, even without an answer. "Unknown after 1d, because Y"
  is a valid result. Propose a second spike only if a sharper question exists.

## 4. Close

1. Write `## Findings` with the answer, the evidence, and the recommendation.
2. Tick the exit criteria that are met.
3. Create or update the follow-up stories with the **story-create** or
   **story-edit** skill, and estimate them with the **story-estimate** skill.
4. `story set <id> status=done --note "answer: <one line>"`.
5. Remove the spike from blocked stories' `depends_on` if the answer
   unblocks them, or replace it with the new stories.

## Report

The question, the answer, the evidence, and the follow-up stories with their
sizes.
