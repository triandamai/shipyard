-- Platform integrations (first: Nomi). An integration holds a platform-level
-- `ship_int_` key and manages the orgs it created through the Open API.

CREATE TABLE integrations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Keys are stored hashed. `scopes`: 'manage' (orgs, apps, deploys, webhooks)
-- and 'secrets:reveal' (read secret values). Give the key used by agent
-- tooling 'manage' only, so secret values can never reach a model.
CREATE TABLE integration_keys (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    integration_id  UUID NOT NULL REFERENCES integrations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    key_prefix      TEXT NOT NULL,
    key_hash        TEXT NOT NULL UNIQUE,
    scopes          TEXT[] NOT NULL DEFAULT ARRAY['manage'],
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ,
    revoked_at      TIMESTAMPTZ
);

ALTER TABLE organizations
    ADD COLUMN integration_id UUID REFERENCES integrations(id) ON DELETE SET NULL,
    ADD COLUMN external_id    TEXT;
CREATE UNIQUE INDEX organizations_integration_external
    ON organizations (integration_id, external_id)
    WHERE integration_id IS NOT NULL;

-- An app groups one project per environment (production, staging, preview-x).
CREATE TABLE integration_apps (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    app_key     TEXT NOT NULL,
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id, app_key)
);

ALTER TABLE projects
    ADD COLUMN app_id            UUID REFERENCES integration_apps(id) ON DELETE SET NULL,
    ADD COLUMN environment       TEXT,
    ADD COLUMN duplicated_from   UUID REFERENCES projects(id) ON DELETE SET NULL;
CREATE UNIQUE INDEX projects_app_environment
    ON projects (app_id, environment)
    WHERE app_id IS NOT NULL;

-- Secret store. Values are AES-256-GCM encrypted with the platform secret key
-- and only ever returned by the reveal endpoint (scope 'secrets:reveal').
-- Everything else refers to a secret by `ref` (sec_<32 hex>).
CREATE TABLE secrets (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ref             TEXT NOT NULL UNIQUE,
    org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    app_id          UUID REFERENCES integration_apps(id) ON DELETE CASCADE,
    project_id      UUID REFERENCES projects(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    -- 'generated' | 'user' | 'dev_db_app' | 'dev_db_owner' | 'database'
    kind            TEXT NOT NULL,
    value_encrypted TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_revealed_at TIMESTAMPTZ
);
CREATE UNIQUE INDEX secrets_scope_name
    ON secrets (org_id, COALESCE(app_id, '00000000-0000-0000-0000-000000000000'::uuid),
                COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid), name);

-- Which service env vars take their value from a secret, so changing the
-- secret updates every service that uses it.
CREATE TABLE secret_bindings (
    service_id  UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    env_key     TEXT NOT NULL,
    secret_id   UUID NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
    PRIMARY KEY (service_id, env_key)
);

-- Databases on the shared development Postgres server.
CREATE TABLE dev_databases (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id           UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    app_id           UUID NOT NULL UNIQUE REFERENCES integration_apps(id) ON DELETE CASCADE,
    db_name          TEXT NOT NULL UNIQUE,
    app_role         TEXT NOT NULL,
    owner_role       TEXT NOT NULL,
    app_secret_id    UUID REFERENCES secrets(id) ON DELETE SET NULL,
    owner_secret_id  UUID REFERENCES secrets(id) ON DELETE SET NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Outbound webhooks. An endpoint belongs to an integration (receives events
-- for every org it manages) or to a single org.
CREATE TABLE webhook_endpoints (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    integration_id   UUID REFERENCES integrations(id) ON DELETE CASCADE,
    org_id           UUID REFERENCES organizations(id) ON DELETE CASCADE,
    url              TEXT NOT NULL,
    secret_encrypted TEXT NOT NULL,
    -- Event type patterns: exact ('deployment.succeeded'), prefix ('deployment.*') or '*'.
    events           TEXT[] NOT NULL DEFAULT ARRAY['*'],
    enabled          BOOLEAN NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (integration_id IS NOT NULL OR org_id IS NOT NULL)
);

-- Every event is recorded, whether or not anyone subscribes, so integrators
-- can also page through them (GET /events). No foreign keys: events about a
-- deleted org or project (org.deleted, app.terminated) must still be
-- delivered. `org_external_id` is kept for the same reason. Old events are
-- pruned by the webhook dispatcher.
CREATE TABLE platform_events (
    id          UUID PRIMARY KEY,
    org_id      UUID NOT NULL,
    org_external_id TEXT,
    project_id  UUID,
    service_id  UUID,
    type        TEXT NOT NULL,
    data        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX platform_events_org_created ON platform_events (org_id, created_at DESC);

CREATE TABLE webhook_deliveries (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    endpoint_id      UUID NOT NULL REFERENCES webhook_endpoints(id) ON DELETE CASCADE,
    event_id         UUID NOT NULL REFERENCES platform_events(id) ON DELETE CASCADE,
    -- 'pending' | 'delivered' | 'failed' (gave up)
    status           TEXT NOT NULL DEFAULT 'pending',
    attempts         INT NOT NULL DEFAULT 0,
    next_attempt_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_status_code INT,
    last_error       TEXT,
    delivered_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX webhook_deliveries_due ON webhook_deliveries (next_attempt_at) WHERE status = 'pending';

-- One row per publish of an app environment: the manifest it applied, the
-- deployments it ran, and the outcome. Duplicating an environment replays
-- the latest successful manifest.
CREATE TABLE publishes (
    id           UUID PRIMARY KEY,
    org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id   UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- 'running' | 'succeeded' | 'failed'
    status       TEXT NOT NULL DEFAULT 'running',
    manifest     JSONB NOT NULL,
    deployments  JSONB NOT NULL DEFAULT '[]'::jsonb,
    urls         TEXT[] NOT NULL DEFAULT '{}',
    error        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at  TIMESTAMPTZ
);
CREATE INDEX publishes_project ON publishes (project_id, created_at DESC);
