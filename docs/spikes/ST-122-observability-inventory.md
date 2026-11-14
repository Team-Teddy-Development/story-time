# ST-122 observability inventory

Question: which Loki, VictoriaMetrics, Tempo and Grafana deployments already exist for Team Teddy, who owns them, and how would a story-time process send them data?

Answer: one stack exists. It is the `drip` namespace on the coffee cluster, owned by the `drip-observability` Argo CD Application in `team-teddy/coffee-apps`. All four components are ClusterIP only. The single write path is the OpenTelemetry Collector at `otel-collector.drip.svc.cluster.local:4318` (OTLP/HTTP, bearer token). The only route from outside the cluster is the Xanthos ingest gateway at `https://xanthos.teamteddy.net`, which accepts GitHub Actions OIDC tokens from the `Team-Teddy-Development` owner.

## Method

The inventory was read only. No apply, deploy, port-forward, or configuration change ran. No Secret data was read; only Secret names that manifests reference are recorded.

Sources:

- `coffee-apps` anchor at `/home/ejernigan/source/team-teddy/coffee-apps-cc5`, commit `e1f37bc9aa03`. Argo CD reports the live revision `b427acd89b7b`. `git diff --stat e1f37bc b427acd -- k8s/drip argocd/drip-observability.yaml` is empty, so the `k8s/drip` files cited below match the live revision.
- `coffee-cluster-talos` anchor at commit `00af5b2ae6e3`.
- Live cluster, kubectl context `admin@coffee-cluster-1` (user `agent-identity`), on 2026-11-14, with `get` only.

Search: `grep -rIil -E '\b(loki|victoria-?metrics|vmagent|tempo|grafana|otel|opentelemetry|otlp|alloy|promtail)\b'` over `/home/ejernigan/source/team-teddy/` (worktrees, `node_modules`, `target` excluded). Cluster-wide: `kubectl get deploy,sts,ds -A` filtered for the same names. No second Loki, VictoriaMetrics, Tempo, Grafana, Prometheus, Alloy, or vmagent deployment exists on the cluster. No Alloy configuration exists in any repository.

Live state:

```text
$ kubectl -n argocd get applications.argoproj.io drip-observability -o jsonpath='{.status.sync.status} {.status.health.status} {.status.sync.revision}'
Synced Healthy b427acd89b7b4eddb5809da338d21bc8a0ddf837

$ kubectl -n drip get deploy,sts,svc,ingress -o wide
statefulset grafana, loki, tempo, victoriametrics   1/1 each
deployment  otel-collector 1/1, xanthos-ingest 1/1
service     grafana 3000, loki 3100/9095, otel-collector 4318/8889,
            tempo 3200/4317/4318, victoriametrics 8428, xanthos-ingest 8080/8081  (all ClusterIP)
ingress     xanthos-ingest  host xanthos.teamteddy.net  class nginx
```

## Inventory

