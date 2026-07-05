-- Add fine-grained shipyard: permission strings to API keys.
-- Replaces the coarse scopes (read/deploy/write/admin) with the same
-- permission format used by org members.

ALTER TABLE api_keys
    ADD COLUMN permissions TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN description  TEXT;

-- Back-fill permissions from existing coarse scopes.
-- read   → projects:read
-- deploy → projects:read (deploy is project-scoped; no org-wide deploy perm exists yet)
-- write  → projects:read + projects:write + settings:read
-- admin  → all org-level permissions
UPDATE api_keys
SET permissions = (
    SELECT ARRAY(
        SELECT DISTINCT unnest(p) ORDER BY 1
    )
    FROM (
        SELECT
            (CASE WHEN 'read' = ANY(scopes) THEN
                ARRAY['shipyard:' || org_id::text || ':projects:read']
             ELSE ARRAY[]::TEXT[] END) ||
            (CASE WHEN 'deploy' = ANY(scopes) THEN
                ARRAY['shipyard:' || org_id::text || ':projects:read']
             ELSE ARRAY[]::TEXT[] END) ||
            (CASE WHEN 'write' = ANY(scopes) THEN
                ARRAY[
                    'shipyard:' || org_id::text || ':projects:read',
                    'shipyard:' || org_id::text || ':projects:write',
                    'shipyard:' || org_id::text || ':settings:read'
                ]
             ELSE ARRAY[]::TEXT[] END) ||
            (CASE WHEN 'admin' = ANY(scopes) THEN
                ARRAY[
                    'shipyard:' || org_id::text || ':settings:read',
                    'shipyard:' || org_id::text || ':settings:write',
                    'shipyard:' || org_id::text || ':members:read',
                    'shipyard:' || org_id::text || ':members:manage',
                    'shipyard:' || org_id::text || ':projects:read',
                    'shipyard:' || org_id::text || ':projects:write',
                    'shipyard:' || org_id::text || ':audit:read',
                    'shipyard:' || org_id::text || ':keys:read',
                    'shipyard:' || org_id::text || ':keys:write'
                ]
             ELSE ARRAY[]::TEXT[] END) AS p
    ) t
);
