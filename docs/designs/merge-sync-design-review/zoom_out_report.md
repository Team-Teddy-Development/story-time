# Stage 5 — Initial Zoom Out Report

*   **Target:** `docs/designs/merge-sync-design.md` (545 lines; FeatureRange 1..374, DetailedRange 375..545)
*   **Reviewers:** `egm-zoom-out`, `egm-inventive-catalyst` (independent, fresh context)
*   **Standard:** `zoom-out-standard.md`
*   **Date:** 2026-11-16
*   **Verdict:** Changes required. 1 Blocker, 5 Majors, 6 Nits after deduplication.

Both reviewers accept the problem split: acceptance is fixed before merge (ST-152),
and publishing is automated after merge (ST-151). The findings are about the
places where the design accepts a cost that it does not need.

## Blocker

### B1. The ST-152 check never reaches consumer repositories (zoom-out B1, catalyst F1)

*   **Where:** Pre-merge check; Q6; items 8, 9, 12.
*   **Issue:** Rules 1–3 go into `scripts/story-policy.rs`, which only this
    repository runs. The consumer template in `docs/playbook.md:147-190` is
    a bash `grep` pipeline (verified). The source incident (`varsity-mcp` #79
    and #80) is in a consumer repository. Item 12 can only add the new rules to
    that template in one of two ways: in shell, which Alternative E rejects, or
    by shipping a script that consumers do not have. The result is three
    `Stories:` parsers: the bash template, `references()`, and
    `stories_line.rs`. The TSV fixture covers only two of them. The argument
    for the copy in Q6 is the cost of installing the binary in `story-link`,
    but `story-sync.yml` already pays that cost in every consumer repository.
*   **Fix:** Move rules 1–3 into the CLI as a read-only verb, for example
    `story check-pr`. Make both `story-link` templates call one pinned
    binary. This deletes Q6, the TSV fixture, and the copy in
    `story-policy.rs`. If needed, `story-policy.rs` can `#[path]`-include
    `stories_line.rs`.

## Major

### M1. The CI install costs 2–4 minutes per run, and B1 doubles it (zoom-out M4, catalyst F1)

*   **Where:** NFR Cost; item 11.
*   **Fix:** Choose and document one of two options in the design. Option one
    caches the binary with `actions/cache`, keyed on `--rev`. Option two
    downloads a pinned release binary with a SHA256 check, which
    `docs/native-story-push.md` already describes, and falls back to
    `cargo install` on a miss.

### M2. Edge-triggered sync loses events (catalyst F2)

*   **Where:** Item 11 (`concurrency: story-sync`, `cancel-in-progress: false`); step 4.
*   **Issue:** A GitHub concurrency group holds at most one running run and one
    pending run. A third queued run cancels the pending one, so a burst of
    merges drops the middle diffs and comments with no report. A failed tracker
    call in step 5 also skips the comment in step 6, and nothing retries it.
*   **Fix:** Mirror state from the current default-branch HEAD, not from the
    run's own diff, so the last run to finish writes the newest state. Keep the
    comment per SHA and idempotent. Add `workflow_dispatch` or `schedule` with
    an `--all` backstop. When the push step fails, still post the comment and
    mark it "sync pending", or write a job summary.

### M3. The closing-keyword rule (Q3, 0.43) is not needed once rule 2 and ST-151 exist (zoom-out M1, catalyst F3)

*   **Where:** Q3; rule 3; CUJ 5; `closing_targets`; the ST-152 AC.
*   **Issue:** A keyword conflicts with `story push` only when the card is not
    done. Rule 2 already fails that case unless the reference is `(partial)`.
    For a `(partial)` reference, or in warn mode, `story-sync` reopens the
    issue right after the merge. The rule adds a whole-body regex, a label call
    per `#N`, and a crate function that `sync-merge` never calls.
*   **Fix (reviewers split):**
    *   Zoom-out: drop rule 3, and state in the Non-Goals that `story-sync`
        corrects a stray keyword.
    *   Catalyst: allow `Closes #N` when rule 2 passes, and narrow the rule to
        "a keyword targets a `(partial)` reference". This is option b of brief
        D4, which the design does not discuss.

### M4. The public `--no-write-back` flag is surface that only `sync-merge` needs (zoom-out M2, catalyst F4)

*   **Where:** Q1; the `--no-write-back` section; items 3, 5, 7.
*   **Issue:** No CUJ or AC needs the public flag. It gives users a way to
    update trackers that leaves no changelog record. It also leaves the
    `reconcile` action (`lib.rs:226-227, 328-335`) unspecified, because that
    action still goes through link write-back and the rollback at
    `lib.rs:395-430`.
*   **Fix (reviewers agree on dropping the flag):**
    *   Zoom-out: keep `write_back` internal to `plans()`/`execute()`.
    *   Catalyst: have `sync-merge` call `Tracker::update` directly. That call
        does no file I/O, and `github_state` runs inside it
        (`gateway.rs:511`). Report a card that has no `remote` or has
        `pending_create` as `unlinked`. Leave `story push` unchanged.

### M5. ST-153 must land before `fail` mode, but the design lists it as future work (zoom-out M3)

*   **Where:** Future Improvements, "Skill text".
*   **Issue:** `skills/story-accept/SKILL.md:3` still says to run it "when a
    story's PR merges". In `fail` mode, ST-152 fails every agent that follows
    that text. ST-153 (#348) already exists.
*   **Fix:** Name ST-153. Make it a roll-out prerequisite, and set
    `depends_on: [ST-153]` on ST-152.

## Nits

| # | Source | Where | Issue | Fix |
| --- | --- | --- | --- | --- |
| N1 | zoom-out N1 | Technical Plan diagram, steps 2 and 6, Data shapes | REST endpoints, `gh` subcommands, and Rust signatures appear in the architecture section. The tracker guard appears twice (step 1 and item 4). | Move signatures and endpoints to the plan. Keep one guard, in step 1. |
| N2 | zoom-out N2 | Data shapes; item 1 | `Reference.partial` exists only for parser parity. | B1 removes the need for it. |
| N3 | zoom-out N3 | Step 6 | The marker does not follow `mcp/comments.rs:11` (`<!-- story-time:v1:... -->`). | Use `<!-- story-time:v1:merge-sync=<sha> -->`, and check that `marker()` does not misread it. |
| N4 | zoom-out N4 | Decisions intro; Alternative E | "TypeSafe Jev", "Article VIII", and "(0.60, confidence 0.20)" are not explained. | Add one line of explanation for each. |
| N5 | zoom-out N5 | `--no-write-back` section | The reason given for taking the push lock is wrong for a fresh CI checkout. | Drop the reason, or state the real one. M4 may remove the lock entirely. |
| N6 | catalyst F5 | Step 5; Pre-merge check | An unlinked card on a merged PR only shows up as a JSON `skipped` entry. | Add "card has no `remote`; run story push before merge" to rule 2's findings. Exempt `sync: off`. |

## Traceability

| Consolidated | zoom-out | inventive-catalyst |
| --- | --- | --- |
| B1 | B1, N2 | F1 |
| M1 | M4 | F1 (install cost) |
| M2 | — | F2 |
| M3 | M1 | F3 |
| M4 | M2, N5 | F4 |
| M5 | M3 | — |
| N1 | N1 | — |
| N3 | N3 | — |
| N4 | N4 | — |
| N6 | — | F5 |

## Resolution decisions (TypeSafe Jev, at the author's request)

These are applied to the design at the first remediation step (Stage 7 resolve),
so that the Stage 6 comprehension test reads the same text as Stage 5.

| Finding | Decision | Confidence |
| --- | --- | --- |
| B1 | Read-only CLI verb `story check-pr`. Both `story-link` templates call one pinned binary. Delete the duplicate parser, the TSV fixture, and Q6. | 0.96 |
| M1 | `actions/cache` keyed on `--rev`, with `cargo install` on a miss. | 0.30 (weak; release binary 0.30, accept cost 0.17) |
| M2 | Mirror from current default-branch HEAD. Keep the per-SHA idempotent comment. Add a `workflow_dispatch` `--all` backstop and a job summary on failure. | 0.80 |
| M3 | Permit `Closes #N` when rule 2 passes. Flag a keyword only on a `(partial)` reference. | 0.88 |
| M4 | `sync-merge` calls `Tracker::update` directly. `story push` is unchanged and there is no public flag. | 1.00 |
| M5 | ST-152 `depends_on: [ST-153]`, and ST-153 is named in the roll-out. | 1.00 |
| N6 | `check-pr` flags a named card with no `remote`. Repositories with `sync: off` are exempt. | 0.98 |
