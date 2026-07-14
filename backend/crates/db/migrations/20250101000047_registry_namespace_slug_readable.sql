-- Migrate registry namespace slugs from UUID/UUID format to org-slug/project-slug.
-- After this, OCI paths become: registry.example.com/<org-slug>/<project-slug>/<image>:<tag>

-- Project-scoped namespaces: org_slug/project_slug
UPDATE registry_namespaces
SET slug = o.slug || '/' || p.slug
FROM organizations o, projects p
WHERE registry_namespaces.project_id IS NOT NULL
  AND registry_namespaces.org_id     = o.id
  AND registry_namespaces.project_id = p.id
  AND p.org_id = o.id;

-- Org-level namespaces (project_id IS NULL): just org_slug
UPDATE registry_namespaces
SET slug = o.slug
FROM organizations o
WHERE registry_namespaces.project_id IS NULL
  AND registry_namespaces.org_id = o.id;

-- Enforce globally unique slugs (org_slug is already globally unique,
-- and org_slug/project_slug is unique because project slugs are unique per org).
ALTER TABLE registry_namespaces
    ADD CONSTRAINT uq_registry_namespaces_slug UNIQUE (slug);
