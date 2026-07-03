-- Track the exact image reference (digest) used in each deployment.
-- Used by the rollback feature to redeploy the same artifact.
ALTER TABLE deployments ADD COLUMN IF NOT EXISTS deployed_image TEXT;
