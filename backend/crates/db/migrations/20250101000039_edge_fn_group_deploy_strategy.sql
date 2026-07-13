-- Edge function group deployment strategy fields.
-- auto_deploy: whether pushes to the watched branch trigger a redeploy.
-- deploy_strategy: push (branch push), tag (tag push), pull_request (PR merge).
-- deploy_tag_pattern: glob pattern for tag strategy (e.g. "v*").

ALTER TABLE edge_function_groups
    ADD COLUMN IF NOT EXISTS auto_deploy       BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS deploy_strategy   TEXT    NOT NULL DEFAULT 'push',
    ADD COLUMN IF NOT EXISTS deploy_tag_pattern TEXT;