| Component | Owner | Endpoint | Ingest path | Authentication | Retention |
|---|---|---|---|---|---|
| VictoriaMetrics (single node) | `drip-observability` Argo app, `coffee-apps` `k8s/drip` | `http://victoriametrics.drip.svc.cluster.local:8428` | Pull only. Static scrape of `otel-collector:8889`, filtered to an allowlist of metric names | None on the HTTP listener | 90 days |
| Loki (single binary, filesystem) | Same Argo app | `http://loki.drip.svc.cluster.local:3100` | Collector `otlphttp/loki` to `/otlp`, and Promtail DaemonSet push to `/loki/api/v1/push` | None (`auth_enabled: false`) | 14 days; `{job="kube-audit"}` 32 days |
| Tempo (single binary) | Same Argo app | Query `http://tempo.drip.svc.cluster.local:3200`; OTLP 4317 gRPC, 4318 HTTP | Collector `otlphttp/tempo` | None on the receiver; the Collector is the authenticated front | 7 days (`block_retention: 168h`); blocks in private MinIO bucket `lb0200-tempo` with a 7-day expiry |
| Grafana | Same Argo app | `http://grafana.drip.svc.cluster.local:3000` | Datasources `drip-vm`, `drip-loki`, `drip-tempo`, with trace-to-log and log-to-trace links | Local admin user (Secret `drip-grafana-admin`); anonymous access off; sign-up off. Operator access is `kubectl port-forward` only | Not applicable; state on a PVC |
| OpenTelemetry Collector (contrib) | Same Argo app | `http://otel-collector.drip.svc.cluster.local:4318` (OTLP/HTTP only, no gRPC) | Receives OTLP traces, metrics and logs | Bearer token (`bearertokenauth/server`, Secret `drip-otel-auth`) | None (forwarding) |
| Xanthos ingest gateway | Xanthos (`team-teddy/xanthos` `packages/ingest`), manifest in the same Argo app | Public `https://xanthos.teamteddy.net` `/v1/{logs,traces,metrics}`; internal `xanthos-ingest.drip.svc:8081` | Forwards OTLP to the Collector with the Collector bearer token | Public listener: GitHub Actions OIDC JWT, audience `https://xanthos.teamteddy.net`, `repository_owner` in `Team-Teddy-Development`. Internal listener: no client auth | None |
| Promtail | `coffee-cluster-talos` `kubernetes/observability/promtail.yaml`, applied with `kubectl` (no Argo tracking) | DaemonSet in `logging`, 8 of 8 nodes | Tails every pod's stdout on each node and pushes to Loki | None (Loki has none) | Not applicable |

### Evidence

Ownership:

- `coffee-apps-cc5/argocd/drip-observability.yaml:4` names `drip-observability`; `:11-13` source `team-teddy/coffee-apps.git`, `main`, path `k8s/drip`; `:18-20` automated sync with prune and self-heal.
- `coffee-apps-cc5/docs/lb-0200-drip-observability.md:5` states that the `drip` namespace owns all five components through that Application. The stack came from the Varsity LB-0200 work item.
- Promtail: `kubectl -n logging get ds promtail -o jsonpath='{.metadata.annotations.argocd\.argoproj\.io/tracking-id}'` is empty, and the field managers are `kubectl-client-side-apply kubectl-rollout`. `grep -rln promtail` over `coffee-cluster-talos/apps` finds nothing.

VictoriaMetrics:

- `k8s/drip/10-victoriametrics.yaml:110-120` ClusterIP Service `victoriametrics`, port 8428.
- `:159` `-retentionPeriod=90d`, confirmed live by `kubectl -n drip get sts victoriametrics -o jsonpath='{.spec.template.spec.containers[0].args}'`.
- `:81-88` scrape job `otel-collector` keeps only `regex: 'lb0200_.+|agent_harness_.+|xanthos_.+'`, confirmed live by `kubectl -n drip get cm victoriametrics-scrape`. A story-time metric with any other prefix is dropped at scrape time.
- No remote-write receiver is used. The Collector exposes metrics on its Prometheus exporter (`40-otel-collector.yaml:81-83`) and VictoriaMetrics pulls them.

Loki:

- `k8s/drip/20-loki.yaml:8` `auth_enabled: false`. `:33` `retention_period: 336h`. `:38-41` `kube-audit` stream at 768h. `:44` `allow_structured_metadata: true`. Confirmed live from ConfigMap `loki-config`.
- `:59-72` ClusterIP Service `loki`, ports 3100 and 9095.
- `coffee-cluster-talos/kubernetes/observability/promtail.yaml:57` client `http://loki.drip.svc:3100/loki/api/v1/push`; `:74` `cri` pipeline stage; `:77-88` labels node, namespace, pod, container, app.

Tempo:

