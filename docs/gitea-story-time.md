# Gitea Story Time

The Rust `gitea-gateway story-push` command pushes explicit local story Markdown files to Gitea issues. It reads `.story-time/settings.json`, renders the story and acceptance criteria, and saves issue links locally. Repeat pushes update the saved issue number.

## Build and select the Gitea path

From this repository, build or install the binary:

```sh
CARGO_TARGET_DIR=/tmp/story-gitea-cli-target cargo build --offline --manifest-path gitea-gateway/Cargo.toml
cargo install --offline --path gitea-gateway
```

Use the build command if you do not want to install it. Replace `gitea-gateway` below with `/tmp/story-gitea-cli-target/debug/gitea-gateway`.

This Gitea path runs directly in Rust. It does not run the Node `story` CLI. Upstream also provides a separate Node Gitea adapter; this native path does not call it. Other tracker implementations remain unchanged and outside this conversion scope.

## Project settings

Create or edit `<project>/.story-time/settings.json` as JSON. Preserve unrelated project settings. Commit settings without credentials.

```json
{
  "tracker": "gitea",
  "sync": "auto",
  "prefix": "ST",
  "backlogDir": "backlog",
  "gitea": {
    "baseUrl": "https://gitea.example.com",
    "owner": "acme",
    "repo": "shop"
  }
}
```

`baseUrl` is the server root, optionally with a deployment path prefix, not an `/api/v1` URL. HTTPS is required except for literal loopback, RFC1918 IPv4 private, or IPv6 unique-local (`fc00::/7`) addresses. HTTP sends credentials and story content in plaintext; use it only on a trusted LAN. Hostnames, public IP addresses, link-local addresses, and IPv4-mapped IPv6 addresses do not qualify for HTTP. URL credentials, queries, and fragments are rejected. Owner and repository names accept ASCII letters, digits, `.`, `_`, and `-`, except the names `.` and `..`.

Setup writes `sync: "auto"` for a new project unless the owner selects another mode. Preserve an existing explicit policy.

`sync` accepts `off`, `ask`, and `auto`. Missing `sync` means `off`. Real pushes refuse `off`. Dry runs work even with `off`. For `ask`, the caller obtains authorization before running a real push; the binary itself does not prompt. An explicit authorized invocation permits that push. `auto` permits callers to push after local changes. Neither mode starts a background watcher.

Set `GITEA_TOKEN` through your existing environment or secret manager. Use a token with issue write access to the named repository. There is no Gitea token-file loader or token argument. Never put credentials in settings, argv, Markdown, or chat. A dry run does not verify credentials.

## Push explicit files

Story files use the existing [frontmatter and sections](../skills/story-time/references/story-format.md). Edit prose and frontmatter directly for Gitea. Preserve existing issue links. `id`, `title`, and `status` must be nonempty strings. Status must be one of the documented lifecycle values. `type`, if present, accepts `story`, `spike`, `bug`, or `chore`.

```sh
gitea-gateway story-push --root /path/to/project --dry-run backlog/ST-001.md backlog/ST-002.md
gitea-gateway story-push --root /path/to/project backlog/ST-001.md backlog/ST-002.md
```

`--root` defaults to the current directory. Paths are relative to that root and must name `.md` files. Absolute paths, parent traversal, symlinks outside the root, duplicate files, duplicate story IDs, and duplicate remote numbers in the same batch are rejected. There is no ID lookup or `--all` option.

The dry run emits the complete plan, including rendered title, body, state, existing links, and desired label names. It reads local files only. It does not load credentials, construct an HTTP client, send requests, or write files. The command validates settings and every story before a real push starts. This validation is not INVEST lint or acceptance of story readiness.

| Local story | Action | Remote state |
| --- | --- | --- |
| Active, no `remote` | POST a new issue | Open |
| Active, linked | PATCH the saved issue | Open, including reopening |
| `done`, `split`, or `archived`, no `remote` | Skip | No remote change |
| `done`, `split`, or `archived`, linked | PATCH the saved issue | Closed |

`remote` accepts a positive integer or numeric string. Empty or null links mean unlinked. If `remote_url` is present, it must identify the configured server, owner, repository, and issue number. A mismatched link fails before HTTP. A legacy number-only link is accepted against the configured target; verify coordinates before using it.

Bodies contain the Story section, checkbox acceptance criteria, and extra sections such as Notes, Out of Scope, and bug reproduction details. Spikes use Question, Timebox, and Exit Criteria. Test Plan and Changelog stay local. A footer records the story ID and WSJF score when all inputs exist. Titles remain the local title without an added ID prefix.

`labels` reports the desired type, size, value, and epic names. A real push lists repository labels in pages of 50, then creates missing names with color `ededed`. The token needs repository label creation permission when a desired label is missing. Issue creation sends numeric label IDs.

For updates, the command reads the complete current issue-label array and replaces the label set after the content update. Gitea's issue-label endpoint is not paginated; repository label listing is paginated. Story Time owns the exact labels `story`, `spike`, `bug`, and `chore`, plus names with the prefixes `size:`, `value:`, and `epic:`. Replacement removes stale managed labels and preserves other label IDs. Removed size, value, or epic metadata therefore removes its remote label. Repository labels themselves are never deleted. The command reuses known labels within a batch and lists them again on the next invocation.

`split` and `archived` are the not-planned terminal statuses. Both close linked issues without deleting them. Gitea receives `state: "closed"`; this path does not send GitHub's separate closing reason. `done` also closes the issue. An active story reopens its linked issue.

## Results and local links

