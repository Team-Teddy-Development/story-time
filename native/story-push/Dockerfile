# The story-time hosted MCP server (ADR 0007). The build context is the repository root,
# because the crate includes skills/story-time/scripts/usage.txt:
#
#   docker build -f native/story-push/Dockerfile -t story-time .
#
# The runbook is docs/runbooks/story-time-image.md.

# The compiler runs on the platform of the build machine and cross-compiles when the target
# platform is another one. An emulated rustc is slow, and it crashes under QEMU on arm64.
FROM --platform=$BUILDPLATFORM rust:1-bookworm AS build
ARG BUILDARCH
ARG TARGETARCH
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) triple=x86_64-unknown-linux-gnu; gcc=x86-64-linux-gnu; cc=x86_64-linux-gnu-gcc ;; \
      arm64) triple=aarch64-unknown-linux-gnu; gcc=aarch64-linux-gnu; cc=aarch64-linux-gnu-gcc ;; \
      *) echo "unsupported architecture: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    echo "$triple" > /triple; \
    : > /cross-env; \
    if [ "$BUILDARCH" != "$TARGETARCH" ]; then \
      apt-get update; \
      apt-get install -y --no-install-recommends "gcc-$gcc" "libc6-dev-$TARGETARCH-cross"; \
      rm -rf /var/lib/apt/lists/*; \
      rustup target add "$triple"; \
      printf '[target.%s]\nlinker = "%s"\n' "$triple" "$cc" >> "$CARGO_HOME/config.toml"; \
      echo "export CC_$(echo "$triple" | tr - _)=$cc" > /cross-env; \
    fi
WORKDIR /src
COPY native/story-push native/story-push
COPY skills/story-time/scripts/usage.txt skills/story-time/scripts/usage.txt
RUN . /cross-env \
    && cargo build --release --locked --target "$(cat /triple)" \
       --manifest-path native/story-push/Cargo.toml --bin story \
    && cp "native/story-push/target/$(cat /triple)/release/story" /story

FROM debian:bookworm-slim
# The server runs the system git to clone and fetch the registered repositories.
RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --user-group --home-dir /data/home --shell /usr/sbin/nologin story \
    && install -d -o 10001 -g 10001 /data
COPY --from=build /story /usr/local/bin/story
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["story", "mcp", "--http", "0.0.0.0:8080"]
