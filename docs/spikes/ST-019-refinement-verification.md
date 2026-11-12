# Research acceptance and lifecycle refinement verification

## Delivered scope

The owner authorized acceptance of research PR51 and refinement of ST-018 and ST-021 for implementation. This run closes the research and delivers versioned contracts, criterion-level future tests, independent estimates, dependencies, Ready records and published issues. It does not implement the planning/adoption commands or install proposed skills.

PR51 merged as `44e12435478d347bc8ca4a5e29a49bafb5450893` from reviewed head `bba6fd2a3d4e2fa28eb9c59d4b0146b068596bdf`. All nine observed checks passed. Research source `cea14a08724dcd0dbeb29d24c86feaec28510443` remains preserved. ST-019's three unchanged exit criteria and Test Plan items have direct evidence in its record. Native story push closed issue49; GitHub readback confirms CLOSED.

PR53 merged as `72fee5af341276d87842e672c04c70fa0a759c03` from refreshed head `c6f86ee42abb62ef3130ff73b2af853221f79c62`, after research integration. All nine refreshed-head checks passed. A direct tree comparison between original06be5a3 and refreshedc6f86ee exited0 with no diff. The three-file requirement delta and original typed review remain preserved. Neither merge uses an administrator bypass or changes rulesets.

## Implementation queue

| Story | State after refinement | Size and evidence |
| --- | --- | --- |
| ST-018 (#48) | Ready, depends on completed ST-019 | 8; builder/tester/operator8/8/8, medium confidence. Seven criteria and planned Rust/installation tests. Size8 warning retained. |
| ST-021 (#52) | Split, not implemented | Original aggregate13 from13/13/13 retained; six-criterion coverage table maps both children. GitHub closes the replaced parent as not_planned, not completed migration. |
| ST-022 (#54) | Ready, depends on completed ST-019 | 5; child votes5/5/8, medium confidence. Read-only preview with conflict/version/preservation tests. |
| ST-023 (#55) | Ready contract, blocked from starting by ST-022 | 8; child votes8/8/13, two medium-or-higher and operator low confidence. Safe apply and recovery remain one delivery. Size and unfinished-dependency warnings retained. |

Every implementation acceptance box remains unticked. The four scored stories use explicit PROPOSED baseline cost-of-delay scores1/1/1, not owner-confirmed value, urgency, dates, velocity or capacity. Actual rank shows ST-022 WSJF0.60, ST-018 WSJF0.375, and excludes ST-023 because ST-022 is unfinished. Existing unrelated in-progress stories and WIP1 remain unchanged; no implementation is started.

The contract is [Planning and brownfield implementation contract](ST-018-ST-021-implementation-contract.md). [ADR0005](../adr/0005-local-planning-and-additive-adoption.md) records the local-artifact/additive-metadata decision. Proposed native commands and future test targets are explicitly not installed or executed yet. The lifecycle version cannot claim brownfield readiness until ST-018 and both children pass their implementation acceptance.

## Direct verification

The configured baseline exited0. `npm test` reports408 passed, zero failed or skipped. `rustc --edition=2021 --test scripts/story-policy.rs -o /tmp/ST019-accept-policy && /tmp/ST019-accept-policy` passes12. `rustc --edition=2021 --test test/story-policy-contract.rs -o /tmp/ST019-accept-contract && /tmp/ST019-accept-contract` passes11. Reader tests pass2 and source verifier tests pass3 against the captured75-page corpus.

`story lint ST-018 ST-022 ST-023` exits0 with zero errors and three warnings: size8 on ST-018, size8 on ST-023, and ST-023's unfinished ST-022 dependency. `story set status=ready` succeeds for each without force, and each change is previewed/published through native story push. No warning is represented as a passing dependency or completed implementation.

The final run ledger is `/home/ejernigan/.local/state/story-time/ST019-refinement/docs/GATES.md`. It stays outside the worktree so final PR and cleanup readback can be recorded after release. It requires current automatic Rust source verification and active-leaf lint, plus manual acceptance/publication/integration evidence. Final exact source commit, PR, CI, cleanup and completion assessments are recorded there and in its adjacent final receipt after those effects occur.

## Limits and follow-ups

ST-024 (#56) records a concrete existing CLI defect. Lint applies size13 readiness errors even to terminal split parents. The family lint command exited1 for ST-021; that result is preserved, not relabeled a pass. Parent13 is unchanged. Active implementation leaves have zero errors. ST-024 remains draft/unestimated; no lint implementation fix is included here.

ST-025 (#57) records native error-sentinel provenance capture failure for two estimator tool-result records, `es_e6e89f2e6d6b` and `es_c6aa76ec2584`. Classification was skipped/pending, not a verified semantic false-positive verdict. This intake does not repair the separate sentinel source or change its installation.

Invocation recovery is separate from source repair. Missing `docs/design` and `model` searches were incorrect path assumptions; actual source paths were then inspected. Legacy branch-protection API returned404 because this repository uses active rulesets26702417 and27056058. Installed gh's `pr edit` hits removed GraphQL projectCards; the exact retarget succeeded via REST. Installed gh rejects issue field stateReason; REST returns state_reason and confirms split parent52 closed/not_planned. None of these recoveries changes branch protection, lowers a gate, or fixes a product source defect.

The first architecture delegate returned unrelated historical follow-up text and was not used as architectural evidence. The coordinator read the actual native parser, config resolution and Tracker interface. Fresh three-lens delegates returned estimates for the requested files; their individual votes, including disagreement, are retained. Optional story judge used its code engine because its own key was absent; those heuristic suggestions are not represented as Jev judgments or the basis of the poker result. Direct typed Jev tools remained available.

The initial research completeness62/done0.28 and later pre-metadata89/done0.83 remain historical. Source acceptance citation supported0.87/confidence0.76. Optional refinement claim review supported0.58/confidence0.25 is weak evidence, not a planning admission veto or a high-confidence certification. Final task completeness is assessed only after concrete final publication/integration evidence exists.

## Rust conversion scope

| Item | Language or format | Role and status |
| --- | --- | --- |
| Changed backlog, contract, ADR, report and trail | Markdown and TSV | Documentation/data; no new executable implementation. |
| Existing source collection/verifier | Rust | Retained research executable and tests, directly rerun. |
| Existing native story init/push | Rust | Real configuration and publication interfaces used without source changes. |
| Existing non-init/push CLI, installer, Node tests and unlazy runner | JavaScript, external compatibility tools | Unchanged dependencies outside this documentation/refinement conversion scope. |
| Future planning/adoption implementation and tests | Required Rust | Not implemented; their story conversion gates remain UNMET until real source and installation paths pass. |

The conversion gate for this delivered documentation/refinement scope is MET because no executable implementation was introduced. This is not a claim that the repository or future features are converted. No signing key, production migration, tracker sprint assignment, or global skill installation changes.

## Decision trail and review

The append-only trail is [ST-019-refinement-decisions.tsv](ST-019-refinement-decisions.tsv). A fresh read-only review by `deepseek/deepseek-v4-pro` checked the actual commits and changed records. It found no criterion loss, unsafe promise, or hidden dependency bypass. Its one material finding was that this refinement was still uncommitted and had no PR at review time. That delivery gap must close before final completion; exact commit/PR/CI/integration evidence belongs in the final run receipt. An estimate or reviewer agreement does not itself establish fulfillment.
