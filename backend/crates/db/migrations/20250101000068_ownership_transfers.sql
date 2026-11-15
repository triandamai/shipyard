-- ════════════════════════════════════════════════════════════════
-- Project & Service Ownership Transfers
-- ════════════════════════════════════════════════════════════════

CREATE TABLE project_transfers (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    source_org_id   UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    target_org_id   UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'in_progress'
                    CHECK (status IN ('in_progress', 'completed', 'failed')),
    initiated_by    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    error_message   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_project_transfers_source ON project_transfers (source_org_id, status);
CREATE INDEX idx_project_transfers_target ON project_transfers (target_org_id, status);
CREATE INDEX idx_project_transfers_project ON project_transfers (project_id);

CREATE TABLE service_transfers (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service_id          UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    source_project_id   UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    source_org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    target_org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    target_project_id   UUID REFERENCES projects(id) ON DELETE SET NULL,
    status              TEXT NOT NULL DEFAULT 'pending_claim'
                        CHECK (status IN ('pending_claim', 'in_progress', 'completed', 'cancelled', 'rejected', 'failed')),
    initiated_by        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    claimed_by          UUID REFERENCES users(id) ON DELETE SET NULL,
    error_message       TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_service_transfers_source ON service_transfers (source_org_id, status);
CREATE INDEX idx_service_transfers_target ON service_transfers (target_org_id, status);
CREATE INDEX idx_service_transfers_service ON service_transfers (service_id);
