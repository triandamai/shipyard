-- Sandbox runtime: in-editor apps get their own service_type, a static-config
-- specialization table, and a live-runtime-state table (mirrors the
-- services/containers split — config vs. frequently-mutated live state).

ALTER TYPE service_type ADD VALUE IF NOT EXISTS 'sandbox_app';

CREATE TABLE sandbox_app_configs (
    service_id      UUID PRIMARY KEY REFERENCES services(id) ON DELETE CASCADE,
    runtime         TEXT,
    base_image      TEXT,
    install_cmd     TEXT,
    dev_cmd         TEXT,
    port            INT,
    manifest_source TEXT NOT NULL DEFAULT 'undetected'
                    CHECK (manifest_source IN ('undetected', 'detected', 'manifest')),
    volume_name     TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE sandbox_instances (
    service_id       UUID PRIMARY KEY REFERENCES services(id) ON DELETE CASCADE,
    status           TEXT NOT NULL DEFAULT 'stopped'
                     CHECK (status IN ('stopped', 'starting', 'running')),
    container_id     TEXT,
    container_name   TEXT,
    preview_url      TEXT,
    last_heartbeat_at TIMESTAMPTZ,
    started_at       TIMESTAMPTZ,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sandbox_instances_status_heartbeat
    ON sandbox_instances (status, last_heartbeat_at);
