# ST-017 verification

## Root cause and correction

The shared `validate_base` predicate allowed HTTP only for literal loopback addresses. The required Gitea server uses an RFC1918 literal, `http://192.168.1.56:30003`. The Rust gateway rejected the configuration before planning a story push.

Test-only commit: `8a0a7596217a12c3caefa58340771241c5a5271a`.
Source-only fix: `f1e0090ba12cb953b86b559b7b691df4cb6d2743`.

The fix permits HTTP for RFC1918 IPv4 or IPv6 unique-local literals in addition to loopback. It retains HTTPS, URL credential/query/fragment rejection, and disabled redirects. Public, hostname, link-local, unspecified, multicast, and IPv4-mapped IPv6 HTTP remain rejected. HTTP is plaintext and requires a trusted LAN.

## Red and green

At the test-only commit, run:

```sh
CARGO_TARGET_DIR=/tmp/story-snt-lan-red-target cargo test --offline --manifest-path gitea-gateway/Cargo.toml --test http --test story_push private_ --no-fail-fast
```

Observed RED with the equivalent isolated test-only checkout: exit 101, two passing tests, two failing tests. `http_private_literal_boundaries_are_accepted` lists the rejected private literals. `private_lan_dry_runs_need_no_token_and_preserve_settings_and_story` reports invalid_configuration for the actual LAN endpoint.

At the source fix, the full gateway suite passes 33 integration tests: 5 CLI, 6 HTTP, and 22 story-push tests. Format checks and Clippy with `-D warnings` pass. Existing `npm test` passes 408 tests. Rust policy suites pass 12 and 11 tests. The full chained verification exits zero.

Use a separate Cargo target directory per source checkout. A first coordinator GREEN attempt reused the RED target and executed stale RED artifacts. A fresh GREEN target rebuilt the source and passed. An earlier Cargo invocation supplied two positional test filters; Cargo rejected that invocation. The corrected command uses the one filter `private_` with both named test targets. Neither invocation recovery changes the assertions or source policy.

## Actual SNT setup path

The coordinator ran the corrected compiled binary without GITEA_TOKEN against the original setup files from dotfiles commit `e2616a8017f48d87b5b29daa01ea0ba91ed24de5`:

```sh
env -u GITEA_TOKEN /tmp/story-snt-lan-green-target/debug/gitea-gateway story-push --root /tmp/snt-story-setup-20261112 --dry-run backlog/SNT-001.md
```

Observed exit 0, `status: ok`, `dry_run: true`, story SNT-001, action create, and empty warnings. Settings SHA256 before and after is `fd6a2f894ce4dc798263dbecabed29f8fb9a4513a13fad12d500c1aebb79f314`. Story SHA256 before and after is `534f17a285f9d4aedfdef57b86f61d7464d2da6ed992b2077f70316ed237971f`.

This verifies the real native local planning path. It does not establish live authentication or Gitea server compatibility.

## Rust conversion gate

MET for this bounded repair. `gitea-gateway/src/lib.rs`, `src/main.rs`, `src/story.rs`, and the integration tests are Rust. Cargo files are build configuration; JSON/Markdown fixtures and settings are required data formats. Reqwest and parsing crates are third-party Rust dependencies; Gitea REST is an external interface. Existing Node installation, planning, and other tracker paths remain unchanged and out of scope. Existing Node regression tests were run as repository-required historical checks, not as implementation or proof of the native path.

## Sentinel record

The source defect behind es_0b91f1afe186 is corrected and the original path passes. The generic `error_sentinel_resolve(kind=fixed)` verifier rejected the cross-repository evidence with `verification test is not present in the original absolute failing command`. The original failing command was a native CLI invocation, not a test-file invocation. This verifier limitation does not establish a source failure; it remains a separate tool follow-up. No tested-fix ledger acceptance is claimed without the tool returning it.
