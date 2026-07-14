-- Registry configuration (storage backend, hostname, S3 credentials) is loaded
-- from the config file / environment variables into RegistryConfig at startup.
-- No DB schema changes are required here; system_config keys can be used later
-- if the admin UI needs to persist overrides.
DO $$ BEGIN NULL; END $$;
