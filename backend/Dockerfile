FROM rust:1.94-slim AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    gcc \
    libc6-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

ARG MIGRATIONS_HASH=unknown
RUN echo "migrations_hash=${MIGRATIONS_HASH}" && \
    cargo build --release --bin shipyard

# Download the static Docker CLI binary (just the `docker` command — no daemon,
# no containerd, no runc) and the Compose plugin.  Doing this in the builder
# means the runtime image needs no curl/wget at all.
ARG DOCKER_CLI_VERSION=27.5.0
ARG COMPOSE_VERSION=v2.27.1
RUN set -eux; \
    ARCH=$(uname -m); \
    curl -fsSL "https://download.docker.com/linux/static/stable/${ARCH}/docker-${DOCKER_CLI_VERSION}.tgz" \
        | tar -xz --strip-components=1 -C /usr/local/bin docker/docker; \
    curl -fsSL "https://github.com/docker/compose/releases/download/${COMPOSE_VERSION}/docker-compose-linux-${ARCH}" \
        -o /docker-compose && chmod +x /docker-compose

# ─── Runtime ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Only runtime library deps — no curl, no docker daemon, no containerd.
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    libssl3 \
    git \
    openssh-client \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /usr/local/lib/docker/cli-plugins

# Static Docker CLI (musl-compiled upstream binary, no glibc dep, ~60 MB).
COPY --from=builder /usr/local/bin/docker                  /usr/local/bin/docker
# Docker Compose v2 plugin.
COPY --from=builder /docker-compose                        /usr/local/lib/docker/cli-plugins/docker-compose
# Shipyard binary (debug symbols stripped by [profile.release] strip = true).
COPY --from=builder /app/target/release/shipyard           /usr/local/bin/shipyard

ARG GIT_SHA=dev
ARG BUILD_DATE=unknown
ARG DOCKER_REPO=

ENV SHIPYARD_GIT_SHA=$GIT_SHA \
    SHIPYARD_BUILD_DATE=$BUILD_DATE \
    SHIPYARD_DOCKER_REPO=$DOCKER_REPO

EXPOSE 3001

CMD ["/usr/local/bin/shipyard"]
