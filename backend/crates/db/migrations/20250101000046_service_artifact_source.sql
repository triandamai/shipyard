-- Artifact source configuration for services that deploy from the Shipyard
-- internal registry instead of git or an external Docker registry.
-- One optional row per service; absence means the service uses git/external source.
CREATE TABLE IF NOT EXISTS service_artifact_sources (
    service_id      UUID        PRIMARY KEY REFERENCES services(id) ON DELETE CASCADE,
    namespace_id    UUID        NOT NULL REFERENCES registry_namespaces(id) ON DELETE RESTRICT,
    repo            TEXT        NOT NULL,
    tag             TEXT        NOT NULL DEFAULT 'latest',
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_sas_namespace ON service_artifact_sources(namespace_id);
