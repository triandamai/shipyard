-- Add Git deployment strategy fields to static_site_configs
ALTER TABLE static_site_configs ADD COLUMN IF NOT EXISTS git_deploy_strategy TEXT NOT NULL DEFAULT 'push';
ALTER TABLE static_site_configs ADD COLUMN IF NOT EXISTS git_deploy_branch TEXT;
ALTER TABLE static_site_configs ADD COLUMN IF NOT EXISTS git_deploy_tag_pattern TEXT;
