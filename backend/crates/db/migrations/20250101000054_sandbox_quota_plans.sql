-- Dedicated dev-sandbox quota, separate from production plan limits, so
-- editor usage can never starve a customer's live production traffic.
ALTER TABLE plans ADD COLUMN IF NOT EXISTS max_concurrent_sandboxes INT NOT NULL DEFAULT 1;
ALTER TABLE plans ADD COLUMN IF NOT EXISTS sandbox_cpu_cores DOUBLE PRECISION NOT NULL DEFAULT 0.5;
ALTER TABLE plans ADD COLUMN IF NOT EXISTS sandbox_memory_gb DOUBLE PRECISION NOT NULL DEFAULT 1.0;

UPDATE plans SET max_concurrent_sandboxes = 1, sandbox_cpu_cores = 0.5,  sandbox_memory_gb = 1.0 WHERE name = 'free';
UPDATE plans SET max_concurrent_sandboxes = 3, sandbox_cpu_cores = 1.0,  sandbox_memory_gb = 2.0 WHERE name = 'pro';
UPDATE plans SET max_concurrent_sandboxes = 5, sandbox_cpu_cores = 2.0,  sandbox_memory_gb = 4.0 WHERE name = 'max';
