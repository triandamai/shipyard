-- Traffic and resource historical metrics for Nomi Monitor sheet (7+ days retention).

CREATE TABLE environment_traffic_samples (
    id           BIGSERIAL PRIMARY KEY,
    project_id   UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    recorded_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    visitor_hash TEXT NOT NULL,
    status_code  SMALLINT NOT NULL,
    duration_ms  INTEGER NOT NULL
);

CREATE INDEX environment_traffic_samples_proj_time
    ON environment_traffic_samples (project_id, recorded_at);

CREATE TABLE container_resource_samples (
    id                  BIGSERIAL PRIMARY KEY,
    project_id          UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    service_slug        TEXT NOT NULL,
    container_id        TEXT NOT NULL,
    cpu_percent         DOUBLE PRECISION NOT NULL,
    memory_percent      DOUBLE PRECISION NOT NULL,
    memory_bytes        BIGINT NOT NULL,
    memory_limit_bytes  BIGINT NOT NULL,
    recorded_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX container_resource_samples_proj_time
    ON container_resource_samples (project_id, recorded_at);

CREATE TABLE service_error_rate_alerts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    service_slug TEXT NOT NULL,
    error_rate  DOUBLE PRECISION NOT NULL,
    threshold   DOUBLE PRECISION NOT NULL,
    fired_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX service_error_rate_alerts_proj_time
    ON service_error_rate_alerts (project_id, fired_at DESC);
