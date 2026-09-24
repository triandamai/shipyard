-- Template-created sandbox apps carry a base64-encoded shell script that
-- seeds their empty volume with starter files on first boot — see
-- sandbox_runtime::templates. NULL for probe-detected apps (the normal case).
ALTER TABLE sandbox_app_configs ADD COLUMN IF NOT EXISTS seed_script_b64 TEXT;
