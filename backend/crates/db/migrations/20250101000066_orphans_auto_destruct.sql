-- Migration: 20250101000066_orphans_auto_destruct.sql
-- Adds auto_destruct_at to orphans table so unmanaged orphans are permanently deleted after 30 days.

ALTER TABLE orphans ADD COLUMN IF NOT EXISTS auto_destruct_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '30 days');
CREATE INDEX IF NOT EXISTS idx_orphans_auto_destruct ON orphans(auto_destruct_at);
