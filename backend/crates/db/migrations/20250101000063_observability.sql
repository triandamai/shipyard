-- Observability: durable logs, request rollups, visitor hashes, resource samples, and hourly rollups.

CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- 1. Partitioned log entries (by day)
CREATE TABLE log_entries (
    id             BIGINT GENERATED ALWAYS AS IDENTITY,
    ts             TIMESTAMPTZ NOT NULL,
    org_id         UUID REFERENCES organizations(id) ON DELETE CASCADE,
    project_id     UUID REFERENCES projects(id) ON DELETE CASCADE,
    service_id     UUID REFERENCES services(id) ON DELETE CASCADE,
    deployment_id  UUID,
    source         TEXT NOT NULL,
    container_id   TEXT,
    replica        INTEGER,
    stream         TEXT NOT NULL,
    level          SMALLINT NOT NULL,
    trace_id       TEXT,
    span_id        TEXT,
    message        TEXT NOT NULL,
    fields         JSONB,
    PRIMARY KEY (ts, id)
) PARTITION BY RANGE (ts);

-- Default partition so unpartitioned timestamps are safely caught
CREATE TABLE log_entries_default PARTITION OF log_entries DEFAULT;

-- Initial partitions for yesterday, today, and tomorrow
CREATE TABLE log_entries_y2026m10d28 PARTITION OF log_entries
    FOR VALUES FROM ('2026-10-28 00:00:00+00') TO ('2026-10-29 00:00:00+00');
CREATE TABLE log_entries_y2026m10d29 PARTITION OF log_entries
    FOR VALUES FROM ('2026-10-29 00:00:00+00') TO ('2026-10-30 00:00:00+00');
CREATE TABLE log_entries_y2026m10d30 PARTITION OF log_entries
    FOR VALUES FROM ('2026-10-30 00:00:00+00') TO ('2026-10-31 00:00:00+00');

CREATE INDEX log_entries_service_ts ON log_entries (service_id, ts DESC);
CREATE INDEX log_entries_org_ts ON log_entries (org_id, ts DESC);
CREATE INDEX log_entries_trace_id ON log_entries (trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX log_entries_message_trgm ON log_entries USING gin (message gin_trgm_ops);

-- 2. Log cursor table for container log collector resume state
CREATE TABLE log_cursors (
    container_id TEXT PRIMARY KEY,
    last_seen    TIMESTAMPTZ NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 3. Request rollups per env per minute
CREATE TABLE request_rollups_1m (
    id                BIGSERIAL PRIMARY KEY,
    project_id        UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    environment       TEXT,
    bucket            TIMESTAMPTZ NOT NULL,
    requests          BIGINT NOT NULL DEFAULT 0,
    s2xx              BIGINT NOT NULL DEFAULT 0,
    s3xx              BIGINT NOT NULL DEFAULT 0,
    s4xx              BIGINT NOT NULL DEFAULT 0,
    s5xx              BIGINT NOT NULL DEFAULT 0,
    latency_p50_ms    INTEGER,
    latency_p95_ms    INTEGER,
    latency_buckets   BIGINT[] NOT NULL DEFAULT '{}',
    bytes             BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT uq_request_rollups_1m UNIQUE (project_id, environment, bucket)
);

CREATE INDEX request_rollups_1m_proj_bucket ON request_rollups_1m (project_id, bucket DESC);

-- 4. Truncated visitor hashes (daily salt, kept 8 days)
CREATE TABLE visitor_hashes (
    id          BIGSERIAL PRIMARY KEY,
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    environment TEXT,
    bucket      TIMESTAMPTZ NOT NULL,
    hash        TEXT NOT NULL,
    CONSTRAINT uq_visitor_hashes UNIQUE (project_id, environment, bucket, hash)
);

CREATE INDEX visitor_hashes_proj_bucket ON visitor_hashes (project_id, bucket DESC);

-- 5. Resource samples per container per minute (48h raw retention)
CREATE TABLE resource_samples_1m (
    id                  BIGSERIAL PRIMARY KEY,
    project_id          UUID REFERENCES projects(id) ON DELETE CASCADE,
    service_id          UUID REFERENCES services(id) ON DELETE CASCADE,
    service_slug        TEXT NOT NULL,
    container_id        TEXT NOT NULL,
    bucket              TIMESTAMPTZ NOT NULL,
    cpu_percent         DOUBLE PRECISION NOT NULL,
    memory_percent      DOUBLE PRECISION NOT NULL,
    memory_bytes        BIGINT NOT NULL,
    memory_limit_bytes  BIGINT NOT NULL
);

CREATE INDEX resource_samples_1m_lookup ON resource_samples_1m (project_id, bucket DESC);
CREATE INDEX resource_samples_1m_container ON resource_samples_1m (container_id, bucket DESC);

-- 6. Resource sample rollups (5m and 1h resolutions, kept >= 7 days)
CREATE TABLE resource_samples_rollup (
    id                  BIGSERIAL PRIMARY KEY,
    resolution          TEXT NOT NULL,
    project_id          UUID REFERENCES projects(id) ON DELETE CASCADE,
    service_id          UUID REFERENCES services(id) ON DELETE CASCADE,
    service_slug        TEXT NOT NULL,
    container_id        TEXT NOT NULL,
    bucket              TIMESTAMPTZ NOT NULL,
    cpu_percent         DOUBLE PRECISION NOT NULL,
    memory_percent      DOUBLE PRECISION NOT NULL,
    memory_bytes        BIGINT NOT NULL,
    memory_limit_bytes  BIGINT NOT NULL,
    CONSTRAINT uq_resource_samples_rollup UNIQUE (resolution, container_id, bucket)
);

CREATE INDEX resource_samples_rollup_lookup ON resource_samples_rollup (project_id, resolution, bucket DESC);

-- 7. Hourly log counts by source and level (for error tiles over 90 days without raw scan)
CREATE TABLE log_counts_1h (
    id           BIGSERIAL PRIMARY KEY,
    bucket_start TIMESTAMPTZ NOT NULL,
    org_id       UUID REFERENCES organizations(id) ON DELETE CASCADE,
    project_id   UUID REFERENCES projects(id) ON DELETE CASCADE,
    service_id   UUID REFERENCES services(id) ON DELETE CASCADE,
    source       TEXT NOT NULL,
    level        SMALLINT NOT NULL,
    count        BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX log_counts_1h_bucket ON log_counts_1h (bucket_start DESC);
CREATE INDEX log_counts_1h_org ON log_counts_1h (org_id, bucket_start DESC);
CREATE INDEX log_counts_1h_proj ON log_counts_1h (project_id, bucket_start DESC);
CREATE INDEX log_counts_1h_svc ON log_counts_1h (service_id, bucket_start DESC);
