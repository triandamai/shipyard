-- Migrate all permission strings to the new hierarchical shipyard: format.
--
-- Old format (org-level):   "app:org:settings:read"
-- New format (org-level):   "shipyard:<org_id>:settings:read"
--
-- Old format (project-level shorthand): ["view", "deploy", "manage"]
-- New format (project-level expanded):  ["shipyard:<org_id>:<project_id>:project:view", ...]
--
-- The expansion rules for project shorthand are:
--   view   → project:view + service:view
--   deploy → project:view + service:view + service:deploy
--   manage → project:view + project:manage + service:view + service:write +
--             service:deploy + service:delete + env:read + env:write +
--             domain:write + volume:write + network:write

-- ── Step 1: org_member_permissions ──────────────────────────────────────────
-- Translate app:* strings to shipyard:<org_id>:* format.
UPDATE org_member_permissions omp
SET permission = 'shipyard:' || omp.org_id::text || ':' ||
    CASE omp.permission
        WHEN 'app:org:settings:read'    THEN 'settings:read'
        WHEN 'app:org:settings:write'   THEN 'settings:write'
        WHEN 'app:org:members:read'     THEN 'members:read'
        WHEN 'app:org:members:invite'   THEN 'members:invite'
        WHEN 'app:org:members:manage'   THEN 'members:manage'
        WHEN 'app:project:read'         THEN 'projects:read'
        WHEN 'app:project:write'        THEN 'projects:write'
        WHEN 'app:project:delete'       THEN 'projects:write'
        -- Catch-all: strip old prefix, keep remainder (safe fallback for unknown strings)
        ELSE REGEXP_REPLACE(omp.permission, '^app:', '')
    END
WHERE omp.permission LIKE 'app:%';

-- ── Step 2: project_members ─────────────────────────────────────────────────
-- Expand shorthand permissions to full shipyard: strings.
-- Uses a subquery that unnests the old array, maps each shorthand token to its
-- expanded set, then re-aggregates to a deduplicated array.

UPDATE project_members pm
SET permissions = (
    SELECT ARRAY(
        SELECT DISTINCT unnest(expanded)
        ORDER BY 1
    )
    FROM (
        SELECT ARRAY_AGG(perm) AS expanded
        FROM (
            -- For every shorthand token, emit the corresponding full string(s)
            SELECT unnest(
                CASE token
                    WHEN 'view' THEN ARRAY[
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':project:view',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':service:view'
                    ]
                    WHEN 'deploy' THEN ARRAY[
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':project:view',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':service:view',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':service:deploy'
                    ]
                    WHEN 'manage' THEN ARRAY[
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':project:view',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':project:manage',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':service:view',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':service:write',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':service:deploy',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':service:delete',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':env:read',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':env:write',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':domain:write',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':volume:write',
                        'shipyard:' || p.org_id::text || ':' || pm.project_id::text || ':network:write'
                    ]
                    -- Already-migrated strings pass through unchanged
                    ELSE ARRAY[token]
                END
            ) AS perm
            FROM UNNEST(pm.permissions) AS token
            JOIN projects p ON p.id = pm.project_id
        ) expanded_tokens
    ) agg
)
WHERE array_length(pm.permissions, 1) > 0;
