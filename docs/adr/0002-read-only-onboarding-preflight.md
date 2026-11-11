# ADR 0002: Inspect admission prerequisites without granting access

Status: Accepted for repository onboarding preparation.

## Context

Story Time needs a Cartographer project under Team Teddy. Repository configuration alone cannot create a signed admission or prove that the deployed panel reads the persistent registry. The first status request guessed a workload name. The actual panel workload is `cartographer-callback-probe`, not `cartographer-ui`.

A repeatable operational check must keep credentials outside its output and distinguish a missing deployment from a failed API request. New executable implementation must use Rust.

## Decision

Add `tools/cartographer-preflight` as a read-only Rust CLI and stdio MCP. Hard-bind Coffee's existing context and the Cartographer namespace. Discover deployment metadata before reading the selected panel and bridge fields. Project only replica counts and four allowlisted literal environment values. Reject caller-selected commands and targets. Never resolve Secrets, private authority configuration, tokens, or signing keys.

Report production prerequisite failures as typed data with observed and expected values. Report transport and malformed-projection failures separately. The tool does not grant admission, authorize source access, change registry state, deploy a service, or log in as the owner.

Keep the repository binding as preparation data. Preserve the actual live evidence and retain the unmet admission criterion. Route required backend work through the Cartographer intake rather than change the authority from this feeder repository.

## Consequences

Maintainers can repeat the same bounded check without reproducing a shell sequence. Rust unit and executable protocol tests cover malformed data, missing required fields, a v2 authority scaled to zero, fixed request targets, and redaction. CI runs these tests and the actual Story Time CLI regression.

A passing preflight is not authorization or a production-health certificate. It does not verify DNS, the HTTP route, registry ownership, signed policy contents, rollout-generation freshness, or a worker's execution permit. An authenticated portfolio and a permitted real task remain separate acceptance evidence.
