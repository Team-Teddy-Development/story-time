# Story Time admission evidence

Status: **NOT DONE**. No production registry publication, authenticated listing, UI visibility, or worker execution is verified by this setup.

## Operator completion record

| Check | Expected result | Observed result |
| --- | --- | --- |
| Source binding | GitHub `team-teddy-development/story-time`, reviewed immutable commit | Pending |
| Trust and ownership | Current authorized signers and ownership for `cartographer:project:story-time` | Pending |
| Signed set verification | `VERIFIED` | Pending |
| Registry publication | Successful publication, then `authority_v2_registry_admission_already_current` | Pending |
| Authenticated owner listing | Team Teddy tenant and Story Time project, status `active` | Pending |
| UI portfolio | Story Time under Team Teddy; existing projects unchanged | Pending |
| Worker capability | Separately permitted bounded task, real result, and verification | Pending |

Record the operator, UTC timestamp, deployed authority image digest, source commit, public record IDs and digests, sanitized command outputs, and authenticated UI observation here. Exclude private configuration, key bytes, and bearer tokens.

## Repository preparation

The source baseline is GitHub `main` at `1cbe7d82eb319ca458b06ba8eb2d603f3a97d69d`.
The initial preparation used local file stories, sync off, prefix `ST`, and `npm test`. After PR #14 merged, the resolution retains GitHub tracking with sync ask and the combined Node and Rust test command. Cartographer onboarding is ST-002; GitHub dogfooding remains ST-001.
The project binding is preparation data, not a registry record.

## Setup investigation correction

A relative-path search ran from the Story Time anchor instead of the Cartographer UI worktree. This caused five missing-directory errors, not product failures. The corrected search used absolute paths in the owned UI worktree and succeeded. Error-sentinel fingerprints `es_ff1169a62cbc`, `es_2d130b9f576a`, `es_82043d10eb6f`, `es_6b0d7282ccc8`, and `es_4f60ab90f04a` record the corrected invocation. Do not reuse relative paths across repository contexts.
