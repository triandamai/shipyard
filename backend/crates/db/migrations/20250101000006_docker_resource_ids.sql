-- Track the Docker-assigned network ID so the event worker can find the DB record
-- when Docker fires a network destroy event (actor.id = Docker network ID, not our UUID).
ALTER TABLE networks ADD COLUMN IF NOT EXISTS docker_network_id TEXT;
