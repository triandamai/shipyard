-- Allow volumes to exist at the project level (not tied to a specific service).
-- service_id becomes optional; project_id is added as the owning scope.

ALTER TABLE volumes
    ADD COLUMN project_id UUID REFERENCES projects(id) ON DELETE CASCADE;

ALTER TABLE volumes
    ALTER COLUMN service_id DROP NOT NULL;

-- Back-fill project_id for existing service-owned volumes
UPDATE volumes v
SET project_id = (SELECT project_id FROM services WHERE id = v.service_id)
WHERE v.service_id IS NOT NULL;

CREATE INDEX idx_volumes_project_id ON volumes(project_id);
