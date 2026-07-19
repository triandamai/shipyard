-- Rename memory_gb → memory_mb on plans and org_quota.
-- Existing values are scaled ×1024 so stored precision stays in MB
-- (e.g. 2 GB → 2048 MB, 8 GB → 8192 MB).

ALTER TABLE plans
    ADD COLUMN memory_mb INT NOT NULL DEFAULT 4096;

UPDATE plans SET memory_mb = memory_gb * 1024;

ALTER TABLE plans
    DROP COLUMN memory_gb;

ALTER TABLE org_quota
    ADD COLUMN memory_mb INT NOT NULL DEFAULT 2048;

UPDATE org_quota SET memory_mb = memory_gb * 1024;

ALTER TABLE org_quota
    DROP COLUMN memory_gb;
