-- Static site hosting support
-- service_type already includes 'static' in the base schema.

CREATE TABLE static_site_configs (
    service_id       UUID PRIMARY KEY REFERENCES services(id) ON DELETE CASCADE,
    -- Deployment source: 'git' (build on deploy) | 'upload' (pre-built artifact)
    source           TEXT NOT NULL DEFAULT 'git',
    -- Build settings — used when source = 'git'; overridable by deploy.json
    build_command    TEXT NOT NULL DEFAULT 'npm run build',
    output_dir       TEXT NOT NULL DEFAULT 'dist',
    node_version     TEXT NOT NULL DEFAULT '20',
    install_command  TEXT NOT NULL DEFAULT 'npm ci',
    -- Framework hint: 'vite' | 'sveltekit-static' | 'nextjs-export' | 'hugo' | 'jekyll' | 'custom'
    framework        TEXT NOT NULL DEFAULT 'custom',
    -- Parsed runtime config from the last deploy.json (spa, redirects, headers, etc.)
    deploy_config    JSONB,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
