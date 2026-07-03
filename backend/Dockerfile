FROM rust:1.88-slim AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    gcc \
    libc6-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Pass a hash of migration files as a build arg so the layer cache busts
# whenever any migration file is added or changed, even if Cargo.lock is unchanged.
ARG MIGRATIONS_HASH=unknown
RUN echo "migrations_hash=${MIGRATIONS_HASH}" && \
    cargo build --release --bin shipyard

# ─── Runtime ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    libpq5 \
    libssl3 \
    git \
    openssh-client \
    docker.io \
    && mkdir -p /usr/local/lib/docker/cli-plugins \
    && curl -fsSL "https://github.com/docker/compose/releases/download/v2.27.1/docker-compose-linux-$(uname -m)" \
       -o /usr/local/lib/docker/cli-plugins/docker-compose \
    && chmod +x /usr/local/lib/docker/cli-plugins/docker-compose \
    && rm -rf /var/lib/apt/lists/*

ARG GIT_SHA=dev
ARG BUILD_DATE=unknown
ARG DOCKER_REPO=

ENV SHIPYARD_GIT_SHA=$GIT_SHA \
    SHIPYARD_BUILD_DATE=$BUILD_DATE \
    SHIPYARD_DOCKER_REPO=$DOCKER_REPO

COPY --from=builder /app/target/release/shipyard /usr/local/bin/shipyard

EXPOSE 3001

CMD ["/usr/local/bin/shipyard"]
