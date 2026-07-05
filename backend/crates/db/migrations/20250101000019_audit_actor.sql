-- Add unified actor fields to audit_logs so API key actions are
-- attributed to the key (not the key's creator).
--
-- actor_type: 'user' | 'api_key'
-- actor_id:   user.id or api_key.id
-- actor_name: user email or api key name (display only, denormalised)

ALTER TABLE audit_logs
    ADD COLUMN actor_type TEXT    NOT NULL DEFAULT 'user',
    ADD COLUMN actor_id   UUID,
    ADD COLUMN actor_name TEXT;

-- Back-fill from existing user_id column
UPDATE audit_logs
SET actor_id   = user_id,
    actor_name = (SELECT email FROM users WHERE id = user_id),
    actor_type = 'user'
WHERE user_id IS NOT NULL;

CREATE INDEX idx_audit_logs_actor ON audit_logs (actor_type, actor_id);
