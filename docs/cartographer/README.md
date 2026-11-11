# Cartographer project setup

Status: repository preparation only. Live registration and autonomous maintenance are **not verified**.

Story Time belongs to the `team-teddy` organization in Cartographer. Its source remains on GitHub at <https://github.com/team-teddy-development/story-time>. The Cartographer organization name does not move the repository to Gitea.

[project-binding.json](project-binding.json) records the intended identity and inspected source commit. It is documentation data, not a Cartographer-consumed configuration format or an admission receipt. Neither Cartographer nor the UI imports this file automatically.

## Repository configuration

`.story-time/settings.json` configures the file backlog in `backlog/`, prefix `ST`, and WIP limit 1. Issue synchronization is off. No external tracker writes or credentials are needed. `ST-001` tracks onboarding and retains the unmet live-registration criterion.

Run these commands from an owned checkout of this repository:

```sh
story config --json
story list --json
npm test
```

The test command uses the existing Node.js tests. It does not prove Cartographer admission, UI visibility, or worker execution. New and replacement executable implementations and tests must use Rust. Markdown skills, JSON configuration, and fixture data retain their required formats.

## Admission handoff

The operator must use Cartographer's existing signed registry tools. Do not add this project to a hard-coded UI list or insert unsigned Neo4j records.

The inspected core runbooks are `docs/runbooks/persistent-admission-signing.md` and `docs/runbooks/registry-admission.md` in `team-teddy/cartographer`. The UI's Rust portfolio reader uses the v2 owner listing when configured. This session did not inspect production configuration or prove that production has the v2 route enabled.

### Prepare on the operator host

1. Read the current runbooks in an owned Cartographer checkout and confirm the deployed authority version.
2. Obtain the current trust, ownership, and policy records without transferring private keys to an agent.
3. Prepare `story-time-admission.json` using the signing runbook's exact spec schema and the identity table below.
4. Supply the actual owner, distinct custodians, signing key IDs, issue time, policy content, and current revision preconditions.
5. Confirm that adding Story Time preserves every existing project admission and ownership record.

| Field | Value |
| --- | --- |
| `organization` | `cartographer:organization:team-teddy` |
| `scope.tenant` | `cartographer:tenant:team-teddy` |
| `scope.project` | `cartographer:project:story-time` |
| New decision's `projects` | `["cartographer:project:story-time"]` |
| Proposed new decision ID | `cartographer:decision:story-time-admission` |
| Source | `https://github.com/team-teddy-development/story-time` |
| Inspected source branch and commit | `main` at `1cbe7d82eb319ca458b06ba8eb2d603f3a97d69d` |

The decision ID is proposed, not reserved. Determine revisions from current registry state. Do not reuse a Team Teddy-wide decision ID with only this project in its project list. A nonempty registry needs the appropriate trusted signers and project ownership; publication does not bootstrap missing ownership automatically. Use the registry's existing authorized ownership procedure before publishing when required.

Placement, execution, provider-egress, and retention policies remain separate signed records. Bind the approved source repository through the backend's actual source authorization mechanism. The owner request supplies the desired project identity, not missing key identities, policy text, trust records, or an execution permit.

### Sign, verify, and publish

Run on the operator host only. Set the path variables to existing operator-controlled files. Keep `SIGNED` outside Git and choose a directory that does not already contain signed outputs. The four policy-role keys and owner key must be separate, mode-600 files. Never paste their contents into chat or evidence.

```sh
owner-admission-v2 sign \
  --spec "$SPEC" \
  --owner-key-file "$OWNER_KEY_FILE" \
  --custodian-key-file "placement=$PLACEMENT_KEY_FILE" \
  --custodian-key-file "execution=$EXECUTION_KEY_FILE" \
  --custodian-key-file "provider_egress=$EGRESS_KEY_FILE" \
  --custodian-key-file "retention=$RETENTION_KEY_FILE" \
  --out "$SIGNED"

owner-admission-v2 verify \
  --dir "$SIGNED" \
  --owner-key-file "$OWNER_KEY_FILE" \
  --custodian-key-file "placement=$PLACEMENT_KEY_FILE" \
  --custodian-key-file "execution=$EXECUTION_KEY_FILE" \
  --custodian-key-file "provider_egress=$EGRESS_KEY_FILE" \
  --custodian-key-file "retention=$RETENTION_KEY_FILE"

cartographer-authority --registry-admission "$AUTHORITY_CONFIG" "$SIGNED"
cartographer-authority --registry-admission "$AUTHORITY_CONFIG" "$SIGNED"
```

`SPEC` names the completed `story-time-admission.json`. `AUTHORITY_CONFIG` names the private production configuration already held by the operator. The sign command must print `SIGNED`; verify must print `VERIFIED`. The second publication must print `authority_v2_registry_admission_already_current`.

These commands are not an instruction to provision a new production registry. If the deployed registry uses a different contract, stop this publication sequence and use that version's supported onboarding procedure. Do not deploy an unqualified authority binary or replace production configuration to make this handoff work.

### Verify and record the result

1. Use the authenticated `POST /v2/owner/projects` route to confirm the exact organization, tenant, and project with status `active`.
2. Confirm the panel and bridge target the same authority and have `CARTOGRAPHER_AUTHORITY_V2_URL` configured when using v2.
3. Open the authenticated UI portfolio and confirm Story Time appears under Team Teddy without a UI redeploy.
4. Confirm existing projects still appear and record sanitized evidence in [admission-evidence.md](admission-evidence.md).
5. Before a build, verify the separate source binding, worker capability, execution policies, and a bounded task's real result.

Read admission is not proof of build or maintenance capability. Do not mark onboarding complete until the story's live criterion has evidence. Never put bearer tokens, private configuration, or signing keys into the evidence file.

## Scope and language audit

| Active path | Language or format | Role | Conversion status |
| --- | --- | --- | --- |
| `.story-time/settings.json` | JSON | Repository planning configuration | Non-executable configuration |
| `docs/cartographer/*.json` | JSON | Admission preparation record | Non-executable data, not imported by runtime |
| `backlog/` and `skills/*/SKILL.md` | Markdown | Stories and agent instructions | Non-executable documentation |
| `skills/story-time/scripts/story.mjs` and `scripts/*.mjs` | JavaScript | Existing CLI, installer, and test runner | Out-of-scope conversion target |
| `skills/story-time/scripts/src/**/*.mjs` | JavaScript | Existing application and adapters | Out-of-scope conversion target |
| `test/` and `skills/story-time/scripts/test/` | JavaScript | Existing tests | Out-of-scope conversion target |
| Cartographer signing and authority tools | Rust | External admission services | Existing tools, unchanged |
| Cartographer UI and bridge | Rust active readers; retained historical JavaScript | External portfolio and graph readers | Unchanged, outside this repository's setup scope |

Rust conversion gate: **UNMET for full-project conversion**. No executable implementation is added or replaced by this configuration change. A later implementation story must inventory its active execution chain and satisfy the Rust gate for its scope. This setup does not claim that the existing JavaScript project is converted.

## Recovery

These files do not activate a project, start a worker, change a remote, or deploy a service. Revert the setup commit to remove repository preparation. After registry publication, use the registry's signed revocation procedure for the exact Story Time decision. Do not delete registry history, restore older authority floors, or revoke unrelated Team Teddy projects.
