ALTER TABLE users ADD COLUMN IF NOT EXISTS is_suspended BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS plans (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name                    TEXT NOT NULL UNIQUE,
    enabled                 BOOLEAN NOT NULL DEFAULT TRUE,
    cpu_cores               INT NOT NULL DEFAULT 2,
    memory_gb               INT NOT NULL DEFAULT 4,
    max_replicas            INT NOT NULL DEFAULT 3,
    node_count              INT NOT NULL DEFAULT 1,
    max_members             INT NOT NULL DEFAULT 5,
    max_projects            INT NOT NULL DEFAULT 10,
    max_orgs                INT NOT NULL DEFAULT 1,
    max_parallel_deployments INT NOT NULL DEFAULT 1,
    max_git_providers       INT NOT NULL DEFAULT 1,
    price_monthly           FLOAT8 NOT NULL DEFAULT 0.0,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO plans (name, enabled, cpu_cores, memory_gb, max_replicas, node_count, max_members, max_projects, max_orgs, max_parallel_deployments, max_git_providers, price_monthly)
VALUES
    ('free',  TRUE, 1, 2, 1, 1, 3, 3, 1, 1, 1, 0.00),
    ('pro',   TRUE, 4, 8, 5, 2, 10, 20, 3, 3, 3, 29.00),
    ('max',   TRUE, 8, 16, 10, 5, 50, 100, 10, -1, 10, 99.00)
ON CONFLICT (name) DO NOTHING;
