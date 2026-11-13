---
name: sprint-planning
description: Propose a goal-based fixed-sprint plan from ordered ready stories, an explicit point budget and reserve, and supplied calendar evidence. Use when the user says "plan a sprint", "sprint planning", or asks which ready stories fit team capacity without starting a sprint.
---

# Sprint planning

`story` means the CLI from the **story-time** skill. Run `story config --json`.
Consume the **refinement** skill's ordered candidates and epic dispositions.
If readiness or epic intent is unresolved, return that work to its existing owner.
Planning proposes local artifacts; it does not start or close a sprint.

## Confirm the inputs

1. Record the supplied sprint goal. Story titles are not a product goal.
   Keep missing goals null and name the question.
2. Record start, end, and time zone from supplied calendar evidence. Configured
   `team.sprintDays` records duration only; it cannot establish dates or holidays.
3. Obtain a point budget, reserve in the same unit, and the evidence source.
   Velocity, working days, and availability are different quantities. If capacity
   is missing, produce a provisional plan with no commitments. Zero capacity is
   valid. Never create a velocity or availability metric from estimates.
4. Review candidate order, source readiness, sizes, dependencies, and access.
   Dependencies must already be done, not merely planned earlier in this sprint.
   Missing sizes exclude selection. WIP is a per-person pull policy, not a cap
   on the total number of stories proposed for a sprint.

## Generate the proposal

Install the native executable separately from the skills. From the source checkout,
run `cargo install --path native/story-push --locked`. Planning is native Rust;
the skill installer does not install the binary. The retained Node CLI has no
`plan` command. Read [planning requests](../story-time/references/planning.md)
for the complete version-1 schema and reviewed output preimages.

```bash
story plan --snapshot > request.json
story plan --from request.json
```

The snapshot is a complete runnable request with current source hashes. Review
`plan_id`, `candidate_order`, `goal`, `calendar`, `capacity`, and `epic_evidence`.
Leave unknown values null. Use `forecasts` only for later groupings, with no
promised dates or assignments. Keep selected IDs out of later forecast groups.
Do not change the source manifest or add a readiness claim to the request.

```bash
story plan --from request.json --output docs/planning/sprint.json
```

The explicit output writes JSON and a Markdown counterpart. Replacement requires
exact reviewed hashes of both outputs. Review both before supplying their hashes.
If sources change, capture and review a new request. Planning never edits the
backlog, settings, links, or tracker, regardless of sync mode.

## Check and hand off

Compare each selected ID with source readiness and completed dependency evidence.
Check `selected_points + reserve <= budget`. For budget 8 and reserve 1,
ordered ready stories sized 3 and 3 select 6 points and leave 1 point.
An 8-point candidate is deferred. Missing inputs stay visible in
`missing_inputs`; incomplete evidence yields `assessment: provisional`.
`planned` means a local proposal, not an active sprint or a guarantee of delivery.

Report the goal, arithmetic, dependency order, deferred work, unresolved epic
questions, and provisional reasons. Publication stays local-only. The current
tracker adapters mirror issues; they do not provide sprint assignment, start,
close, or epic publication. With a remote tracker, unsupported sprint fields keep
the plan provisional with `publication.tracker_sprint_fields` as a named reason.
Record these limits rather than inventing tracker commands or synchronization.
For missing permission or access, add an `exclusions` reason for that ID in the
request. It blocks selection and records the provisional eligibility gap.

Hand a selected ready story to the **story-pick** skill. Recheck WIP and current
source state before **story-kickoff** starts implementation. Completion belongs to
the **story-accept** skill, with criterion evidence and the configured Definition
of Done. A planning artifact is not acceptance evidence or active-sprint authority.
