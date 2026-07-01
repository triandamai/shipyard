-- Project-level membership — stores which users have explicit access to a project
-- and what they can do.  Permissions are simple strings: "view", "deploy", "manage".
CREATE TABLE project_members (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permissions TEXT[] NOT NULL DEFAULT '{}',
    invited_by  UUID REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, user_id)
);

CREATE INDEX idx_project_members_lookup ON project_members (project_id, user_id);
CREATE INDEX idx_project_members_user   ON project_members (user_id);

-- Extend invitations to carry per-project assignments applied on accept.
-- Format: [{ "project_id": "uuid", "permissions": ["view","deploy"] }]
ALTER TABLE invitations
    ADD COLUMN project_assignments JSONB NOT NULL DEFAULT '[]'::jsonb;
