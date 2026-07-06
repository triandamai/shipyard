-- Stores resolved platform-{slug} env var references so the topology endpoint
-- can emit env_ref edges (same-project) and portal nodes (cross-project)
-- without re-scanning env values on every topology fetch.

CREATE TABLE service_env_refs (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    service_id          UUID        NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    env_key             TEXT        NOT NULL,
    ref_value           TEXT        NOT NULL,   -- e.g. "platform-my-db"
    resource_type       TEXT        NOT NULL CHECK (resource_type IN ('service','network','volume')),
    resource_id         UUID        NOT NULL,
    resource_project_id UUID        NOT NULL,   -- not FK — resource may live in another project
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (service_id, ref_value)
);

CREATE INDEX idx_service_env_refs_service ON service_env_refs(service_id);
