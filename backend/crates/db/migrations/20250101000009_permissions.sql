-- Fine-grained permission assignments for org members.
-- Permissions are stored as strings (e.g. "app:project:service:read")
-- and are additive on top of the coarse member_role.

CREATE TABLE org_member_permissions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission  TEXT NOT NULL,
    granted_by  UUID REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id, user_id, permission)
);

CREATE INDEX idx_org_member_permissions_lookup
    ON org_member_permissions (org_id, user_id);

-- Invitations can carry initial permission grants applied on accept.
ALTER TABLE invitations
    ADD COLUMN permissions TEXT[] NOT NULL DEFAULT '{}';
