-- Custom domains for Static Projects
-- Enables attaching custom domains (e.g. app.example.com) to static projects (e.g. Nomi static publish)
-- with automated Traefik routing, Let's Encrypt TLS, and Cloudflare DNS management.

CREATE TABLE IF NOT EXISTS static_project_domains (
    id                   UUID PRIMARY KEY,
    project_id           UUID NOT NULL REFERENCES static_projects(id) ON DELETE CASCADE,
    hostname             TEXT NOT NULL UNIQUE,
    alias                TEXT NOT NULL DEFAULT 'production',
    tls_enabled          BOOLEAN NOT NULL DEFAULT TRUE,
    cert_provider        TEXT NOT NULL DEFAULT 'letsencrypt',
    traefik_router_name  TEXT NOT NULL,
    cloudflare_zone_id   TEXT,
    cloudflare_record_id TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_static_project_domains_project ON static_project_domains(project_id);
CREATE INDEX IF NOT EXISTS idx_static_project_domains_hostname ON static_project_domains(hostname);
