-- Shipyard PaaS — Full Database Schema
-- Milestone 1.1: All tables, enums, and indexes

-- ════════════════════════════════════════════════════════════════
-- Custom ENUM types
-- ════════════════════════════════════════════════════════════════

CREATE TYPE member_role AS ENUM ('owner', 'admin', 'member', 'viewer');
CREATE TYPE service_type AS ENUM ('git', 'docker', 'docker_compose', 'manual', 'static', 'database');
CREATE TYPE container_status AS ENUM ('pending', 'preparing', 'running', 'complete', 'failed', 'shutdown', 'rejected', 'orphan');
CREATE TYPE deployment_status AS ENUM ('pending', 'running', 'success', 'failed', 'cancelled');
CREATE TYPE step_status AS ENUM ('pending', 'running', 'success', 'failed', 'skipped');
CREATE TYPE log_level AS ENUM ('debug', 'info', 'warn', 'error');

-- ════════════════════════════════════════════════════════════════
-- Auth & Org
-- ════════════════════════════════════════════════════════════════

CREATE TABLE users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email       TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE organizations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    slug        TEXT NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE org_members (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role        member_role NOT NULL DEFAULT 'member',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id, user_id)
);

CREATE TABLE invitations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    email       TEXT NOT NULL,
    role        member_role NOT NULL DEFAULT 'member',
    token       TEXT NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ════════════════════════════════════════════════════════════════
-- Projects
-- ════════════════════════════════════════════════════════════════

CREATE TABLE projects (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    slug            TEXT NOT NULL,
    directory_path  TEXT NOT NULL,
    node_positions  JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id, slug)
);

-- ════════════════════════════════════════════════════════════════
-- Services / Apps
-- ════════════════════════════════════════════════════════════════

CREATE TABLE services (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    slug            TEXT NOT NULL,
    type            service_type NOT NULL DEFAULT 'docker',
    directory_path  TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'stopped',
    replicas        INT NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, slug)
);

CREATE TABLE service_envs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service_id      UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    key             TEXT NOT NULL,
    value_encrypted TEXT NOT NULL DEFAULT '',
    is_secret       BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (service_id, key)
);

CREATE TABLE volumes (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service_id  UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    mount_path  TEXT NOT NULL,
    driver      TEXT NOT NULL DEFAULT 'local',
    size_mb     INT NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE networks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    driver      TEXT NOT NULL DEFAULT 'overlay',
    subnet      TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, name)
);

CREATE TABLE service_networks (
    service_id  UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    network_id  UUID NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    PRIMARY KEY (service_id, network_id)
);

CREATE TABLE domains (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service_id          UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    hostname            TEXT NOT NULL UNIQUE,
    tls_enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    traefik_router_name TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ════════════════════════════════════════════════════════════════
-- Containers (replicas) — reconciled from Docker events
-- ════════════════════════════════════════════════════════════════

CREATE TABLE containers (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service_id          UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    docker_container_id TEXT NOT NULL UNIQUE,
    docker_task_id      TEXT,
    node_id             TEXT,
    replica_index       INT,
    status              container_status NOT NULL DEFAULT 'pending',
    status_message      TEXT,
    image               TEXT NOT NULL,
    started_at          TIMESTAMPTZ,
    finished_at         TIMESTAMPTZ,
    exit_code           INT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ════════════════════════════════════════════════════════════════
-- Docker event audit log
-- ════════════════════════════════════════════════════════════════

CREATE TABLE docker_events (
    id                  BIGSERIAL PRIMARY KEY,
    event_type          TEXT NOT NULL,
    action              TEXT NOT NULL,
    actor_id            TEXT NOT NULL,
    actor_attributes    JSONB,
    scope               TEXT,
    raw                 JSONB NOT NULL,
    received_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ════════════════════════════════════════════════════════════════
-- Deployments
-- ════════════════════════════════════════════════════════════════

CREATE TABLE deployments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service_id      UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    triggered_by    TEXT NOT NULL DEFAULT 'manual',
    source_ref      TEXT NOT NULL DEFAULT '',
    status          deployment_status NOT NULL DEFAULT 'pending',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at     TIMESTAMPTZ
);

CREATE TABLE deployment_steps (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    deployment_id   UUID NOT NULL REFERENCES deployments(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    status          step_status NOT NULL DEFAULT 'pending',
    order_index     INT NOT NULL DEFAULT 0,
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ
);

CREATE TABLE deployment_logs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    deployment_id   UUID NOT NULL REFERENCES deployments(id) ON DELETE CASCADE,
    step_id         UUID REFERENCES deployment_steps(id) ON DELETE SET NULL,
    level           log_level NOT NULL DEFAULT 'info',
    message         TEXT NOT NULL,
    timestamp       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ════════════════════════════════════════════════════════════════
-- Topology (computed / cached edges)
-- ════════════════════════════════════════════════════════════════

CREATE TABLE topology_edges (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    source_node_id  TEXT NOT NULL,
    target_node_id  TEXT NOT NULL,
    edge_type       TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ════════════════════════════════════════════════════════════════
-- System config (setup wizard state)
-- ════════════════════════════════════════════════════════════════

CREATE TABLE system_config (
    key         TEXT PRIMARY KEY,
    value       JSONB NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ════════════════════════════════════════════════════════════════
-- Indexes
-- ════════════════════════════════════════════════════════════════

CREATE INDEX idx_org_members_org_id ON org_members(org_id);
CREATE INDEX idx_org_members_user_id ON org_members(user_id);
CREATE INDEX idx_projects_org_id ON projects(org_id);
CREATE INDEX idx_services_project_id ON services(project_id);
CREATE INDEX idx_service_envs_service_id ON service_envs(service_id);
CREATE INDEX idx_volumes_service_id ON volumes(service_id);
CREATE INDEX idx_networks_project_id ON networks(project_id);
CREATE INDEX idx_domains_service_id ON domains(service_id);
CREATE INDEX idx_containers_service_id ON containers(service_id);
CREATE INDEX idx_containers_docker_container_id ON containers(docker_container_id);
CREATE INDEX idx_docker_events_actor_id ON docker_events(actor_id);
CREATE INDEX idx_docker_events_received_at ON docker_events(received_at);
CREATE INDEX idx_deployments_service_id ON deployments(service_id);
CREATE INDEX idx_deployment_steps_deployment_id ON deployment_steps(deployment_id);
CREATE INDEX idx_deployment_logs_deployment_id ON deployment_logs(deployment_id);
CREATE INDEX idx_deployment_logs_step_id ON deployment_logs(step_id);
CREATE INDEX idx_topology_edges_project_id ON topology_edges(project_id);
CREATE INDEX idx_invitations_org_id ON invitations(org_id);
CREATE INDEX idx_invitations_token ON invitations(token);

-- ════════════════════════════════════════════════════════════════
-- Updated_at trigger function
-- ════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER set_projects_updated_at
    BEFORE UPDATE ON projects
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER set_services_updated_at
    BEFORE UPDATE ON services
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER set_containers_updated_at
    BEFORE UPDATE ON containers
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
