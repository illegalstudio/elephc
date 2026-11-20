# syntax=docker/dockerfile:1
# Build the elephc origin in a multi-stage image.
#
# Build from the repository root:
#   docker build -f examples/web-edge/deploy/Dockerfile.app -t elephc-edge-app .
#
# The final image runs the compiled binary directly as PID 1. There is no shell
# wrapper and no process supervisor: the origin's own prefork master supervises
# its workers, and the container runtime only needs to deliver SIGTERM.
FROM rust:1.98-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --locked --bin elephc \
    && ./target/release/elephc --web examples/web-edge/main.php

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 elephc-web \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin elephc \
    && mkdir -p /run/elephc /var/lib/elephc/sessions \
    && chown -R 10001:10001 /run/elephc /var/lib/elephc

COPY --from=builder --chown=10001:10001 /src/examples/web-edge/main /usr/local/bin/app

ENV ELEPHC_SESSION_SAVE_PATH=/var/lib/elephc/sessions
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/app"]
CMD ["--listen", "unix:/run/elephc/app.sock", "--workers", "4", "--trusted-proxy", "unix"]
STOPSIGNAL SIGTERM
