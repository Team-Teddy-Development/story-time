# Gitea issue gateway

This standalone Rust library creates, updates, and closes Gitea issues. It uses the Gitea REST API and returns typed issue responses. Provide an API token to `Gateway::new`; the library sends it in the Authorization header. Do not log request headers.

The base URL can include a path prefix. HTTPS is required except for literal loopback IP addresses over HTTP. URLs with credentials, queries, or fragments are rejected. Redirects are disabled. Connections time out after 5 seconds and requests after 15 seconds. Successful response bodies are limited to 1 MiB. Errors do not include response bodies, URLs, tokens, or underlying transport errors.

## Verify

```sh
cd gitea-gateway
CARGO_TARGET_DIR=/tmp/story-gitea-6723 cargo test --offline
```

The integration tests run against a local Rust TCP server. They do not contact Gitea.

## Integration gaps

No application calls this library yet. Token provisioning, retries, rate limiting, reconciliation after ambiguous failures, and mapping application-specific fields are caller responsibilities. The library has not been tested against a live Gitea instance. Operations are not automatically retried. A timeout after a create request can leave the caller uncertain whether the issue was created.
