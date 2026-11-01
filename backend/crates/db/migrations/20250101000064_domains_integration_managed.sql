-- Marks the hostname the integration API manages for an environment (its
-- platform address). Lets Nomi send any full hostname, not only
-- `<label>.<apps_base_domain>`, and still find it again on the next publish,
-- check or change. Rows from before this flag are still found by the base
-- domain suffix.
ALTER TABLE domains ADD COLUMN IF NOT EXISTS integration_managed BOOLEAN NOT NULL DEFAULT FALSE;
