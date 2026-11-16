# Running a project on story-time

This playbook describes how to run a whole repository on story-time, so that every change, by a
person or an agent, starts as a story and ends with evidence. It comes from a project where it
carried a backlog of well over a hundred stories, including overnight batches run by agents with
no one watching. The skills are only one of three parts. The system works because these three fit
together:

1. **The tool**: the story-time skills and the `story` CLI, which own the backlog.
2. **Mandates**: a few rules in the repo's agent instructions (`CLAUDE.md`, `AGENTS.md`) that tell
   every agent, including subagents, how to find and use a story.
3. **Enforcement**: CI checks and configuration that make skipping the mandates fail.

Each mandate below exists because of a failure that happened without it.

## 1. The tool layer

Every skill reads and writes the same `backlog/` folder through the `story` CLI, so one skill's
output is the next one's input.

```
idea → story-create → acceptance-criteria → story-estimate → story-ready → [ready]
          ├─ too big?    → story-split
          ├─ an epic?    → story-map (with state-machine / decision-table where behaviour needs it)
          └─ an unknown? → spike
[ready] → story-pick → story-kickoff (criteria become the test list) → build → story-accept (evidence) → [done]
```

- One markdown file per story (`backlog/ST-###.md`). Frontmatter holds id, status, type, size,
  WSJF fields, `depends_on` and the tracker link. Sections hold Story, Acceptance Criteria, Out of
  Scope, Notes, Test Plan, Evidence or Findings, and Changelog.
- Frontmatter changes only through `story set`, which validates and logs them; `status=ready` is
  lint-gated. Edit section prose directly and log it with `story note`.
- `story push` mirrors stories to the tracker (GitHub Issues here), with type, size, value and
  epic labels.
- Lifecycles and multi-input rules live as checked files (`model/state/*.toml`,
  `model/rules/*.toml`) and generate acceptance criteria, so a story about a rule cites a table
  that `story table check` has proven complete.

### Configure it per repository

Run the **story-setup** skill once and commit `.story-time/settings.json`. The parts that matter
most are the ones you write yourself:

```json
{
  "tracker": "github",
  "sync": "ask",
  "prefix": "ST",
  "definitionOfReady": [
    "The design it builds on is linked (docs/design/...)"
  ],
  "definitionOfDone": [
    "No personal data in code, tests, stories, issues or logs (ids and counts only)",
    "Rule-derived behaviour cites its source, with the source's worked example as a test",
    "Destructive production steps carry the owner's explicit approval"
  ]
}
```

- Keep `sync` at `ask` in a shared repository; `auto` creates issues other people see.
- Put your non-negotiables in the Definitions of Ready and Done. They are checked at the two
  moments that count: before work starts and before it is accepted.

## 2. The mandates

Paste these into the repository's agent instructions. Replace `ST` with your prefix.

```markdown
## Before any work: find the story

Every agent, including subagents writing code, does this before its first edit:

1. **Find the story.** Use the id or issue named in the request (`story show ST-###`). If none
   was named, search: `story list`, then `grep -ril "<keywords>" backlog/`. Match on the outcome
   the work delivers, not on file names.
2. **Found it: read and obey it.** Read Story, Acceptance Criteria, Out of Scope and Notes. It
   must be `ready` or `in-progress`; if it is `draft` or `refined`, run acceptance-criteria,
   story-estimate and story-ready first. Start it with story-kickoff: the acceptance criteria
   become the test list, written before the code.
3. **Not found: make one.** Run `git fetch --prune` first, so every live branch's story ids are
   visible, then story-create, write its criteria, run story-ready, and push it.
4. **Work to the criteria only.** Anything outside them, or under Out of Scope, becomes a new
   story, not part of this change.
5. **Close the loop.** Every PR body declares its stories on a `Stories: ST-### (#issue)` line.
   Run story-accept with evidence on the PR branch before requesting merge, so the PR carries
   the `done` card. After the merge, `story-sync` closes the issue and comments on it.

## Every change starts as a story

No work begins without a story. Small items are `chore` stories with one criterion. Exception: a
production incident ships its fix first (PR labelled `incident`) and files its `bug` story within
one business day.

## Closing a spike

1. Evidence (commands, outputs, sources, how to reproduce) goes in `docs/spikes/ST-XXX-<slug>.md`.
2. The story gets a `## Findings` section: the answer, the key evidence, links. Tick the exit
   criteria that were met; state an unmet one as unmet.
