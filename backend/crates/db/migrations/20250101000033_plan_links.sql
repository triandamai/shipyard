-- Link organizations to their active plan
ALTER TABLE organizations ADD COLUMN IF NOT EXISTS plan_id UUID REFERENCES plans(id) ON DELETE SET NULL;

-- Backfill existing orgs to the free plan
UPDATE organizations
SET plan_id = (SELECT id FROM plans WHERE name = 'free' LIMIT 1)
WHERE plan_id IS NULL;

-- Link org_billing rows to the plan being billed
ALTER TABLE org_billing ADD COLUMN IF NOT EXISTS plan_id UUID REFERENCES plans(id) ON DELETE SET NULL;

-- Backfill org_billing: match tier name to plan name
UPDATE org_billing ob
SET plan_id = p.id
FROM plans p
WHERE p.name = ob.tier::text
  AND ob.plan_id IS NULL;

-- Link each payment to the plan it was for
ALTER TABLE payments ADD COLUMN IF NOT EXISTS plan_id UUID REFERENCES plans(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_payments_plan_id ON payments(plan_id);
