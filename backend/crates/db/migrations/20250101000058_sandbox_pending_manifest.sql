-- 'pending' marks a Custom-created sandbox whose stack hasn't been detected
-- yet (blank /app, idle placeholder container) — see sandbox_runtime::manager
-- provision_sandbox's redetect-on-pending branch. Transitions permanently to
-- 'detected'/'manifest' once a real project is found; never reverts.
ALTER TABLE sandbox_app_configs DROP CONSTRAINT IF EXISTS sandbox_app_configs_manifest_source_check;
ALTER TABLE sandbox_app_configs ADD CONSTRAINT sandbox_app_configs_manifest_source_check
    CHECK (manifest_source IN ('undetected', 'detected', 'manifest', 'pending'));