3. Record the decision where it governs future work (the design doc it changes).
4. Every piece of work the answer creates becomes a follow-up story, linked from Findings.
5. Prototypes stay out of the repository.
6. Set the story done, push it, land it through one PR naming the story.
7. Post the one-line answer on the issue, linking the evidence.
```

Why each one is there:

| Mandate | The failure it prevents |
|---|---|
| Every change starts as a story | Work that was never planned, so nobody stopped to think before building |
| Find the story before the first edit | An agent builds what it guesses instead of what the criteria say, or files a duplicate |
| Kickoff turns criteria into tests first | "Done" that was never tested against the criteria |
| Work to the criteria only | Scope creep inside a PR, which reviewers cannot see |
| `Stories:` line on every PR | Code nobody can trace back to a need |
| Spike close-out | Research that ends as a chat message and is lost |
| A spike must unblock a story (`spike.unblocks` lint) | Research nobody needed |

### Feeder repositories: intake stories

When agents work in other repositories (an older system you are replacing, for example), do not
let them design across both systems. Give them a global instruction to **file** the task in the
planning repository and stop:

- type `bug` (something broken) or `story` (a missing capability), never `chore`;
- title phrased as planning: "Plan the fix for: <what is wrong, for whom>";
- a `## Source task` section (what was asked, where from, when) and a `## To plan` section of open
  questions;
- at most one criterion, the observed symptom;
- left as `draft` with an `intake` label.

The planning session grooms intake stories: real criteria, estimate, dependencies, and which
system the work lands in. This turns "go fix X" into "plan the fix", and the backlog stays the one
queue.

## 3. Enforcement

Mandates in a text file are advice. These make them hold.

| Mechanism | Catches |
|---|---|
| `story-link` CI check (`story check-pr`), **required** on the default branch | A PR with no `Stories:` line, or one naming an id or issue that is not a story; with `STORY_ACCEPTANCE=fail`, a named story that is not accepted or a closing keyword that no accepted story covers |
| `story-ids` CI check | Two branches (often parallel agents) creating the same new story id |
| `story-sync` workflow (`story sync-merge`) on each push to the default branch | Issues left open, or out of date, after their stories merge; each named issue gets one merge comment |
| `story set status=ready` lint gate | Unestimated stories, vague or non-Given/When/Then criteria, orphan spikes, missing design links |
| Definitions of Ready and Done in settings | Project rules, applied at ready and at accept |
| story-accept with evidence | A story closed without proof (commands, test names, PR links) |

### `.github/workflows/story-link.yml`

The check is the native `story check-pr` verb. Pin `<sha>` to a story-time commit.

```yaml
name: story-link
on:
  pull_request:
    types: [opened, edited, synchronize, reopened, labeled, unlabeled]
permissions:
  contents: read
  issues: read
  pull-requests: read
jobs:
  story-link:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - id: cache
        uses: actions/cache@v4
        with:
          path: ~/.story-root/bin/story
          key: story-<sha>
      - if: steps.cache.outputs.cache-hit != 'true'
        run: cargo install --locked --git https://github.com/Team-Teddy-Development/story-time --rev <sha> story-time-native --bin story --root ~/.story-root
      - name: Check declared stories
        env:
          PR_BODY: ${{ github.event.pull_request.body }}
          IS_INCIDENT: ${{ contains(github.event.pull_request.labels.*.name, 'incident') }}
          STORY_ACCEPTANCE: ${{ vars.STORY_ACCEPTANCE }}
          REPO: ${{ github.repository }}
          GH_TOKEN: ${{ github.token }}
        run: ~/.story-root/bin/story check-pr
```

The check reads the prefix and backlog folder from `.story-time/settings.json`. Its rules:

1. **Links (always fail).** Every `ST-###` has a card, and every `#N` is an issue with a story-type label (`story`, `spike`, `bug`, `chore`), not a pull request. A PR with no `Stories:` line fails unless it has the `incident` label.
2. **Acceptance.** Each reference not marked `(partial)` names a `done` card with every criterion ticked. Run story-accept on the PR branch before requesting merge. Mark a reference `(partial)` when the PR delivers only part of the story: `Stories: ST-904 (#905) (partial)`.
3. **Link consistency.** That card has a `remote`, and it matches the reference's `(#N)`. Skipped with `sync: off`.
4. **Closing keywords.** `Closes #N` (and `fix`, `resolve` in any tense, `owner/repo#N`, or an issue URL) may target only an issue that an accepted reference covers, or an issue that is not a story issue.

