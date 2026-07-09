-- Migration: 20250101000024_multiple_git_providers.sql

-- 1. Create git_providers table
CREATE TABLE IF NOT EXISTS git_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,                  -- Display nickname, e.g. "My Personal GitHub"
    provider_type TEXT NOT NULL,         -- "github", "gitlab", "bitbucket", "gitea"
    auth_type TEXT NOT NULL,             -- "pat" or "oauth"
    token TEXT NOT NULL,                 -- Access token
    username TEXT,                       -- Resolved username on the platform (e.g. "@octocat")
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for fast listings by organization
CREATE INDEX IF NOT EXISTS idx_git_providers_org ON git_providers(org_id);

-- 2. Add git_provider_id to services
ALTER TABLE services ADD COLUMN IF NOT EXISTS git_provider_id UUID REFERENCES git_providers(id) ON DELETE SET NULL;
