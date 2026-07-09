-- Git-based deployment strategy fields for standard services
ALTER TABLE services ADD COLUMN IF NOT EXISTS git_deploy_strategy    TEXT NOT NULL DEFAULT 'push';
ALTER TABLE services ADD COLUMN IF NOT EXISTS git_deploy_branch      TEXT;
ALTER TABLE services ADD COLUMN IF NOT EXISTS git_deploy_tag_pattern TEXT;
