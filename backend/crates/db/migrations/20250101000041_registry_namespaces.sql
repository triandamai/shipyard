-- Artifactory: registry namespaces.
-- project_id NULL  → org-level namespace (shared across all projects in the org)
-- project_id SET   → project-level namespace (isolated per project)
-- Build cache namespaces are always project-level.

CREATE TABLE IF NOT EXISTS registry_namespaces (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID        NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id  UUID        REFERENCES projects(id) ON DELETE CASCADE,
    slug        TEXT        NOT NULL,
    is_public   BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(org_id, project_id)
);

CREATE INDEX IF NOT EXISTS idx_registry_namespaces_org    ON registry_namespaces(org_id);
CREATE INDEX IF NOT EXISTS idx_registry_namespaces_project ON registry_namespaces(project_id);
