# Stage 9 Implementation Readiness Report: gap-sweep (revision 2)

*   **Target:** `docs/designs/gap-sweep.md`, DetailedRange 308..411
*   **Reviewers:** `egm-implementation-readiness` (completed; 1 Blocker, 4 Major, 5 Nit),
    `egm-codebase-details` (completed; 1 Blocker, 5 Major, 3 Nit), `egm-style` (completed; 1 Blocker, 3 Major, 9 Nit)
*   **Verdict:** Not ready. All three reviewers report the same Blocker. Stage 5 (R3) and Stage 7 (S7-11) flagged it first, and revision 2 still omits it.
*   **Note:** The Stage 8 slice (move issue adoption to a prerequisite story, GitHub only) changes files 1, 3, and 6. Several findings below disappear or move once that slice is applied.

## Blocker

**S9-1. Adding the skill breaks `planning_install.rs`, and local verification cannot catch it.** (all three reviewers)
`native/story-push/tests/planning_install.rs:76` asserts `"Installed 19 story-time skills"`. `scripts/install.mjs:84,121` counts directories, so adding `gap-sweep` makes it 20. CI runs the full native suite (`.github/workflows/test.yml:49`). The plan's `testCommand` does not, and it adds only `--test gap_sweep_skill`. Fix: add `planning_install.rs` (Modify, change 19 to 20). Verify with the CI commands (S9-6).

## Major

**S9-2. The test's file list contradicts file 6.** (all three)
File 6 says file 3 asserts the `remote=` rule. File 3 does not read `skills/story-create/SKILL.md`, and line 341 gives the rule to gap-sweep. Under the Stage 8 slice, this assertion moves to the adoption story's test.

**S9-3. The `story-create` rule is placed before the story has an ID.** (readiness, codebase)
Step 2 (duplicate check) runs before `story new` in step 4. Fix: in step 2, carry the issue number and do not treat the issue as a duplicate. After `story new` in step 4, add the original title and body to `## Notes` and run `story set <id> remote=<n>`, before "Then follow `sync`" (`SKILL.md:109`). Give the exact sentences. This moves to the adoption story.

**S9-4. Assertion strings and the SKILL.md text that must match them are not given.** (all three)
The Zero-Decision Rule requires literal substrings, and the skill must contain each one verbatim. "Does not call `story push`" needs a defined check, for example no fenced `bash` line that starts with `story push`. Also give the full `description` frontmatter value. Use "at most 2", matching the spec.

**S9-5. Test setup is undefined.** (readiness, codebase, style)
Say whether the test reads the source tree or an install. If it installs, give the installer arguments (`planning_install.rs:58-76` uses `--agent all --scope user` with a temp `HOME`), a temp directory name that does not collide with `ST018-install-<pid>`, which agent directories to check, and cleanup.

**S9-6. Verification does not match CI.** (codebase, style)
CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and the full `cargo test --locked` on `native/story-push` (`.github/workflows/test.yml:47-49`). List all three, and rename the "Full suite" bullet.

**S9-7. The README row and flow block are unspecified.** (readiness, style, codebase)
Give the full three-column row. Place it after the `backlog-groom` alias row, not between `refinement` and its alias. Decide the README flow block (`README.md:36-42`) and the router flow line (`skills/story-time/SKILL.md:14-25`) together, with literal text.

## Nit

*   Show the `openai.yaml` `interface:` block with its indentation and quotes (readiness).
*   File 9 is Modify, not Create. List the review report files (all three).
*   Write out the story's criteria, persona, benefit, and verification commands (readiness). Name the **story-ready** and **story-kickoff** skills. Drop the stale "next free ID after ST-028" (style).
*   Map each file to the red commit or the green commit (readiness). The registry entry is data and cannot itself "fail" (style).
*   "As in the Technical Plan" does not match the section names (style). "Found useful" needs a measure: count of rows approved (style).
*   Explain or drop "delete the Cargo target directory". The repo convention keeps it, but the owner's global cleanup rule requires removing per-task `target` dirs, so keep the step and state that reason (codebase, Elephant).
*   Drop the trailing "*Drafted with EGM*" (style).

## Checked and accurate (reviewers)

`lib.rs:456` update on `remote`. `judgement.mjs:123`. `remote` is settable (`update-story.mjs:7`). `story list` with no `--status` reads all stories (`story.mjs:122-123`). What the Node lint enforces. `short_description` is 46 characters. The installer flags. ST-028 is the highest ID. `sync: auto` and `tracker: github`. The `story-link` and `story-ids` workflows exist. Cargo auto-discovers the new test.
