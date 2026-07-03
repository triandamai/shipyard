-- API keys for the public Open API (shipyard-openapi crate).
-- Keys are org-scoped, long-lived, and authenticated by SHA-256 hash.

CREATE TABLE api_keys (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       UUID        NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    created_by   UUID        NOT NULL REFERENCES users(id),
    name         TEXT        NOT NULL,
    -- First 8 hex chars of the key body (after "ship_"), shown in the UI so
    -- users can identify which key is which without exposing the secret.
    key_prefix   TEXT        NOT NULL,
    -- SHA-256 hex digest of the full key string (e.g. "ship_<64-hex>").
    -- SHA-256 is appropriate here because keys are 256-bit random values —
    -- not passwords — so brute-force pre-image attacks are infeasible.
    key_hash     TEXT        NOT NULL UNIQUE,
    -- Coarse permission scopes: "read", "deploy", "write", "admin"
    scopes       TEXT[]      NOT NULL DEFAULT '{}',
    last_used_at TIMESTAMPTZ,
    expires_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_api_keys_org_id  ON api_keys(org_id);
CREATE INDEX idx_api_keys_key_hash ON api_keys(key_hash);
