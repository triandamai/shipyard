-- Generic Static Artifact Deployments
-- Independent of Docker services, supporting immutable deployments, unique preview URLs, and atomic aliases.

CREATE TABLE IF NOT EXISTS static_projects (
    id               UUID PRIMARY KEY,
    org_id           UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    slug             TEXT NOT NULL,
    framework        TEXT NOT NULL DEFAULT 'custom',
    spa_fallback     BOOLEAN NOT NULL DEFAULT FALSE,
    preview_base_url TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT static_projects_org_slug_unique UNIQUE (org_id, slug)
);

CREATE TABLE IF NOT EXISTS static_deployments (
    id                      UUID PRIMARY KEY,
    project_id              UUID NOT NULL REFERENCES static_projects(id) ON DELETE CASCADE,
    status                  TEXT NOT NULL DEFAULT 'PENDING',
    archive_checksum_sha256 TEXT,
    archive_size_bytes      BIGINT,
    extracted_size_bytes    BIGINT,
    file_count              INTEGER,
    entry_file              TEXT NOT NULL DEFAULT 'index.html',
    storage_path            TEXT NOT NULL,
    preview_token           TEXT,
    is_private              BOOLEAN NOT NULL DEFAULT FALSE,
    error_message           TEXT,
    metadata                JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS static_aliases (
    id            UUID PRIMARY KEY,
    project_id    UUID NOT NULL REFERENCES static_projects(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    deployment_id UUID NOT NULL REFERENCES static_deployments(id) ON DELETE CASCADE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT static_aliases_project_name_unique UNIQUE (project_id, name)
);

CREATE TABLE IF NOT EXISTS static_deployment_events (
    id            UUID PRIMARY KEY,
    deployment_id UUID NOT NULL REFERENCES static_deployments(id) ON DELETE CASCADE,
    status        TEXT NOT NULL,
    message       TEXT NOT NULL,
    details       JSONB,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_static_projects_org ON static_projects(org_id);
CREATE INDEX IF NOT EXISTS idx_static_deployments_project ON static_deployments(project_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_static_deployments_status ON static_deployments(status);
CREATE INDEX IF NOT EXISTS idx_static_aliases_project ON static_aliases(project_id);
CREATE INDEX IF NOT EXISTS idx_static_deployment_events_dep ON static_deployment_events(deployment_id, created_at ASC);