Rules 2–4 follow the repository variable `STORY_ACCEPTANCE`. Unset or `warn` prints `::warning::` lines and passes; `fail` fails the check. A PR cannot change a repository variable. Start in `warn`. When the check shows no warnings for 7 days, the repository owner sets `STORY_ACCEPTANCE=fail`. The `incident` label exempts rules 2–4.

Make it a required status check on the default branch (a branch ruleset or branch protection).

### `.github/workflows/story-sync.yml`

After each push to the default branch, `story sync-merge` mirrors every card the push range
changed, and every card a merged PR names, to its issue, as `story push` would write it. It writes
no card file. Each merged PR gets one `Merged in #<PR> (<sha>)` comment on each issue it names. A
card that no PR names and whose status changed (a revert or a direct push) gets one
`Status <status> from <commit>` comment. Runs are serialized, and each covers the range since the
last successful push run, so a skipped or failed run is covered by the next one. A weekly `--all`
run mirrors every linked card. The tracker repository in settings must be this repository.
Replace `main` with your default branch, and pin `<sha>` to a story-time commit.

```yaml
name: story-sync
on:
  push:
    branches: [main]
  schedule:
    - cron: '17 6 * * 1'
  workflow_dispatch:
    inputs:
      all:
        type: boolean
        default: true
concurrency:
  group: story-sync
  cancel-in-progress: false
permissions: {}
jobs:
  build:
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: dtolnay/rust-toolchain@stable
      - id: cache
        uses: actions/cache@v4
        with:
          path: ~/.story-root/bin/story
          key: story-<sha>
      - if: steps.cache.outputs.cache-hit != 'true'
        run: cargo install --locked --git https://github.com/Team-Teddy-Development/story-time --rev <sha> story-time-native --bin story --root ~/.story-root
      - uses: actions/upload-artifact@v4
        with:
          name: story-bin
          path: ~/.story-root/bin/story
  sync:
    needs: build
    # A schedule event carries no repository payload; it always runs on the default branch.
    if: github.event_name != 'workflow_dispatch' || github.ref == format('refs/heads/{0}', github.event.repository.default_branch)
    runs-on: ubuntu-latest
    permissions:
      contents: read
      issues: write
      pull-requests: read
      actions: read
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          persist-credentials: false
      - uses: actions/download-artifact@v4
        with:
          name: story-bin
          path: ${{ runner.temp }}/story-bin
      - run: chmod +x "$RUNNER_TEMP/story-bin/story"
      - name: Sync merged stories
        env:
          GH_TOKEN: ${{ github.token }}
          ALL: ${{ github.event_name == 'schedule' || inputs.all == true }}
        run: |
          if [ "$ALL" = true ]; then
            "$RUNNER_TEMP/story-bin/story" sync-merge --all --json
          else
            "$RUNNER_TEMP/story-bin/story" sync-merge --json
          fi
      - run: test -z "$(git status --porcelain)"
```

### `.github/workflows/story-ids.yml`

```yaml
name: story-ids
on:
  pull_request:
    types: [opened, synchronize, reopened]
permissions:
  contents: read
  pull-requests: read
jobs:
  story-ids:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: New story ids are unique across the base branch and older open PRs
        env:
          GH_TOKEN: ${{ github.token }}
          REPO: ${{ github.repository }}
          PR: ${{ github.event.pull_request.number }}
          BASE: ${{ github.event.pull_request.base.ref }}
          HEAD_SHA: ${{ github.event.pull_request.head.sha }}
        run: |
          git fetch -q origin "$BASE"
          mb=$(git merge-base "origin/$BASE" "$HEAD_SHA")
          added=$(git diff --name-only --diff-filter=A "$mb" "$HEAD_SHA" -- 'backlog/*.md' | xargs -r -n1 basename | sed 's/\.md$//')
          [ -z "$added" ] && { echo "No new stories."; exit 0; }
          fail=0
          for id in $added; do
            if git cat-file -e "origin/$BASE:backlog/$id.md" 2>/dev/null; then
              echo "::error::$id already exists on $BASE. Fetch, rebase and recreate the story."; fail=1
            fi
          done
          for n in $(gh pr list -R "$REPO" --state open --json number --jq '.[].number'); do
            [ "$n" -ge "$PR" ] && continue
            theirs=$(gh api "repos/$REPO/pulls/$n/files" --paginate --jq '.[] | select(.status=="added") | .filename' | grep -E '^backlog/[A-Z]+-[0-9]+\.md$' | xargs -r -n1 basename | sed 's/\.md$//')
            for id in $added; do
              if grep -qx "$id" <<<"$theirs"; then
                echo "::error::$id is also added by older open PR #$n. The older PR keeps the id."; fail=1
              fi
            done
          done
          [ "$fail" = 0 ] && echo "New ids are unique: $added"
          exit $fail
```

