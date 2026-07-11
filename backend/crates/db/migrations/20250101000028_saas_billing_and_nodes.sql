-- 1. Subscription Tiers Enum
CREATE TYPE subscription_tier AS ENUM ('free', 'pro', 'max');

-- 2. Provisioning State Machine Enum
CREATE TYPE node_status AS ENUM (
    'provisioning',
    'cloud_init_running',
    'wireguard_joined',
    'active',
    'degraded',
    'failed',
    'stopped'
);

-- 3. Organization Billing Info
CREATE TABLE org_billing (
    org_id             UUID PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
    stripe_customer_id TEXT UNIQUE,
    stripe_sub_id      TEXT UNIQUE,
    tier               subscription_tier NOT NULL DEFAULT 'free',
    sub_status         TEXT NOT NULL DEFAULT 'active',
    current_period_end TIMESTAMPTZ,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 4. Compute Nodes
CREATE TABLE compute_nodes (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id                UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name                  TEXT NOT NULL,
    provider              TEXT NOT NULL,
    provider_vm_id        TEXT,
    region                TEXT NOT NULL,
    ip_address            TEXT,
    public_ip             TEXT,
    status                node_status NOT NULL DEFAULT 'provisioning',
    last_heartbeat_at     TIMESTAMPTZ,
    cpu_cores             INT NOT NULL DEFAULT 1,
    ram_mb                INT NOT NULL DEFAULT 1024,
    tls_ca_cert           TEXT,
    tls_client_cert       TEXT,
    tls_client_key        TEXT,
    provision_error       TEXT,
    provision_attempts    INT NOT NULL DEFAULT 0,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_compute_nodes_org    ON compute_nodes(org_id);
CREATE INDEX idx_compute_nodes_status ON compute_nodes(status);

-- 5. Service → Node Assignment
CREATE TABLE service_node_assignments (
    service_id  UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    node_id     UUID NOT NULL REFERENCES compute_nodes(id) ON DELETE RESTRICT,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (service_id)
);

CREATE INDEX idx_sna_node ON service_node_assignments(node_id);
