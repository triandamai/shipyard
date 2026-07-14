-- Artifactory: unified artifact table.
-- One row per (namespace, repo, tag, kind). kind discriminates between
-- docker_image, static_bundle, edge_function, build_cache, etc.
-- metadata JSONB holds kind-specific fields (framework, entry_point, etc.).

CREATE TABLE IF NOT EXISTS artifacts (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    kind            TEXT        NOT NULL,
    namespace_id    UUID        NOT NULL REFERENCES registry_namespaces(id) ON DELETE CASCADE,
    repo            TEXT        NOT NULL,
    tag             TEXT        NOT NULL,
    manifest_digest TEXT        NOT NULL REFERENCES registry_manifests(digest),
    size_bytes      BIGINT      NOT NULL DEFAULT 0,
    metadata        JSONB       NOT NULL DEFAULT '{}',
    pushed_by       UUID        REFERENCES users(id) ON DELETE SET NULL,
    pushed_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(namespace_id, repo, tag, kind)
);

CREATE INDEX IF NOT EXISTS idx_artifacts_namespace ON artifacts(namespace_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_kind      ON artifacts(kind);
CREATE INDEX IF NOT EXISTS idx_artifacts_repo_tag  ON artifacts(namespace_id, repo, tag);
