-- Sandbox preview domains are upserted by (service_id, hostname) on every
-- sandbox start/stop (toggling the port between the app's real port and the
-- placeholder port) — this constraint makes that upsert well-defined.
ALTER TABLE domains ADD CONSTRAINT domains_service_hostname_unique UNIQUE (service_id, hostname);
