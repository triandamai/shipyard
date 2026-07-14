-- Artifactory: external registry credentials at org level.
-- password_enc is AES-GCM encrypted at rest.
-- Used by the deploy engine to pull images from Docker Hub, ECR, GCR, etc.

CREATE TABLE IF NOT EXISTS external_registries (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       UUID        NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name         TEXT        NOT NULL,
    registry_url TEXT        NOT NULL,
    username     TEXT,
    password_enc TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(org_id, registry_url)
);

CREATE INDEX IF NOT EXISTS idx_external_registries_org ON external_registries(org_id);