- `k8s/drip/30-tempo.yaml:13-17` OTLP receivers gRPC 4317 and HTTP 4318. `:22` `block_retention: 168h`, confirmed live from ConfigMap `tempo-config`. `:29` MinIO endpoint `lb0196-minio-api.varsity-social-artifacts.svc.cluster.local:9000`.
- `docs/lb-0200-drip-observability.md:13` 7-day bucket expiry and dedicated MinIO identity.

Collector:

- `k8s/drip/40-otel-collector.yaml:11-18` bearer-token authenticator on the only receiver, OTLP/HTTP 4318.
- `:24-59` processors delete attribute keys such as `url.full`, `url.path`, `http.route`, `user.id`, `exception.message`, `exception.stacktrace`, `prompt`, `authorization`.
- `:66-69` `spanmetrics` connector with namespace `lb0200`, so span-derived metrics are named `lb0200_*`.
- `:89-104` pipelines: traces to Tempo and spanmetrics; OTLP metrics to the Prometheus exporter with `deltatocumulative`; logs to Loki.
- `:72-80` exporters retry for at most 30 s with a 512-item queue.

Grafana:

- `k8s/drip/50-grafana-config.yaml:20-21` `[auth.anonymous] enabled = false`. `:54-97` datasources: `drip-vm` (prometheus type), `drip-loki` with a `TraceID` derived field to `drip-tempo`, `drip-tempo` with `tracesToLogsV2` to `drip-loki` and service map from `drip-vm`.
- `k8s/drip/53-grafana-workload.yaml:10-14` ClusterIP Service on 3000. `:78-83` admin user from Secret `drip-grafana-admin`.
- `docs/lb-0200-drip-observability.md:5` operator access by authenticated `kubectl port-forward` only.

Xanthos ingest:

- `k8s/drip/70-xanthos-ingest.yaml:1-15` path Cloudflare to cloudflared to ingress-nginx to `xanthos-ingest:8080` to `otel-collector:4318`, and the OIDC auth model. `:47-54` upstream endpoint, OIDC issuer, audience, and `ALLOWED_OWNERS=Team-Teddy-Development`. `:105-124` Ingress host `xanthos.teamteddy.net`, 20 requests per second limit, 4 MB body limit.
- `xanthos/packages/ingest/src/server.ts:56-60` routes `/v1/logs`, `/v1/traces`, `/v1/metrics` as generic OTLP signals. `:333-338` the public listener checks the OIDC token and owner before forwarding.

Network policy:

- `k8s/drip/60-network-policies.yaml:1-11` `default-deny` for all `drip` pods, annotated "intent-only on Coffee Flannel; no policy controller is installed".
- That annotation is stale. `kubectl get ds -A` shows `kube-system/kube-network-policies` 8 of 8 ready, 33 days old, deployed by `coffee-apps-cc5/argocd/kube-network-policies.yaml`. The policies are therefore probably enforced now. This is inferred from the controller being present; no connection test was run.
- `60-network-policies.yaml:31-42` allows Collector ingress on 4318 only from `varsity-social-artifacts/lb0196-gateway`. `70-xanthos-ingest.yaml:126-148` allows the ingest gateway to reach the Collector. `60-network-policies.yaml:199-215` allows Loki ingress from the Collector, Grafana, and the `logging` namespace.

## Story-time execution surfaces

Story-time has no telemetry dependency today. `grep -E '^(tracing|opentelemetry|log)'` over every `Cargo.toml` in the repository finds nothing. It runs in three places:

1. The `story` CLI on developer machines (Spark), outside the cluster. No route exists from Spark to the ClusterIP services except `kubectl port-forward`.
2. GitHub Actions on `ubuntu-latest` (`.github/workflows/test.yml`, `gitea-rust.yml`). The repository owner `Team-Teddy-Development` is on the Xanthos ingest allowlist.
3. A possible future MCP server (ST-006). No in-cluster story-time workload exists.

## Recommendations

