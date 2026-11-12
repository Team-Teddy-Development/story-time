# Atlassian Agile lifecycle research and story-time skill gaps

Research story: ST-019, [issue 49](https://github.com/Team-Teddy-Development/story-time/issues/49). Implementation consumer: ST-018, [issue 48](https://github.com/Team-Teddy-Development/story-time/issues/48).

## Decision

Build on story-time's existing story lifecycle. Add the missing project and sprint coordination around it. Use the owner's selected operating model: fixed-length sprints, continuous product-backlog prioritization, WIP-limited pull, and delivery when accepted work is releasable.

This is a proposed story-time Scrumban policy, not a claim that all Scrumban teams use it. Sprint goals and capacity constrain active-sprint changes. Future sprints are forecasts. Refinement does not assign the entire backlog to dated sprints.

The repository has 17 skills. It handles story creation, criteria, estimation, readiness, selection, kickoff, and acceptance. It does not provide a complete project-management lifecycle. The recommended library changes are **eight new skills and one replacement entry point**, supported by revisions to existing owners. No skills or tracker features are implemented by this research.

## Scope and evidence

The supplied starting page is [Agile project management](https://www.atlassian.com/agile/project-management). Research covers every unique URL under `/agile/project-management` linked from that page: the hub plus 74 articles, 75 pages total. All 75 fetches returned HTTP 200 with matching final URLs. Four Jira support pages were fetched and read separately. Total source pages read: 79.

The boundary is the project-management section, including its topic-navigation article links. Global site navigation, product landing pages, and neighboring Scrum, marketing, design, product-management, and software-development libraries are not an unbounded crawl target. Jira pages below are explicit supplements. This does not claim every article anywhere in Atlassian's Agile library was reviewed.

Research used full extracted article text, not only search snippets. The text reader strips scripts, styles, and markup, starts at the first article H1, and retains source links. It does not interpret images, videos, or interactive charts. The [article appendix](ST-019-atlassian-articles.md) records a title, finding, exact short quotation, application, and limitation for each of the 75 pages. The [source manifest](ST-019-source-manifest.tsv) records original URL, process exit, HTTP status, and effective URL. Search results served as discovery leads; findings below use the fetched primary text.

Read date: 2026-11-12. The index HTML SHA-256 is `83587181444c9928e8f9c29635efb06ef46bbd89681382021bd2f73ba1adda21`. Local HTML and text snapshots are temporary research inputs, not committed copies of Atlassian articles. The appendix preserves the necessary short evidence excerpts. Vendor prices, AI claims, product rankings, and plan entitlements were not independently verified.

## What Atlassian's material establishes

### The lifecycle extends past story acceptance

The hub's main table names initiation, planning, execution, review, release, and retrospective. Review gathers feedback and changes priorities. Release includes communication and adoption/performance monitoring. Retrospective chooses process improvements. Its FAQ instead names a five-phase model. Use the coverage of responsibilities, not the number of headings, as the design input. [Source: hub](https://www.atlassian.com/agile/project-management).

For story-time, accepted work answers whether a story satisfies its criteria and Definition of Done. Stakeholder review asks what the increment achieved. Release asks what reached users. A retrospective asks how to improve delivery. These are different outcomes and need explicit owners.

### Refinement and sprint planning are separate

Backlog grooming is the older term for refinement. It keeps work detailed appropriately, estimated when useful, emergent, and prioritized. It includes obsolete work, splitting, value, and readiness. Refinement can be asynchronous. Sprint planning selects feasible work toward the next sprint's goal. [Sources: grooming](https://www.atlassian.com/agile/project-management/backlog-grooming), [refinement meetings](https://www.atlassian.com/agile/project-management/backlog-refinement-meeting), [sprint backlog](https://www.atlassian.com/agile/project-management/sprint-backlog).

The source warns: “Too much refinement and you’ve introduced waste to the process”. Inventory all open stories for coverage, but refine near-term work deeply. Leave distant work coarse. A story can remain unestimated when its product boundary is unclear. [Sources: grooming](https://www.atlassian.com/agile/project-management/backlog-grooming), [estimation](https://www.atlassian.com/agile/project-management/estimation).

### Epics, sprints, and releases are different dimensions

Epics group related stories into larger outcomes. They can span sprints. Initiatives can group epics. Standalone stories are valid: “Not every user story needs to be part of an epic”. Release slices and versions coordinate delivery, not necessarily sprint boundaries. [Sources: hierarchy](https://www.atlassian.com/agile/project-management/epics-stories-themes), [epics](https://www.atlassian.com/agile/project-management/epics), [roadmap](https://www.atlassian.com/agile/project-management/project-roadmap).

For story-time, do not force every small bug into an invented epic. Reuse or define an epic when a real shared outcome exists. Allow one sprint to contain stories from several epics when they support its goal. A release can include work from several sprints.

### Scrumban requires an explicit working agreement

Atlassian's Scrumban article retains fixed sprints, planning, daily coordination, retrospectives, pull, WIP limits, and continuous delivery. It also says teams cannot receive new tasks until sprint end, while praising adaptability. Its methodology comparison instead allows pull as capacity becomes available. These passages do not establish one consistent sprint-scope policy. [Sources: Scrumban](https://www.atlassian.com/agile/project-management/scrumban), [methodologies](https://www.atlassian.com/agile/project-management/project-management-methodologies), [cadence](https://www.atlassian.com/agile/project-management/sprint-cadence).

The owner's fixed-sprint choice resolves the local cadence. Proposed policy: continuously reorder unscheduled work; record every active-sprint addition/removal with its reason, goal impact, capacity effect, and displaced work. Recheck WIP before pulling. Do not silently exceed capacity or treat fixed sprints as fixed release dates.

### Metrics describe delivery; they do not establish value or productivity

Story points express relative effort, complexity, and uncertainty. Velocity uses completed work and team-local history. Neither is an individual performance quota or a cross-team comparison. Lead time, cycle time, throughput, and WIP require defined event boundaries. [Sources: estimation](https://www.atlassian.com/agile/project-management/estimation), [Fibonacci points](https://www.atlassian.com/agile/project-management/fibonacci-story-points), [velocity](https://www.atlassian.com/agile/project-management/velocity-scrum), [Kanban metrics](https://www.atlassian.com/agile/project-management/kanban-metrics), [metrics](https://www.atlassian.com/agile/project-management/metrics).

Record uncertainty when history is absent. Do not manufacture team velocity, dates, utilization, or a point-to-hour conversion. Track user outcomes separately from completion counts.

## How Jira implements the distinctions

These observations come from Jira Cloud support pages, primarily company-managed Scrum spaces. They are product mechanics, not universal Agile rules. Features and permissions differ across space types and configurations.

| Source | Observed behavior and exact source excerpt | Consequence for story-time |
|---|---|---|
| [Use your scrum backlog](https://support.atlassian.com/jira-software-cloud/docs/use-your-scrum-backlog/) | The backlog groups work into backlog and sprints; supports ranking and assignment to sprints, epics, and versions. Excerpt: “assign them to sprints, epics, or versions”. Future sprints can be planned while one is active. | Preserve independent epic, sprint, and release relationships. Provide backlog, selected-sprint, and active-flow views over the same stories. |
| [Start a sprint](https://support.atlassian.com/jira-software-cloud/docs/plan-a-sprint/) | Jira records a sprint name and dates; the goal is optional in this UI. Adding/removing active work is “considered to be a scope change”. | A required sprint goal would be a story-time working agreement, not a Jira requirement. Preserve initial scope and changes for reporting. |
| [Complete a sprint](https://support.atlassian.com/jira-software-cloud/docs/complete-a-sprint/) | Incomplete work can move to the backlog, an existing future sprint, or a new sprint. Board completion means the last column. Excerpt: “Work items in any other column will be considered incomplete”. | Sprint closure must classify unfinished work. Do not declare unfinished stories done, automatically recommit them, or replace story-accept evidence with board position. |
| [Manage epics](https://support.atlassian.com/jira-software-cloud/docs/manage-epics-in-a-scrum-project/) | The epic panel creates, edits, groups, and filters work. Completing an epic “will not affect the epic’s workflow or its Status field, and none of the epic’s work items will be affected.” | Epic completion does not prove story completion. Retain separate outcome and acceptance evidence. Do not copy automatic child-closure examples from marketing articles. |

A crucial boundary: Jira offers these product operations, but story-time does not yet expose equivalent native sprint management. A skill prompt cannot create missing persistence, adapters, permission checks, or event history by claiming they exist.

## Repository skill inventory

Reviewed `skills/*/SKILL.md` from source base `2cd39f6`, not merely the skill names. No new skills have been installed.

| Existing skill | Current ownership | Lifecycle disposition |
|---|---|---|
| `story-time` | CLI, file conventions, lifecycle router | Extend to route the whole lifecycle, including sprint/release/feedback boundaries. |
| `story-setup` | Tracker, team settings, Ready/Done rules, authentication setup | Extend with Scrumban working agreement, capability discovery, and brownfield adoption/upgrade (ST-021); preserve existing sync policy and backlog identities. |
| `principle-invest` | Story quality and vertical slicing | Reuse. Explain local sizing rules as conventions, not universal Agile thresholds. |
| `story-create` | Persona, benefit, criteria, bugs, scope, initial intake | Reuse; add links to goals, evidence, and epic selection where applicable. |
| `story-edit` | Safe clarification, scope, scoring, dependency changes | Reuse as the single-story mutation owner; preserve estimates and historical acceptance. |
| `story-split` | Vertical decomposition and atomic child creation | Reuse; record parent/child outcomes and dependency direction. |
| `acceptance-criteria` | Testable behavioral criteria | Reuse. Story criteria stay distinct from team-wide DoD. |
| `story-estimate` | Relative points, calibration, WSJF inputs | Reuse; add explicit no-quota/no-cross-team-comparison guidance and unknown-history handling. |
| `story-ready` | Automated and qualitative DoR checks | Reuse; reconcile dependency wording with actual completion prerequisites. |
| `backlog-groom` | Backlog health, stale/duplicate/unclear/oversized/unscored triage, ordering | Replace its entry point with `refinement`; retain a compatibility alias, not a second process. |
| `story-pick` | One next ready, unblocked story, checked against WIP/capacity | Reuse for WIP-limited pull inside the selected sprint; it is not sprint planning. |
| `story-map` | Journey backbone, epic decomposition, release slices | Extend to manage existing epic links, missing epic definitions, outcome links, and revisable roadmap views. |
| `spike` | Timeboxed technical or decision-linked research | Reuse for answerable unknowns; product decisions remain explicit, not invented research conclusions. |
| `story-kickoff` | Criterion-to-test plan, seams, branch, execution handoff | Reuse for engineering breakdown after selection. |
| `story-accept` | Story-level criteria and DoD evidence | Reuse; hand accepted work to stakeholder review/release without equating the three. |
| `state-machine` | Checked entity lifecycle models | Reuse for sprint, release, and workflow transitions when implemented. It does not itself implement a sprint service. |
| `decision-table` | Checked multi-input rules | Reuse for pull eligibility, sprint changes, carryover, and capability rules when implemented. |

## Recommended missing skill map

Skill names below are proposed contracts, not installed capabilities. Each has an independently useful output. Shared rules belong to existing owners rather than copied instructions.

### Establish direction and prepare work

| Proposed skill | Status and boundary | Inputs | Required output and completion check |
|---|---|---|---|
| `project-discovery` | New. Establish the problem, users, product/project goals, success measures, stakeholders, boundaries, and decision owners. It does not estimate or schedule the backlog. | User brief, current evidence, constraints, existing project context. | A concise project brief, validated versus assumed facts, open decisions, and links to initial epics or research. Every proposed epic traces to an outcome or is explicitly unresolved. |
| `refinement` | Replacement entry point for `backlog-groom`, not a competing owner. Operates on one story or all open stories and delegates edits, criteria, estimates, splits, and readiness. | Current stories, product goals, feedback, existing epic map, priority inputs. | An all-open-story disposition table, epic mapping or justified standalone/unresolved status, near-term refined work, labeled assumptions, dependencies, and a readiness handoff. Every input ID appears exactly once in the inventory. Existing estimates change only under explicit re-estimation scope. |
| `sprint-planning` | New. Select a feasible increment around a sprint goal. Separate selection from starting the sprint and from pulling one story. | Refined ordered backlog, completed prerequisites, team availability, constraints, historical evidence if available. | Sprint goal, dates when known, capacity basis and arithmetic, selected story IDs, exclusions, risks, initial scope snapshot, and a forecast beyond the next sprint. Missing capacity yields a provisional plan, not fabricated commitment. |

`story-map` remains the owner of epics and release slicing. `story-edit` remains the owner of changing an individual story. A standalone `epic-management` or `story-refinement` implementation is not needed in the first release if these boundaries cover its outcomes. The requested “refinement skill” can expose both single-story and backlog modes without duplicating those owners.

### Run the sprint and close the feedback loop

| Proposed skill | Status and boundary | Inputs | Required output and completion check |
|---|---|---|---|
| `flow-review` | New. Inspect active work, blocked/aging items, WIP, dependencies, risks, and urgent requests. Async coordination is valid; a meeting is optional. | Active sprint, story states, blocked evidence, transition history, WIP policy, proposed scope changes. | A concise flow/blocker report with owners and next actions; recorded scope swaps and capacity effects. Pull decisions call `story-pick`. It cannot silently alter goals, assign extra capacity, or bypass acceptance. |
| `sprint-review` | New. Inspect the increment and sprint goal with stakeholder feedback; coordinate honest sprint closure. Story acceptance remains in `story-accept`. | Accepted stories, demonstrable increment, goal, initial scope, changes, unfinished work, stakeholder observations. | Goal outcome, demo evidence, feedback linked to backlog work, and an explicit disposition for every unfinished story. Preserve original estimates and sprint membership history. Completing a sprint never completes its stories automatically. |
| `release-planning` | New. Coordinate an independently releasable increment and verify delivery/handoff. It invokes approved deployment capabilities rather than inventing them. | Accepted work, release goal, dependencies, external constraints, release criteria and operational access. | Release manifest, readiness facts, rollout/recovery plan, communications, actual delivery evidence or a specific handoff, and post-release outcome check. Distinguish planned, accepted, and released. A sprint can release multiple times. |
| `retrospective` | New. Improve the delivery process, not re-accept features or duplicate sprint review. | Flow evidence, defects, team feedback, previous experiments, sprint/release observations. | A small set of owned improvement actions with observed problem, proposed change, expected effect, follow-up date/cadence, and outcome measure. Revisit previous actions before starting more. No automatic policy change without measured results and applicable authority. |

Risks and dependencies do not need separate general-purpose skills in the initial suite. Refine risks with the affected stories; inspect active risks and blockers in `flow-review`; review cross-epic dependencies in `story-map`. A future specialist capability is justified only by work these owners cannot handle.

### Measure and communicate

| Proposed skill | Status and boundary | Inputs | Required output and completion check |
|---|---|---|---|
| `agile-metrics` | New. Interpret deterministic metrics with named definitions and limitations. It does not compute authoritative figures from prose or fill missing history. | Timestamped state/sprint/release events, accepted completions, scope snapshots, explicit metric definitions, outcome measures. | Team-local velocity where valid, WIP/throughput/lead/cycle time, scope changes, and available goal/outcome evidence. Every reported figure identifies its source/window/units; unknown history remains unknown. Use Rust tools for computation. |
| `project-status` | New. Create audience-specific progress and forecast communication over existing records. It does not maintain another status database. | Goals, epic/release views, sprint and flow reports, metrics, risks, evidence, stakeholder needs. | A dated status report distinguishing accepted, released, blocked, and forecast work, with risks, decisions, owners, and next actions. Every status claim links to an authoritative record. |

Recommended count: eight new owners (`project-discovery`, `sprint-planning`, `flow-review`, `sprint-review`, `release-planning`, `retrospective`, `agile-metrics`, `project-status`) plus `refinement` replacing the existing `backlog-groom` entry point. Names can change during design; responsibility boundaries should remain explicit.

## Proposed Scrumban lifecycle

```text
Project discovery -> goals and epic map -> ongoing refinement
                                            |
                                            v
                                  ready ordered backlog
                                            |
                                            v
                             sprint goal + capacity planning
                                            |
                         WIP-limited pick -> kickoff -> deliver
                              ^                         |
                              |                         v
                         flow review <- evidence <- story acceptance
                                                        |
                             stakeholder sprint review <-+
                                                        |
                          release when releasable -> outcome feedback
                                                        |
                          retrospective -> improvement work -> refinement
```

Release can occur before sprint review and does not wait for sprint end. Review, release feedback, and retrospective can run on separate cadences. This diagram shows responsibility handoffs, not a mandatory serial execution chain.

The authoritative backlog remains one set of stories. Epic, sprint, active-flow, release, and reporting views reference those IDs. Every open story receives a disposition during an all-backlog pass, but only ready, feasible work receives a near-term sprint commitment. Later sprint grouping remains provisional.

## Build order and implementation prerequisites

### First delivery slice: a workable planning loop

Extend `story-setup` and `story-time` with the agreed Scrumban vocabulary. Extend `story-map` for outcome-based epic grouping. Replace the `backlog-groom` entry point with `refinement` and preserve its alias. Add `sprint-planning`, then integrate `story-pick`, kickoff, and acceptance with selected-sprint context. This is the scope that should refine ST-018 after research acceptance.

Before a skill writes sprint data, define sprint identity, goal, dates, state, membership, initial scope, and change events. Use existing `state-machine` and `decision-table` owners to specify transitions and change rules. A provisional Markdown plan can be useful before native writes exist, but must be labeled a plan rather than synchronized sprint state.

### Second delivery slice: active flow and learning

Add `flow-review` and `sprint-review`, including scope-change records and honest carryover. Add `retrospective` so recurring problems produce measured improvement actions. This closes the sprint feedback loop without requiring a new dashboard application.

### Third delivery slice: project outcomes and release coordination

Add `project-discovery` for initiatives that lack goals, and `release-planning` for operational delivery. Discovery is logically first when a project has no direction; it need not block a planning pilot whose goals already exist. Integrate post-release feedback with refinement. Extend `story-map` for roadmap views without duplicating stories.

### Fourth delivery slice: trusted reporting

Instrument event history from the first implementation slice, then add `agile-metrics` and `project-status` once observed history exists. Do not fabricate historical data to demonstrate charts. Reporting can initially show explicit unavailable fields.

These are dependency-aware release slices, not numbered promised sprints. No story points or dates are assigned to the missing-skill programme by this research. Detailed estimates require the implementation contracts and adapter decisions; keep ST-018 unestimated until that scope is refined.

## Brownfield adoption and upgrade

**Owner requirement added after the research; local proposal, not an Atlassian claim.** ST-021 (#52) tracks projects that already have stories and a backlog. A new-project-only setup is insufficient for the planned lifecycle version.

Extend the existing `story-setup` entry point with three distinct paths: new project, adopt an existing project, and upgrade an existing story-time project. Do not create a parallel setup owner. `project-discovery` still owns evidence about goals; `story-map` still owns epic relationships; existing story editing and readiness skills keep their responsibilities.

1. Inspect the source format, installed version, project settings, story files, tracker records, and capabilities without mutation. Record missing or unrecognized data explicitly.
2. Produce a source-to-target migration plan and conflict report. Preserve IDs, remote identities, custom fields, dependencies, estimates, statuses, closed records, acceptance criteria, evidence, and configured sync policy.
3. Apply a reviewed plan with a verified backup, exact preimage checks, and an ordered migration journal. Do not duplicate issues or renumber stories. Keep credentials and signing keys outside migration evidence.
4. Verify the migrated backlog and tracker mappings. Repeat runs must produce no duplicate records or changes. Retain missing sprint goals, capacity, relationships, and event history as explicit gaps rather than fabricated data.
5. On a conflict or partial failure, retain recovery evidence and provide rollback or a precise handoff. Report which local changes are reversible and which external tracker actions require compensation; do not promise an atomic cross-tracker rollback.

The target schema/version contract and supported legacy formats are implementation prerequisites. Define their compatibility rules before coding; no migration CLI command is claimed to exist yet. Test uninitialized repositories, older configurations, tracker-only and files-only backlogs, custom prefixes/fields, local-versus-tracker conflicts, interrupted application, rollback, and repeat runs in Rust. Preserve historical and completed work without forcing it through newly introduced workflow gates.

ST-021 is a separate draft capability linked to the ST-018 planning loop. Include it in delivery planning for the lifecycle version, not as an optional post-launch cleanup. Its existence does not mean migration is implemented, estimated, or accepted.

## Data and adapter gaps that skills alone cannot solve

| Required capability | Observed repository boundary | Needed implementation decision |
|---|---|---|
| Sprint records and transitions | Current story format documents lifecycle status, epic, points, and completion dependencies; no native sprint lifecycle is documented. | Define authoritative storage, identity, goal, state, dates, membership, and close/carryover semantics. |
| Scope and flow history | Story changelog is human-readable; `created`/`updated` are not a complete active/blocked/completed event stream. | Define timestamped events and concurrency behavior before trusting cycle-time or historical sprint reports. |
| Epic outcomes and roadmap views | `story-map` creates maps and `epic` links; it does not coordinate a full pre-existing epic portfolio. | Extend that owner with outcome and existing-link reconciliation; standalone stories remain valid. |
| Tracker sprint/epic/release capabilities | Native push mirrors supported tracker issues and managed labels. Jira/Linear setup is manual/off; Jira-like sprint operations cannot be inferred from issue push. | Inspect each adapter's actual API and permissions, show supported/unsupported operations, preview writes, and verify remote effects. |
| Evidence-based metrics | Backlog stats/rank exist, but full flow and scope-event history is absent. | Build deterministic Rust calculation tools with defined boundaries and fixture/real-path tests. |

The adapter decisions are follow-up design work, not approved implementation here. Preserve current tracker settings and IDs. No Jira migration, sprint assignment, dependency rewrite, production change, or bulk issue relabeling is authorized by the research request.

## Do not create a skill for every article

A product catalogue is not a lifecycle specification. The research does not justify separate skills for Gantt charts, Fibonacci points, Scrum values, each chart, every tool, or each ceremony name. Reuse estimation, modeling, mapping, and reporting owners. Critical-chain scheduling, PMO governance, portfolio management, enterprise staffing, and full dashboard applications remain optional extensions outside the first Scrumban lifecycle.

## Source disagreements and remaining decisions

The hub's six-phase table and five-phase FAQ are different models. Its “Kanban has no backlogs (usually)” is inconsistent with richer backlog guidance elsewhere. Scrumban and sprint-backlog articles mix fixed-scope language with adaptive pull. The support documentation directly confirms Jira records active-sprint scope changes and does not auto-complete epic children. Prefer that product documentation for those specific Jira behaviors.

The appendix records other inconsistencies without adopting them as rules. The Agile value is responding to change over following a plan; some extracted articles invert that phrase. Do not reproduce the inverted version in skills. Numerical thresholds, buffers, suggested durations, and product entitlements remain context-specific source claims.

Implementation still needs a working agreement for sprint duration/calendar, goal-change authority, urgent-work capacity, WIP scope, dependency eligibility, tracker authority, and event storage. The owner has selected fixed sprints and continuous prioritization, but has not supplied those project-specific values. Preserve the distinction between decisions that can be derived from repository configuration and genuinely missing product choices.

## Verification and scope classification

This change produces research Markdown, a source-manifest TSV, backlog records, and two retained Rust research helpers under `tools/research/`. No executable product implementation is changed. The existing Node CLI, Rust push/init implementations, and installer remain outside the conversion scope. The HTML reader and source-coverage checks use Rust, with curl as the external HTTP client. Markdown/TSV are documentation/data, and the existing unlazy runner is an external verification dependency. Rust conversion gate for the approved new research-tooling chain: MET; no new non-Rust implementation is introduced. This is not a claim that the entire story-time repository is converted.

Structural verification reconciles manifest URLs, article rows, quotation matches against the fetched texts, all 17 source skill paths, and proposed skill counts. Negative controls reject missing/duplicate rows, wrong URLs, and unsuccessful fetch records. Semantic review assesses source interpretation and the non-overlapping owner map. Those checks support this research, not installation or implementation of the proposed skills.

From a checkout with the captured texts at `/tmp/story-atlassian-research`, run:

```bash
rustc --edition=2021 --test tools/research/atlassian-reader.rs -o /tmp/ST019-reader-tests
/tmp/ST019-reader-tests
STORY_RESEARCH_ROOT="$PWD" rustc --edition=2021 --test tools/research/atlassian-verify.rs -o /tmp/ST019-verify
/tmp/ST019-verify --nocapture
```

To refresh the temporary corpus, compile `tools/research/atlassian-reader.rs`, fetch the hub with curl, run the reader with the HTML path and `links`, then use `reader collect <links-file> /tmp/story-atlassian-research`. A refreshed page can differ from this research snapshot; a failed exact quotation check requires examining the changed source, not replacing the recorded quotation silently.

Research evidence: the retained source verification reports `SOURCE_COVERAGE=75 EXACT_QUOTES=75`, `EXISTING_SKILL_OWNERS=17 PROPOSED_NEW_SKILLS=8 REPLACEMENT_ENTRY_POINTS=1`, and three passing Rust verification tests. Reader tests cover script/style exclusion, Unicode, entity decoding, and duplicate links. The ledger at `ST-019-gates.md` records final gate outcomes after verification.

Independent Jev claim review of the source-to-owner synthesis returned supported probability 0.86, verdict supported with confidence 0.72. This is limited review evidence, not a probability gate or implementation acceptance. The source reader's separate disagreement review returned supported probability 0.90 and verdict confidence 0.94. Keep both results without treating agreement as proof of completeness.