## 4. Agents and autonomous batches

The backlog doubles as a work queue and a proof of completion, which is what lets agents run
batches without someone watching.

- **The queue.** A story is workable when it is `ready` and every story in its `depends_on` is
  `done`. That is computed from the files, not from anyone's memory.
- **Roles.** One root session plans, grooms, reviews every diff and merges. Builder subagents each
  take one leaf (one or more stories with disjoint files) in their own worktree and open one PR.
  A separate agent gives an independent verdict before merge; a failed verdict gets one fix round
  and a re-verdict.
- **Proof, not reports.** A leaf counts as finished only when a check confirms it on the default
  branch: the story is `done`, every criterion is ticked, an Evidence or Findings section exists,
  and the issue is closed. A minimal check:

  ```js
  // node story-done.mjs ST-028 : exits 0 only if the story is done on origin/main
  import { execFileSync } from 'node:child_process';
  const id = process.argv[2];
  const sh = (c, a) => execFileSync(c, a, { encoding: 'utf8' });
  sh('git', ['fetch', '-q', '--prune', 'origin']);
  const text = sh('git', ['show', `origin/main:backlog/${id}.md`]);
  const field = (k) => (text.match(new RegExp(`^${k}: ?(.*)$`, 'm')) || [])[1]?.trim();
  const crit = text.split(/^## (?:Acceptance Criteria|Exit Criteria)$/m)[1]?.split(/^## /m)[0] ?? '';
  const open = (crit.match(/^- \[ \]/gm) || []).length;
  const ticked = (crit.match(/^- \[x\]/gim) || []).length;
  const fail = (m) => { console.error(`${id}: ${m}`); process.exit(1); };
  if (field('status') !== 'done') fail(`status is ${field('status')}`);
  if (open || !ticked) fail(`${open} unticked, ${ticked} ticked`);
  if (!/^## (Findings|Evidence|Test Plan)/m.test(text)) fail('no evidence section');
  const state = JSON.parse(sh('gh', ['issue', 'view', field('remote'), '--json', 'state'])).state;
  if (state !== 'CLOSED') fail(`issue is ${state}`);
  console.log(`${id} verified done`);
  ```

- **Honest gaps.** A criterion that cannot be met (it needs an approval, production access, or an
  owner decision) stays unticked with the reason written beside it, and the story stays
  `in-progress`. The batch reports it as a handoff, never as done.
- **Ids.** Builders do not create stories; they list follow-ups and the root files them together
  in one PR. Spikes that must file their own follow-ups run `git fetch --prune` right before each
  `story new` and push right after.
- **Judgement calls.** When a fork needs a decision, put it to a decision engine (`story judge` and
  the Jev `decide` tool here) before asking a person. Record the answer and its confidence in the
  story; below your confidence threshold, the person decides.

## 5. Starting a new project

1. Run **story-setup**; write your Definitions of Ready and Done; commit the settings.
2. Add `story-link.yml`, `story-ids.yml`, and `story-sync.yml`, and make `story-link` required.
3. Paste the mandates into the repository's agent instructions.
4. Seed the backlog: **story-map** the first epic into a walking skeleton; turn each unknown into
   a **spike** wired into the `depends_on` of the story it unblocks; use **state-machine** and
   **decision-table** for lifecycles and rules.
5. Optional: add the intake-story instruction for feeder repositories.
6. Optional: add the story-done check if agents will run batches.

## Lessons

- Run `git fetch --prune` before `story new`; the id allocator only sees branches it has fetched.
- Never merge a PR with `--delete-branch` while another open PR targets that branch; the stacked
  PR is closed. Merge, retarget the stacked PR, rebase it, check its diff, then delete.
- Never push a branch while a rebase is stopped on a conflict.
- A story closed before it was ever pushed is skipped by `story push`: set it `in-progress`, push,
  then set it `done` and push again.
- Keep the PR's `Stories:` line strict and the prose free. Only the line is checked, so a PR can
  mention related stories without claiming them.
- Put rules people care about in the Definitions of Ready and Done, not only in prose. They are
  checked at the two moments that matter.
