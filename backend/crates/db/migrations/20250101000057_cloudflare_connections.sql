-- Per-org Cloudflare API token connection, mirroring git_providers'
-- shape (plaintext token column, one row per org).
CREATE TABLE IF NOT EXISTS cloudflare_connections (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    api_token    TEXT NOT NULL,
    account_id   TEXT NOT NULL,
    account_name TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id)
);

-- Nullable: a domain not covered by any connected Cloudflare zone (or
-- whose org never connected Cloudflare at all) simply has both NULL,
-- and behaves exactly as it did before this feature existed.
ALTER TABLE domains ADD COLUMN IF NOT EXISTS cloudflare_zone_id TEXT;
ALTER TABLE domains ADD COLUMN IF NOT EXISTS cloudflare_record_id TEXT;
