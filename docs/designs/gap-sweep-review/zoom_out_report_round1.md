# Stage 5 Zoom Out Report: backlog-discover

*   **Target:** `docs/designs/backlog-discover.md` (lines 1..485)
*   **Reviewers:** `egm-zoom-out`, `egm-inventive-catalyst` (fresh context, read-only)
*   **GoldfishContext:** `skills/backlog-groom/SKILL.md`, `skills/story-time/references/backends.md`,
    `skills/story-time/scripts/src/ports/tracker-gateway.mjs`, `test/skills.test.mjs`,
    `test/expected-skills.mjs`, `README.md`
*   **Date:** 2026-11-13

## E0. Elephant finding: the design targets a stale checkout (Blocker)

The Elephant found this while verifying reviewer claims. Neither reviewer could see it, because both read the same stale worktree.

*   **Evidence.** The shared anchor and this run-owned worktree are on branch
    `claude/story-time-skills-uubus7` at `1b2a3a5`. The repository default is
    `origin/main` at `af8dc9b` (merged 2026-11-13, PR #65). `origin/main`
    contains `backlog/` (ST-001 to ST-028), `.story-time/settings.json`
    (`tracker: github`, `sync: auto`, prefix `ST`), `gitea-gateway/`, `native/`
    (the Rust CLI), `docs/adr/`, `docs/spikes/`, and the skills `refinement` and
    `sprint-planning`. The stale base has none of these.
*   **Effect on the design.**
    *   `refinement` replaced `backlog-groom` as the groom entry point (ST-018).
        The design is framed against `backlog-groom`.
    *   `docs/spikes/ST-019-atlassian-lifecycle.md:100` plans a new
        `project-discovery` owner (goals, users, epics). The names and
        responsibilities overlap with `backlog-discover`, and the design does not
        mention it.
    *   ST-028 (done) groomed all 17 open GitHub issues against the backlog by
        hand. That is the tracker collector's job, done once.
    *   The Detailed Implementation Plan names Node test files and says "no
        executable code". `origin/main` has Rust policy tests
        (`scripts/story-policy.rs`, `test/story-policy-contract.rs`) in its
        `testCommand`. The file list and verification commands must be
        re-derived from `origin/main`.
    *   "This repository has no story-time setup" is false on `origin/main`. The
        Rule 1 `chore` story can be filed there (next ID after ST-028), with
        `sync: auto`.
*   **Required action.** Rebase the design onto `origin/main` before any other
    finding is resolved. Several findings below change once that is done.

## Reviewer findings and Elephant verification

Status key: **Verified** (checked in code), **Stale-base** (true only on
`1b2a3a5`; re-check on `origin/main`), **Judgment** (design opinion, not a
code fact).

### Blocker

**Z1. The skill relies on Jev calls that the shipped CLI cannot make.** (zoom-out)
Scope: Duplicate check, Where Jev is used, NFR Cost, Non-Goals. The only Jev
entry point is `story judge`. `cite` is not a supported question type
(`skills/story-time/scripts/src/domain/judgement.mjs:123` accepts `noul`,
`choice`, `score` only). The README says only story fields leave the machine,
through an egress gate. Ad hoc `noul` and `cite` on code excerpts bypass that
gate. Proposed fix: drop Jev from this skill and let the agent compare, as the
other skills do, or design a gated `story judge` extension separately.
Status: **Verified** on `1b2a3a5`; **Stale-base** for the `origin/main` Jev
surface.
Related: inventive-catalyst C4 (Nit) raises the same egress-gate gap.

### Major

**Z2. The filing step re-does what `story-create` already does, and overrides `sync`.** (zoom-out)
`story-create` already checks duplicates with `story list --json`, calls
`acceptance-criteria`, runs `story judge`, batches approval, and follows
`sync` (`off`, `ask`, `auto`). The design calls `story push` directly, which
skips the `ask` prompt and is refused under `off`. Fix: hand each approved
row to `story-create` with evidence in Notes, then `story-estimate`. Drop the
planned test that requires `story push --dry-run` in this skill.
Status: **Verified** (`skills/story-create/SKILL.md:105-118` on `1b2a3a5`).

**Z3. Tracker list covers Gitea, which does not exist, and omits ADO.** (zoom-out)
Status: **Stale-base, reversed.** `1b2a3a5` has no Gitea code, but
`origin/main` has `gitea-gateway/` and Gitea docs. On `origin/main` the
collector should cover GitHub, Gitea, and ADO, plus Jira and Linear by the
manual adapter.

**Z4. Adopt the existing tracker issue instead of creating a second one.** (zoom-out)
CUJ 3 creates a new issue and leaves the original for manual cleanup. Setting
`remote=<n>` on the new story before pushing makes push update the existing
issue (`push-stories.mjs` chooses update when `remote` is set).
Status: **Verified** on `1b2a3a5`; re-check that `remote` is still settable on
`origin/main`.

**Z5. The cap and the verification order contradict each other.** (zoom-out)
Feature and doc checks run after the cap and can drop rows, so a sweep can
show fewer than 10 rows with no refill rule. The SKILL.md outline also orders
the checks before the cap, which contradicts the Technical Plan.
Status: **Verified** (design text, lines 225-234 against 416-420).

**C1. Duplicate checks before the cap make cost grow with the whole repository.** (inventive-catalyst)
The order key uses only collector fields, so sorting can happen first. Then
walk the sorted list and check candidates one at a time until 10 survivors
(2 features) are found. "Next batch" becomes a continuation of the same walk.
This also resolves Z5. Status: **Verified** (the order key at lines 225-234
does not use `match`).

**C2. No memory plus a deterministic cap causes starvation, not just noise.** (inventive-catalyst)
If the user declines the same top 10, every sweep shows them again, and rank
11 and lower are never seen. Options: carry the session's declined set into
"next batch" and report hidden rows, or reconsider memory using excerpt
fingerprints, per-user files, or a tracker label for tracker rows.
Status: **Judgment.** It conflicts with the owner's "no memory" choice, so it
needs an owner decision.

**C3. Candidates are never merged with each other.** (inventive-catalyst)
Each signal is one record, so "number of independent sources" is always 1, and
one piece of work found by two collectors becomes two rows that can both be
filed. Fix: a consolidation step that groups candidates sharing an issue
number or ST ID before ordering. Status: **Verified** (data shape lines
176-188 and "Every signal becomes one record").

### Nit

**Z6. `kind` and `type` duplicate one fact.** Let `story-create`'s type check
set the type. Status: **Judgment.**

**Z7. Use `story list --json` once.** It returns archived stories without
`--status` (`fs-story-repository.mjs:30-33`). Status: **Verified** on
`1b2a3a5`.

**Z8. Show feature rows only on request.** The reserved 2 rows have 0.25
confidence. Gate them behind the "suggest features" trigger. Status:
**Judgment.** It conflicts with the owner's request and the recorded
owner-over-Jev decision.

**C4. Evidence checks bypass the egress gate.** This is the same issue as Z1. Status: **Verified**.

## Summary for the author

*   E0 comes first. The design must be redone against `origin/main`. Z3 already
    reverses because of it, and Z1, Z4, and Z7 need re-checking.
*   Z2, Z5, C1, and C3 hold on any base. They simplify the skill: hand off to
    `story-create`, sort first, then walk with checks, and merge candidates.
*   C2 and Z8 conflict with earlier owner decisions (no memory, reserved
    feature rows). They need the owner, not the reviewers.
