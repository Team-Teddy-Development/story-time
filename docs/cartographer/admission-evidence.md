# Story Time admission evidence

Status: **NOT DONE**. Repository setup and production prerequisite inspection are verified. Signed registry admission, authenticated portfolio visibility, and worker execution remain blocked.

## Live prerequisite inspection

On 2026-11-11, the Rust `cartographer_project_preflight` MCP tool read projected deployment metadata in context `admin@coffee-cluster-1`, namespace `cartographer`. [live-preflight.jsonl](live-preflight.jsonl) contains the sanitized actual response, not a fixture.

| Prerequisite | Observed result |
| --- | --- |
| Panel | `cartographer-callback-probe`, desired 1, ready 1 |
| Legacy authority | `cartographer-authority`, desired 1, ready 1 |
| Persistent v2 authority | `cartographer-authority-v2`, desired 0, ready 0 |
| Bridge | `cartographer-bridge`, desired 1, ready 1 |
| Panel and bridge authority URL | `http://cartographer-authority.cartographer.svc.cluster.local:8080` |
| Panel and bridge v2 authority URL | Absent |
| Panel organization | `cartographer:organization:team-teddy` |
| Panel origin | `https://cartographer.teamteddy.net` |

The `coffee-cluster_cc_status` read of application `cartographer-ui` and workload `cartographer-callback-probe` returned Synced, Healthy, revision `4556b30068223bf5cabe1f12bece877bea3dc171`, desired 1, ready 1. An earlier request guessed workload `cartographer-ui` and failed. Discovering and using the actual workload corrected the operation; the new tool tests prevent that name substitution.

A browser opened the configured origin and reached the Team Teddy Internal sign-in page. This proves the unauthenticated entry route, not project visibility. No credentials or browser tokens were accessed. The owned browser tab was closed afterward.

The sign-in page also attempted to load a Cloudflare Insights beacon, which the browser reported as `net::ERR_CONNECTION_REFUSED`. This external resource failure did not prevent the sign-in page from loading. Root cause within this observation is the failed external network connection; its deployment or network-policy cause is not established. That separate site-operation issue is recorded here for handoff, not called a product-code fix.

## Remaining admission work

| Check | Expected result | Observed result |
| --- | --- | --- |
| Source binding | GitHub `team-teddy-development/story-time`, reviewed immutable commit | Prepared identity only; backend binding not verified |
| Trust and ownership | Current authorized signers and ownership for `cartographer:project:story-time` | Not read; private configuration and keys remain outside agent access |
| Qualified admission path | Running supported authority and matching reader configuration | v2 authority scaled to zero; v2 reader URLs absent |
| Signed set verification | `VERIFIED` | Not run |
| Registry publication | Successful publication and idempotent readback | Not run |
| Authenticated owner listing | Exact organization, tenant, and project, status `active` | No authenticated session available |
| UI portfolio | Story Time under Team Teddy; existing projects unchanged | Not verified |
| Worker capability | Separately permitted bounded task and real result | Not verified |

Use the conditional operator steps in [README.md](README.md) after the applicable production authority is qualified. Do not enable v2 or alter the existing v1 registry as an unreviewed fallback. Adding an expired or fabricated v1 grant is not a substitute for an admitted persistent project.

## Repository verification

The inspected source baseline is GitHub `main` at `1cbe7d82eb319ca458b06ba8eb2d603f3a97d69d`.
The actual repository CLI reads the ST file backlog, sync off, WIP limit 1, and `npm test` command.
The Rust CLI regression failed at test-only commit `c0e2bb6f7d3fa8ade55bfa2b2d6d867a64b19247` and passed after fix commit `4f70014bf2c28b4dfd963ad4a6f2caa8ee992f75` supplied proposed size and WSJF inputs.

[error-recovery.md](error-recovery.md) names every owner-reported fingerprint, the original cause, actual recovery, and the correction-recorder API limitation. A failed attempt to submit `es_22067bfa3b92` as a typed `fixed` resolution returned `verification test is not present in the original absolute failing command`. The original command was a CLI lint invocation, not a named Rust test. The new regression proves the data repair, but this verifier cannot attach a newly introduced test to that historical lint command. Its older follow-up disposition therefore remains ledger history, not the proof of this repair.

## Completion record

After admission, record the operator, UTC timestamp, deployed authority image digest, source commit, public record IDs and digests, sanitized command outputs, and authenticated UI observation. Exclude private configuration, key bytes, bearer tokens, and sign-in redirect nonce values.
