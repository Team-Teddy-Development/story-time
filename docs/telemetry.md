# Telemetry

The native `story` binary can send one OTLP log event for each command it runs. Export is off by default. A run sends nothing and opens no connection unless an OTLP endpoint variable is set. The supported route is a GitHub Actions job that posts to the Xanthos ingest gateway with the job's OIDC token (decision in `docs/spikes/ST-122-observability-inventory.md`). Developer-machine export is out of scope.

## Configuration

| Variable | Effect |
| --- | --- |
| `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT` | Full logs URL, used as given, for example `https://xanthos.teamteddy.net/v1/logs`. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Base URL. `/v1/logs` is appended. Used when the logs variable is unset or empty. |
| `OTEL_EXPORTER_OTLP_LOGS_HEADERS`, `OTEL_EXPORTER_OTLP_HEADERS` | Comma-separated `key=value` request headers with percent-encoded values, for example `Authorization=Bearer%20<token>`. The logs variable wins. |
| `TRACEPARENT` | W3C trace context. A valid value supplies the event's trace id, so a caller can choose and record the correlation id. Otherwise the id is random. |

The exporter speaks OTLP/HTTP with JSON encoding (`Content-Type: application/json`). Xanthos accepts `application/json` and `application/x-protobuf` and forwards the body unchanged (`xanthos/packages/ingest/src/server.ts:63`, `:340-344`).

## Failure behavior

Export happens after the command has written its output. One request is posted with a 2 second total timeout, so a command never waits longer than that for telemetry. The payload is a single record of a few hundred bytes; nothing is queued, retried, or written to disk. Every export error is dropped without a message, so stdout, stderr, and the exit code are the same with telemetry on, off, or failing.

## Event

| Field | Value |
| --- | --- |
| `timeUnixNano` | When the command finished. |
| `severityText` / `severityNumber` | `INFO` / 9 for exit code 0, `ERROR` / 17 otherwise. |
| `body` | `story <operation> <outcome>`. |
| `traceId` | The correlation id. ST-010 traces reuse it, so Grafana's Loki-to-Tempo link works. |
| `story.operation` | The command name from the native dispatch table, or `unknown`. |
| `story.outcome` | `success` or `failure`. |
| `story.exit_code` | The process exit code. |
| `story.duration_ms` | Wall time of the command in milliseconds. |

The event is built only from these values. Command arguments, story files, command output, and environment variables are never read into it, so credentials and story bodies cannot be exported.

## Labels

Loki turns OTLP resource attributes into stream labels and stores record attributes as structured metadata. The resource attribute set is fixed at `service.name="story-time"` and `service.version`. Loki indexes `service.name` as the label `service_name`; the other values are structured metadata (`story_operation`, `story_outcome`, `story_exit_code`, `story_duration_ms`, `trace_id`, `service_version`). Adding a resource attribute adds an index label, so it needs a change to this document and to `native/story-push/tests/telemetry_export.rs`.

## Finding events

```logql
{service_name="story-time"} | story_outcome="failure"
{service_name="story-time"} | story_operation="show" | story_outcome="success"
{service_name="story-time"} | trace_id="<correlation id>"
```

`.github/workflows/telemetry-smoke.yml` runs successful and failing commands through the gateway and prints the correlation id of each.
