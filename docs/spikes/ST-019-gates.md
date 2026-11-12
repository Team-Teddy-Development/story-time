# ST-019 research acceptance gates

Scope: cited Atlassian/Jira research and a story-time lifecycle skill-gap map. No proposed skill is implemented.

| Gate | State | Evidence |
|---|---|---|
| G1: Extract article text and unique source links | Passed | `rustc --edition=2021 --test tools/research/atlassian-reader.rs -o /tmp/ST019-reader-tests && /tmp/ST019-reader-tests`: 2 passed. |
| G2: Reconcile source findings, exact excerpts, and repository owners | Passed | `STORY_RESEARCH_ROOT="$PWD" rustc --edition=2021 --test tools/research/atlassian-verify.rs -o /tmp/ST019-verify && /tmp/ST019-verify --nocapture`: 3 passed; `SOURCE_COVERAGE=75 EXACT_QUOTES=75`; `EXISTING_SKILL_OWNERS=17 PROPOSED_NEW_SKILLS=8 REPLACEMENT_ENTRY_POINTS=1`. Captured source text is required; refresh instructions are in the report. |
| G3: Separate guidance, Jira mechanics, and local proposals | Passed | Source excerpts and disagreements remain in the appendix. Four Jira support pages inform the synthesis. Jev source-to-owner review: supported 0.86, verdict confidence 0.72. Reader disagreement review: supported 0.90, confidence 0.94. These judgments are review evidence, not proof by agreement. |
| G4: Map lifecycle ownership and missing capabilities | Passed | All 17 existing skills have disposition rows. Eight new skill contracts and one replacement entry point specify inputs, outputs, boundaries, and dependency-aware build order. ST-018 depends on ST-019 and links the recommendations. |
| G5: Preserve verified research and tracker evidence | Passed | PR51 exact head `bba6fd2a3d4e2fa28eb9c59d4b0146b068596bdf` passed all nine observed CI checks, including native Rust, and merged as `44e12435478d347bc8ca4a5e29a49bafb5450893`. Source `cea14a08724dcd0dbeb29d24c86feaec28510443` remains preserved. Research acceptance citation supported0.87, confidence0.76; criterion-level source checks and all Test Plan items are recorded in ST-019. Native push closed issue49 and gh readback confirmed CLOSED. Fresh reader2/verifier3, Node408, policy12 and contract11 pass. Initial completeness62/done0.28 and later pre-metadata completeness89/done0.83 remain historical below-target assessments, not relabeled passes. Final refinement delivery has separate verification. |

## Verification boundaries

The research uses extracted text, not rendered diagrams, video, or interactive content. No vendor price, product ranking, or numerical marketing claim is independently certified. The report declares its bounded index scope and preserves source inconsistencies.

Existing regressions recorded in this run: 408 Node tests, 12 Rust policy tests, and 11 Rust policy-contract tests pass. New retained executable research helpers and tests use Rust. Product CLI conversion is outside this research scope.

The original run-local ledger remains historical execution evidence. ST-020 (#50) records the separate sentinel follow-up; this research acceptance does not take credit for its implementation. The current refinement ledger is outside the checkout at `/home/ejernigan/.local/state/story-time/ST019-refinement/docs/GATES.md`, so cleanup evidence can be recorded after releasing the run-owned worktree. The committed handoff is `ST-019-refinement-verification.md`.

## Refinement handoff

PR53 integrated the separately labeled brownfield owner requirement as `72fee5af341276d87842e672c04c70fa0a759c03`. ST-018 now has a versioned planning contract, criteria-to-test plan and independent size8. ST-021 retains aggregate13 as a split parent; ST-022(size5) owns preview and ST-023(size8) owns full safe apply/recovery. All original criteria remain covered. See `ST-018-ST-021-implementation-contract.md` and ADR0005. These are implementation-ready records, not completed features.