- ST-009 metrics. Do not write to VictoriaMetrics directly; it is pull only. Emit OTLP metrics to the Collector, as `agent_harness_*` and `xanthos_*` already do. Use one fixed prefix, for example `story_time_`, and add it to the `otel-collector` keep regex at `10-victoriametrics.yaml:88`. That one-line `coffee-apps` change is a prerequisite and needs a PR there. Send cumulative histograms, or delta, which `deltatocumulative` converts.
- ST-010 traces. Emit OTLP/HTTP protobuf traces to the Collector. Do not send directly to Tempo; Tempo's receiver has no authentication and the Collector holds the redaction rules. Span-derived RED metrics will appear as `lb0200_*` from the shared spanmetrics connector, so ST-009 should not depend on them. Put the trace ID in each log record so the existing Loki `TraceID` derived field and Tempo `tracesToLogsV2` links work without Grafana changes.
- ST-008 logs. Emit OTLP logs through the same exporter and endpoint. Loki stores OTLP resource attributes as labels and other attributes as structured metadata (`allow_structured_metadata: true`), so keep resource attributes to a small bounded set such as `service.name`. Promtail only helps a process that runs as a cluster pod, so it does not fit the CLI.
- One exporter for all three signals. Use a single OTLP/HTTP exporter in Rust (the `opentelemetry-otlp` crate with `http-proto`) with a configurable endpoint and auth mode, off by default. For GitHub Actions, send to `https://xanthos.teamteddy.net/v1/*` with the job's OIDC token (`id-token: write`, audience `https://xanthos.teamteddy.net`). No stored secret is needed. For a developer machine, no sanctioned path exists. The test path is a port-forward to `otel-collector:4318` with the bearer token, run by the owner, or a local Collector for tests.

## Follow-up questions

1. Can story-time share the Xanthos ingest gateway, or does Team Teddy want a generic OTLP ingest gateway? The gateway is Xanthos code, its audience names Xanthos, and its `ALLOWED_OWNERS` already admits every repository in the organization.
2. Who approves adding `story_time_` to the VictoriaMetrics keep regex in `coffee-apps`? A Grafana dashboard for ST-009 also lives in `coffee-apps` (`51-grafana-dashboard.yaml` pattern) and needs the same owner.
3. Is the CLI on developer machines in scope for ST-008, ST-009 and ST-010, or only CI and a future MCP server? A developer-machine export needs either a public authenticated route or a new secret distribution path.
4. Are the `drip` NetworkPolicies enforced now? `kube-network-policies` runs on every node, but the `default-deny` annotation says intent only. An in-cluster story-time workload would need an allow rule for `otel-collector:4318`.
5. Are 14 days of Loki retention and 7 days of Tempo retention enough for story-time? Changing them is out of scope for ST-008.

## Not checked

- Live connectivity to the Collector or Loki. A test would need a port-forward and the bearer token, which this spike must not read.
- Whether the Xanthos gateway accepts OTLP JSON or only protobuf, and whether it rewrites `service.name`. Only `server.ts:56-60` and `:333-338` were read.
- Grafana users, teams, and folder permissions. They live in the Grafana database and need a signed-in query.
- Whether NetworkPolicies are enforced in practice (see follow-up 4).

## Decision

Decided 2026-11-14 by Jev (jev-1.14.0) under the owner's "jev decide" instruction:

- Route (0.85): story-time sends OTLP/HTTP only to the Xanthos gateway, `https://xanthos.teamteddy.net/v1/{logs,traces,metrics}`, from GitHub Actions with an OIDC token. It never sends to the in-cluster collector or the backends directly.
- Scope (0.60): developer-machine CLI runs are out of scope. Telemetry is off unless an OTLP endpoint is configured, so a plain CLI run stays silent.
- Metrics (0.64): story-time metrics use the `story_time_` prefix. The VictoriaMetrics keep regex in coffee-apps `k8s/drip/10-victoriametrics.yaml:88` must add that prefix. That change is a coffee-apps PR and a cluster deploy, so it is filed as its own story and ST-009's live acceptance depends on it.
