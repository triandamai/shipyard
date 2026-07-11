CREATE TABLE IF NOT EXISTS payments (
    id                        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id                    UUID REFERENCES organizations(id) ON DELETE SET NULL,
    stripe_payment_intent_id  TEXT UNIQUE,
    amount                    INT NOT NULL DEFAULT 0,
    currency                  TEXT NOT NULL DEFAULT 'usd',
    status                    TEXT NOT NULL DEFAULT 'pending',
    description               TEXT,
    created_at                TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_payments_org_id    ON payments(org_id);
CREATE INDEX IF NOT EXISTS idx_payments_status     ON payments(status);
CREATE INDEX IF NOT EXISTS idx_payments_created_at ON payments(created_at DESC);
