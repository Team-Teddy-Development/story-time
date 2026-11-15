# The story-time hosted MCP server (ADR 0007). The build context is the repository root,
# because the crate includes skills/story-time/scripts/usage.txt:
#
#   docker build -f native/story-push/Dockerfile -t story-time .
#
# The runbook is docs/runbooks/story-time-image.md.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY native/story-push native/story-push
COPY skills/story-time/scripts/usage.txt skills/story-time/scripts/usage.txt
RUN cargo build --release --locked --manifest-path native/story-push/Cargo.toml --bin story

FROM debian:bookworm-slim
# The server runs the system git to clone and fetch the registered repositories.
RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --user-group --home-dir /data/home --shell /usr/sbin/nologin story \
    && install -d -o 10001 -g 10001 /data
COPY --from=build /src/native/story-push/target/release/story /usr/local/bin/story
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["story", "mcp", "--http", "0.0.0.0:8080"]
