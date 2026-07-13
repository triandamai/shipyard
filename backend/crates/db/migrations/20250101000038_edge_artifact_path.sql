-- Edge function artifact-on-disk migration
-- Adds artifact_path so code is stored on the shared volume instead of in the DB.
-- code_bundle becomes nullable to preserve existing rows as fallback during transition.

ALTER TABLE edge_function_deployments
    ADD COLUMN IF NOT EXISTS artifact_path TEXT;

ALTER TABLE edge_function_deployments
    ALTER COLUMN code_bundle DROP NOT NULL;
