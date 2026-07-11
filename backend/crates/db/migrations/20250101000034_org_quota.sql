-- Snapshot of plan limits at the time an org subscribes.
-- This row is written once (on org creation or plan purchase) and is
-- intentionally NOT updated when the plan definition changes later —
-- existing orgs keep the limits they signed up for.
CREATE TABLE IF NOT EXISTS org_quota (
    org_id                   UUID PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
    -- Which plan snapshot this came from (informational only — do NOT join
    -- to plans for limit checks; use the columns below instead).
    plan_id                  UUID REFERENCES plans(id) ON DELETE SET NULL,
    -- Limits frozen from the plan at subscription time
    max_projects             INT NOT NULL DEFAULT 3,
    max_members              INT NOT NULL DEFAULT 5,
    max_replicas             INT NOT NULL DEFAULT 1,
    max_parallel_deployments INT NOT NULL DEFAULT 1,
    max_git_providers        INT NOT NULL DEFAULT 1,
    max_orgs                 INT NOT NULL DEFAULT 1,
    node_count               INT NOT NULL DEFAULT 1,
    cpu_cores                INT NOT NULL DEFAULT 1,
    memory_gb                INT NOT NULL DEFAULT 2,
    applied_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Backfill existing orgs from their current plan_id (if any).
INSERT INTO org_quota (
    org_id, plan_id,
    max_projects, max_members, max_replicas, max_parallel_deployments,
    max_git_providers, max_orgs, node_count, cpu_cores, memory_gb,
    applied_at, updated_at
)
SELECT
    o.id,
    p.id,
    p.max_projects, p.max_members, p.max_replicas, p.max_parallel_deployments,
    p.max_git_providers, p.max_orgs, p.node_count, p.cpu_cores, p.memory_gb,
    NOW(), NOW()
FROM organizations o
JOIN plans p ON p.id = o.plan_id
ON CONFLICT (org_id) DO NOTHING;

-- Orgs with no plan yet get free-tier defaults.
INSERT INTO org_quota (org_id, plan_id, applied_at, updated_at)
SELECT o.id, p.id, NOW(), NOW()
FROM organizations o
CROSS JOIN (SELECT id FROM plans WHERE name = 'free' LIMIT 1) p
WHERE o.id NOT IN (SELECT org_id FROM org_quota)
ON CONFLICT (org_id) DO NOTHING;
