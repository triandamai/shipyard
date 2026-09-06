-- Opt-in: when an image is pushed to the Shipyard registry for a service's
-- bound (namespace, repo, tag), automatically queue a deployment for that
-- service. Off by default so a push is inert unless the user asks for it.
ALTER TABLE service_artifact_sources
    ADD COLUMN IF NOT EXISTS auto_deploy_on_push BOOLEAN NOT NULL DEFAULT FALSE;
