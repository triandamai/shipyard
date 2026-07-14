-- Add git_provider_id to edge_function_groups so groups can use an OAuth/PAT
-- token when cloning the repo, matching the behaviour of services.
ALTER TABLE edge_function_groups
    ADD COLUMN IF NOT EXISTS git_provider_id UUID REFERENCES git_providers(id) ON DELETE SET NULL;
