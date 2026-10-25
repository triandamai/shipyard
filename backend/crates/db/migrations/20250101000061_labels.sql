-- Key/value labels on orgs, projects and services. Keys under `shipyard.io/`
-- are written by Shipyard itself (integration, app, environment, version);
-- everything else comes from the integration that manages the object.

ALTER TABLE organizations ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE projects      ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE services      ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Backfill what Shipyard already knows about integration-managed objects.
UPDATE organizations o
SET labels = o.labels || jsonb_build_object('shipyard.io/integration', i.name)
FROM integrations i
WHERE o.integration_id = i.id;

UPDATE projects p
SET labels = p.labels
    || jsonb_build_object('shipyard.io/integration', i.name, 'shipyard.io/app', a.app_key)
    || CASE WHEN p.environment IS NOT NULL
            THEN jsonb_build_object('shipyard.io/environment', p.environment)
            ELSE '{}'::jsonb END
FROM integration_apps a
JOIN organizations o ON o.id = a.org_id
JOIN integrations i ON i.id = o.integration_id
WHERE p.app_id = a.id;

UPDATE services s
SET labels = s.labels || (p.labels - 'shipyard.io/version')
FROM projects p
WHERE s.project_id = p.id AND p.app_id IS NOT NULL;

-- Services already deployed carry their image tag as the version.
UPDATE services
SET labels = labels || jsonb_build_object('shipyard.io/version', substring(image from ':([^:/@]+)$'))
WHERE image NOT LIKE '%@%' AND substring(image from ':([^:/@]+)$') IS NOT NULL;
