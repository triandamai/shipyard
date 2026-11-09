-- Migration: 20250101000065_orphans.sql
-- Table to store detected orphan resources (directories, orgs, projects, services, volumes)
-- discovered by the background worker so administrators can inspect and decide on deletion.

CREATE TABLE IF NOT EXISTS orphans (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    orphan_type VARCHAR(32) NOT NULL, -- 'org', 'project', 'service', 'volume', 'directory'
    identifier VARCHAR(255) NOT NULL,
    path TEXT NOT NULL UNIQUE,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    details TEXT,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    auto_destruct_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '30 days'),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_orphans_type ON orphans(orphan_type);
CREATE INDEX IF NOT EXISTS idx_orphans_detected ON orphans(detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_orphans_auto_destruct ON orphans(auto_destruct_at);