The command writes one JSON object to stdout, writes nothing to stderr during normal operation, and exits `0` on success or `1` on failure. `status` is `ok` or `error`. Plans include `action`, `completed`, `remote`, and `remote_url`. A skipped story has `action: "skip"` and `completed: false` because it performed no mutation.

After each successful issue response, the command validates `number` and `html_url` against the configured target, then saves:

```yaml
remote: 7
remote_url: "https://gitea.example.com/acme/shop/issues/7"
```

The save preserves other metadata, comments, Markdown sections, newline style, and file permissions. Link fields are replaced with canonical values. Unsupported link encodings fail preflight rather than rewriting unrelated YAML. The command uses a temporary file in the story's directory and renames it into place. It checks for changed local content before the request and again before saving. Concurrent processes are not locked; run one push per story at a time.

## Failures and recovery

Batch processing is sequential and per-story, not an all-or-nothing saga. Completed stories retain their links if a later story fails. Error output includes the plan and completed progress. Fix the failed input or server condition, then rerun the explicit files. Already-linked files use PATCH.

A plan is `completed: true` only after all required issue and label requests succeed. Label HTTP failures return `status: "error"`, a failure code, and `completed: false`; no deferred-label warning substitutes for synchronization.

An update saves its validated issue identity before label replacement. If label replacement fails, the local link remains available for retry, while output reports incomplete work. Repository labels created before a later failure remain available for the next push.

If a remote mutation succeeds but the local save fails, output retains the returned `remote` and `remote_url` with `completed: false`. Verify the remote issue and restore those link fields locally before retrying. A concurrent edit returns `story_changed` without overwriting that edit.

An interrupted POST or transport failure can leave an issue created without a local link. Do not blindly repeat an unlinked create. Inspect Gitea for the title and story ID footer, restore its number and URL locally, then push again. Automatic ambiguous-create reconciliation, retries, and cross-process locking are not implemented.

Story-specific error codes include `invalid_arguments`, `invalid_path`, `invalid_story`, `duplicate_story`, `unsupported_link_format`, `target_mismatch`, `sync_off`, `story_changed`, and `output_io`. Shared codes include `invalid_configuration`, `input_io`, `missing_token`, `transport`, `http_status`, `invalid_response`, and `response_too_large`. Unlike the raw commands, story error output does not include a numeric HTTP status. Server response bodies and transport details are not returned.

## Verification and Rust conversion gate

```sh
CARGO_TARGET_DIR=/tmp/story-gitea-cli-target cargo test --offline --manifest-path gitea-gateway/Cargo.toml
CARGO_TARGET_DIR=/tmp/story-gitea-cli-target cargo clippy --offline --manifest-path gitea-gateway/Cargo.toml --all-targets -- -D warnings
```

Rust integration tests invoke the compiled binary against a local Rust HTTP listener. They assert POST then PATCH, stored local links, rendered criteria, terminal skips, close/reopen state, dry-run isolation, preflight rejection, HTTP failures, response-target validation, and partial-batch recovery information. Label tests assert numeric IDs, creation of missing labels, repository pagination and complete issue-label reads, team-label preservation, stale managed removal, removed optional metadata, and truthful failure progress. Raw create/update/close tests remain in place.

The active Gitea conversion gate is **MET** for this bounded story-push and raw-issue path. The entire Story Time conversion gate remains **UNMET**, outside this task.

| Component | Language or format | Role and conversion status |
| --- | --- | --- |
| `gitea-gateway/src/main.rs` | Rust | Active CLI dispatch, input parsing, credential redaction, and JSON output. Converted. |
| `gitea-gateway/src/story.rs` | Rust | Active settings parsing, Markdown/frontmatter, planning, rendering, lifecycle mapping, typed labels, managed-label reconciliation, paginated HTTP orchestration, and atomic local link saves. Converted. |
| `gitea-gateway/src/lib.rs` | Rust | Active Gitea HTTP, TLS, target validation, response bounds, and raw issue methods. Rust. |
| `gitea-gateway/tests/*.rs` | Rust | Active verification through the binary and local HTTP fixtures. Rust. |
| Cargo manifests and lockfile, Markdown/JSON fixtures | Configuration and data | Required formats, not executable conversion targets. |
| serde, serde_yaml, serde_json, reqwest, tempfile, and transitive crates | Third-party Rust dependencies | Parsing, HTTP, temporary files, and test infrastructure. Not agent-authored non-Rust logic. |
| Gitea REST API | External platform interface | Network protocol. Local fixture coverage plus an isolated live-instance verification; production remains unverified. |
| `skills/story-time/scripts/` | Existing JavaScript planners and tracker adapters | Upstream includes a separate Node Gitea adapter. This Rust path never calls these implementations; they remain outside its conversion scope. |
| Root `scripts/`, `test/`, and npm entry points | Existing JavaScript installation and tests | Unchanged and out of scope. Not used as Gitea runtime or verification evidence. Cargo installation is explicit. |
| Story Time and setup skills, this document | Agent instructions and documentation | Route Gitea directly to Rust; other tracker behavior remains historical. |

An isolated live Gitea server passed the native story flow. See [live verification](gitea-live-verification.md). Production version compatibility, token permissions, and TLS remain unverified. Story discovery, milestones, assignees, comments, pull requests, and general repository management are outside this bounded flow. Concurrent label editors are not locked; label replacement can overwrite changes made after the command reads current labels. Repository label pagination is bounded to 1,000 pages. Label creation races return the server error without automatic retries. See [the verification record](gitea-story-time-verification.md) for observed results and remaining evidence gaps.
