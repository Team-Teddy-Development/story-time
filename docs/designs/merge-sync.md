# Concepts Brief: Close Stories After a PR Merges

**Author:** Eric Jernigan
**Date:** 2026-11-16
**Mode:** BRAINSTORM (multi-lens)
**Codebase inspected:** `/home/ejernigan/source/team-teddy/story-time`

> **Correction (2026-11-16, after review):** The reviewers read a stale anchor checkout (branch `claude/story-time-skills-uubus7`, Node `.mjs` code). The real default branch is `origin/main`. It has `docs/playbook.md`, `.github/workflows/story-link.yml`, `.github/workflows/story-ids.yml`, a Gitea gateway, and 139 Rust files. The premise rows that say these do not exist are wrong. File and line references to `src/*.mjs` describe the old code.
>
> **Still true on `main`:** `story push` sets the issue state from the card (`gitea-gateway/src/story.rs:553`, test `terminal_unlinked_skip_and_linked_terminal_close_then_active_reopen`). A `Closes #N` merge is reopened by the next push. Objection 1 (a ticked box is not acceptance) does not depend on the codebase.
>
> **Superseding hypothesis:** The owner filed ST-151 (`backlog/ST-151.md` on branch `chore/st-151-merge-sync-workflow`, PR #345, issue #344). ST-151 never changes card status. It mirrors cards that the merge already set to done, and it comments on each named issue. This drops the close-if-ticked path and the follow-up bot PR.

## Reviewer Status

| Lens | Reviewer | Status |
| --- | --- | --- |
| Design | `egm-design` (Skeptical Architect Brainstormer) | completed |
| Codebase details | `egm-codebase-details` (Codebase Fidelity Brainstormer) | completed |
| Zoom out | `egm-zoom-out` (Simplicity and Scope Brainstormer) | completed |
| Comprehension | `egm-comprehension` (Naive Reader Brainstormer) | completed |
| Implementation readiness | `egm-implementation-readiness` (Implementation Readiness Brainstormer) | completed |
| Structural | `egm-structural` (Structure and Decomposition Brainstormer) | completed |
| Inventive catalyst | `egm-inventive-catalyst` (Inventive Catalyst Brainstormer) | completed |

Severity labels: **Blocker**, **Major**, **Nit**.

---

## 1. Overview

### The gap

After a PR merges, its stories often stay `in-progress` and their mirrored issues stay open. The cause is that agents skip the `story-accept` skill and `story push`.

### The proposal under review

A GitHub Actions workflow runs on PR merge and reads the `Stories:` line in the PR body. For each named story, it does one of two things:

- If every acceptance criterion is ticked with evidence, it opens a follow-up backlog PR on branch `story-time/<ID>` that sets `status=done`.
- If any criterion is unticked, it posts a gap comment on the PR and on the mirrored issue. It changes nothing else.

Merging the follow-up PR, then running `story push`, closes the issue.

### Consensus verdict

All seven lenses agree that the gap is real. Five of seven lenses say the post-merge follow-up PR is the wrong shape: design, zoom out, inventive catalyst, structural, and implementation readiness. Each of the five tensions in the brief comes from one timing choice: acceptance happens after merge, so recording it needs a second PR. The lenses converge on a different shape:

1. Acceptance happens in the same PR, before merge.
2. A read-only CLI check enforces this at PR time.
3. The CI wrappers stay thin.

The checks also found that several premises of the proposal are not true in this checkout (Section 2.1).

---

## 2. Analysis

### 2.1 Premises that the codebase does not support

| Premise | What the code shows | Source |
| --- | --- | --- |
| "Every PR body starts with a `Stories:` line" | No skill, doc, or code writes or parses this line. The only `Stories:` text is a stats label. `story-kickoff` names a branch only. | `skills/story-time/scripts/story.mjs:329`; `skills/story-kickoff/SKILL.md:58` |
| "Ticked with evidence" is checkable | `listItems()` and `acceptanceCriteria()` return only `{text, done}`. No evidence field exists. | `skills/story-time/scripts/src/domain/story.mjs:38-49` |
| Boxes are ticked before acceptance | `story-accept` ticks the boxes at the moment of acceptance. A merged story with all boxes ticked has usually been accepted already, or someone ticked it without the DoD check. | `skills/story-accept/SKILL.md:42` |
| `story-link.yml` / `story-ids.yml` exist in `docs/playbook.md` | `docs/` does not exist. The only workflow is `.github/workflows/test.yml`. | repository tree |
| Mirrored to GitHub **or Gitea** | Only GitHub, ADO, and memory gateways exist. No Gitea adapter exists. | `src/config/config.mjs:5,8`; `src/adapters/` |
| Hosted MCP server pattern `story-time/<ID>` | Nothing in this repository defines it. It is an outside assumption. | `references/backends.md:5` |
| Workflow can be installed in consumer repos | `scripts/install.mjs` copies only `skills/*`. Nothing writes to a consumer's `.github/`. | `scripts/install.mjs` |

### 2.2 Tension 1: only `story-accept` establishes done (Blocker)

The lenses found a consensus Blocker. A rule that turns "all ticked" into `done` creates a second path to `done` that is weaker than `story-accept`:

- `story-accept` requires evidence for each criterion: "Unproven is unmet" (`SKILL.md:22`).
- It also requires Definition of Done checks:
  - the Test Plan is ticked;
  - CI is green on the final commit;
  - no review threads are unresolved;
  - docs and the changelog are updated;
  - rollout notes are in Notes;
  - each `definitionOfDone` line from `story config` is met (`SKILL.md:25-38`).
- The code does not check `definitionOfDone` (`config.mjs:43`). `updateStory` sets `done` with no gate (`update-story.mjs:24`). Only `ready` is lint-gated.

Two outcomes are possible. Usually the merged file has unticked boxes, so the bot only posts gap comments. In the other case, boxes were ticked early, and the bot promotes stories that `story-accept` would reject. Thus close_if_ticked (0.92) automates the shortcut, not the check. The design lens also notes that a human must still merge the follow-up PR. That makes the bot a reminder with extra machinery.

### 2.3 Tension 2: `Closes #N` (Blocker, live conflict)

`Closes #N` is not only a duplicate mechanism. It works against `story push`. `GitHubIssuesGateway.syncState` reopens a closed issue when the story is not `done`, `split`, or `archived` (`src/adapters/github-issues-gateway.mjs:61-69`; covered at `test/gateways.test.mjs:57`). The conflict happens in three steps:

1. A PR with `Closes #N` merges.
2. GitHub closes the issue while the story is still `in-progress`.
3. The next `story push` reopens the issue.

The brief must name one source of truth (the backlog file) and one closer.

### 2.4 Tension 3: missing CI templates (Major, prerequisite)

The workflow depends on a reliable `Stories:` line, and nothing checks that line today. The structural and design lenses rank the line grammar and a `story-link` check as slice 0, not as a side note. The grammar must cover:

- position (the first line);
- separator;
- an optional `(#N)`, and whether it names the issue or the PR;
- unknown IDs;
- IDs without `remote:`;
- malformed lines.

### 2.5 Tension 4: bot PRs and loops (Major)

- A bot PR needs its own `Stories:` line and a guard to skip it: a head of `story-time/*`, a bot author, or a label.
- `pushStories` writes back to the story file after each remote update: the changelog plus `remote` and `remote_url` (`src/application/push-stories.mjs:46-55`). A CI `story push` after merge therefore leaves a dirty tree. That needs a third commit or PR, and the loop risk returns.
- PRs opened with `GITHUB_TOKEN` do not start other workflows. This blocks the loop, but `test.yml` and any required check also do not run, so branch protection blocks the bot PR. A PAT or App token brings the checks back, and also brings the loop back.

### 2.6 Tension 5: tokens and permissions (Major)

- The job needs `contents: write`, `pull-requests: write`, and `issues: write`.
- The repository setting "Allow GitHub Actions to create PRs" is off by default in many organizations.
- A backlog in a different repository from the code needs a PAT or App secret.
- `tokenEnv` already lets `GH_TOKEN` in the environment take priority (`src/config/secrets.mjs:36-42`).
- `story push` refuses to run when `sync` is `off` (the default) (`config.mjs:30,56-57`). `ask` cannot prompt in CI.
- Security: trigger on `pull_request: closed` with `merged == true`. Read only the base ref. Pass the PR body through `env`, never through `${{ }}` in `run:`. Never check out a fork head.

### 2.7 Other gaps the lenses raised

| Gap | Severity | Lenses |
| --- | --- | --- |
| A story delivered across several PRs gets a gap comment on each intermediate merge. A `(partial)` marker is necessary. | Major | comprehension, zoom out, inventive |
| The "merged story file" ref is not defined. Read at `merge_commit_sha` (covers squash, rebase, and merge queues). Define the behavior when the file is missing, done, split, or archived. | Major | readiness, comprehension |
| Idempotency is not defined: an existing `story-time/<ID>` branch or PR, a re-run, or two PRs that race on one story. Gap comments must use a hidden marker and be edited in place. | Major | readiness, structural, design |
| Gap comments arrive after the decision point, and posting them on both the PR and the issue doubles the noise and the scopes. | Major | zoom out, design |
| The `TrackerGateway` port has only create, update, and remove (`src/ports/tracker-gateway.mjs:4-10`). Comments need a new method. | Nit | codebase details |
| Distribution and versioning: a pinned `npx` tag, a vendored copy, or a reusable `workflow_call`. Without a pin, a parser change can start mass promotion. | Major | readiness |
| The `story-accept` step 4 (report unblocked dependents) is not covered. `story rank` already derives unblocked work from `depends_on`. | Nit | comprehension, zoom out, inventive |

---

## 3. Goals vs. Non-Goals

### Goals

- A merged PR never leaves a named story silently `in-progress` with no signal to anyone.
- `done` is set only through the `story-accept` judgment. Automation enforces or reminds. It never accepts.
- The backlog file is the single source of truth. The tracker follows it through one closer.
- All decision logic is in tracker-neutral, unit-testable CLI code. CI files are thin wrappers.
- The PR-time check reports gaps where the author can still act on them.

### Non-Goals

- The workflow never sets `done` without a human or agent acceptance act.
- No LLM or Jev judgment in CI for v1. DoD judgment stays in the local `story-accept` skill.
- Notifications to dependent stories (pull through `story rank` is sufficient).
- Multi-repo backlogs (declare a single repository for v1).
- Direct pushes to the default branch with no PR.

---

## 4. Alternatives Weighed

| # | Alternative | Write token | Loop risk | Respects `story-accept` | Tracker-neutral | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| A | **Proposal as written:** post-merge workflow, auto-done follow-up PR when ticked, gap comments | yes (3 scopes) | yes | no | no | Reject in its current form |
| B | **Proposal PR only:** same as A, but the bot PR is labeled "candidate for acceptance" and the human merge is the acceptance act | yes | yes | partly | no | Fallback only |
| C | **Pre-merge acceptance plus PR check:** the agent runs `story-accept` on the feature branch, and the same PR sets `status=done`. A read-only `story lint --pr-body` check fails unless each named story is `done` with all criteria ticked, or is marked `(partial)`. | no | no | yes | yes | **Recommended** |
| D | **Post-merge reconcile:** a default-branch job runs `story push` for changed stories (needs a push mode that does not write back) | tracker token | low if read-only | yes | yes | Add after C |
| E | **Read-only sweep:** `story merged <pr\|sha>` or a schedule lists merged-but-open stories and posts one sticky gap comment. It never edits the backlog. | issues only | no | yes | yes | Optional safety net |
| F | **Skill text only:** add "run story-accept and write the `Stories:` line before merge" to `story-kickoff`, `story-accept`, and the `story-setup` agent template | no | no | yes | yes | Do first. It costs almost nothing. |
| G | **Rely on `Closes #N`** for closing, with no `story push` | no | no | n/a | yes | Conflicts with `syncState` reopen (Section 2.3). Use it only if the check also requires `done`. |

Option C resolves tensions 1, 3, 4, and 5 together. The weak decision (GitHub-only at 0.60 vs. a CLI command at 0.40) becomes unnecessary: the lenses unanimously reverse it in favor of the CLI.

Option C has three trade-offs:

- `done` lands at merge, not at deploy. A team that defines done as "in production" needs a `released` state.
- A reverted PR leaves a `done` story, so revert handling is necessary.
- Agents learn a new pre-merge step.

---

## 5. Key Decisions & Staging for Next Steps

### 5.1 Decisions to record

| # | Decision | Recommended answer | Replaces |
| --- | --- | --- | --- |
| D1 | When is acceptance done? | Before merge, in the same PR, through `story-accept`. Change its trigger text from "when a story's PR merges" to "before requesting merge". | close_if_ticked (0.92) |
| D2 | Write path | No bot PR. The author's PR carries the story edit. | follow-up PR (0.79) |
| D3 | Where the logic lives | Pure domain functions behind CLI verbs (`story lint --pr-body`, later `story merged`). The YAML is 5 to 20 lines. | GitHub-only v1 (0.60) |
| D4 | One closer | The backlog file is the truth. Either ban closing keywords and close through `story push`, or permit `Closes #N` only when the check confirms `done`. Choose one. | (new) |
| D5 | Evidence grammar | Define a machine-readable form (for example `- [x] text — evidence: <test \| PR #n \| URL>`), or state that the check verifies ticks only and that a human reviews evidence in the PR. Decide how legacy stories that have ticks but no evidence are handled. | (new) |
| D6 | Partial delivery | `Stories: ST-012 (partial)` keeps the story `in-progress` and passes the check. | (new) |
| D7 | Dependents | Out of scope. `story rank` handles them. | dependents later (0.98), kept |

### 5.2 Open question for the owner

Does the team Definition of Done require deployment? If yes, D1 needs a `released` state after `done`, and Option C changes.

### 5.3 Slices (walking skeleton first)

| Slice | Content | Depends on | Acceptance test |
| --- | --- | --- | --- |
| S0 | Update the skill text: `story-kickoff`, `story-accept`, and the `story-setup` template require the `Stories:` line and pre-merge acceptance. | none | Skill text names the step. Measure whether the gap persists. |
| S1 | Write the `Stories:` line grammar plus a pure parser in `src/domain/`, with unit tests. | none | Parser tests cover a valid line, a malformed line, a missing line, an unknown ID, and `(partial)`. |
| S2 | Make the D4 closer decision. Fix the `syncState` reopen interaction, or add a PR-template rule that bans closing keywords. | none | Gateway test: an externally closed issue on a story that is not done is reported, not reopened silently (or the keyword is rejected). |
| S3 | Build the assessor `assessAcceptance(story, prContext) -> {ready, gaps[]}` (D5 evidence grammar, Test Plan ticks), shared by the CLI and the skill. | S1 | Unit tests against `memory-story-repository`. |
| S4 | Add `story lint --pr-body <file>` (read-only, exits 1 on gaps) plus a 5-line GitHub wrapper with `permissions: read`. Ship it as the `story-link.yml` template. | S1, S3 | A PR with an unaccepted story fails the check. A `(partial)` PR passes. |
| S5 | Add a post-merge push for changed stories, with a mode that does not write back (or the changelog line pre-written in the PR). | S2, S4 | After merge, the issue closes, and the tree stays clean. |
| S6 (optional) | Add `story merged <sha>` as a read-only sweep with one sticky gap comment for each story (hidden marker), plus a new comment method on the gateway port. | S1, S3 | A re-run edits the comment in place and creates no duplicate. |

S0, S1, and S2 are independent and can start now. A Gitea or ADO wrapper later reuses the same CLI verbs.

### 5.4 Material risks that remain

- Adoption: a PR check is only as reliable as branch protection. A team that does not make it required gets reminders, not enforcement.
- Legacy backlogs: the first run of any check flags each in-progress story that a PR names. Phase in the check with a warning-only mode.
- Distribution: no mechanism installs workflows into consumer repositories yet (`scripts/install.mjs`). S4 needs a `story-setup` step or a reusable `workflow_call`.
