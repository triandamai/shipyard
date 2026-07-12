-- Edge functions support
-- Adds: edge_fn_status enum, edge_function_groups, edge_functions,
--       edge_function_deployments, edge_function_invocation_logs

CREATE TYPE edge_fn_status AS ENUM (
    'inactive',
    'active',
    'deploying',
    'error'
);

-- Links a git repo+branch to an org's function set.
-- One group per repo+branch per org.
CREATE TABLE edge_function_groups (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    provider        TEXT NOT NULL,          -- github, gitlab, bitbucket
    repo_url        TEXT NOT NULL,
    branch          TEXT NOT NULL,
    webhook_secret  TEXT NOT NULL DEFAULT '',
    last_deployed_sha TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_efg_org ON edge_function_groups(org_id);

-- One row per function definition.
CREATE TABLE edge_functions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id              UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    group_id            UUID REFERENCES edge_function_groups(id) ON DELETE SET NULL,
    name                TEXT NOT NULL,      -- kebab-case slug, unique per org
    runtime             TEXT NOT NULL DEFAULT 'deno',
    file_path           TEXT NOT NULL DEFAULT '',
    env_vars            JSONB NOT NULL DEFAULT '{}',
    env_whitelist       TEXT[] NOT NULL DEFAULT '{}',
    timeout_secs        INT NOT NULL DEFAULT 10,
    status              edge_fn_status NOT NULL DEFAULT 'inactive',
    last_deployed_at    TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id, name)
);

CREATE INDEX idx_ef_org ON edge_functions(org_id);
CREATE INDEX idx_ef_status ON edge_functions(status);

-- Append-only deploy history; code snapshots stored here.
CREATE TABLE edge_function_deployments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    function_id     UUID NOT NULL REFERENCES edge_functions(id) ON DELETE CASCADE,
    commit_sha      TEXT,                   -- NULL for direct uploads
    code_bundle     TEXT NOT NULL,
    deployed_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending, live, rolled_back, error
    error           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_efd_function ON edge_function_deployments(function_id, created_at DESC);

-- Rolling 7-day invocation log.
CREATE TABLE edge_function_invocation_logs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    function_id     UUID NOT NULL REFERENCES edge_functions(id) ON DELETE CASCADE,
    request_id      TEXT NOT NULL,
    method          TEXT NOT NULL,
    path            TEXT NOT NULL,
    status_code     INT NOT NULL,
    duration_ms     INT NOT NULL,
    error           TEXT,
    logged_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_efil_function_time ON edge_function_invocation_logs(function_id, logged_at DESC);
