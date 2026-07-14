-- Artifactory: content-addressed blob store and OCI manifest table.
-- Blobs are shared across all artifact kinds and namespaces — a blob with the
-- same digest is stored once regardless of how many manifests reference it.
-- ref_count drives GC: blobs with ref_count = 0 are eligible for deletion.

CREATE TABLE IF NOT EXISTS registry_blobs (
    digest      TEXT        PRIMARY KEY,        -- sha256:<hex>
    size_bytes  BIGINT      NOT NULL,
    storage_key TEXT        NOT NULL,           -- S3 key or local path: blobs/<algo>/<hex>
    ref_count   INT         NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- In-progress chunked uploads (POST initiate → PATCH stream → PUT finalise).
-- Cleaned up after finalisation or on timeout.
CREATE TABLE IF NOT EXISTS registry_upload_sessions (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    storage_key TEXT        NOT NULL,           -- temp write path
    bytes_received BIGINT   NOT NULL DEFAULT 0, -- bytes received so far
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- OCI manifests (image manifest or image index).
-- content is the raw JSON as sent by the client; digest is its sha256.
CREATE TABLE IF NOT EXISTS registry_manifests (
    digest      TEXT        PRIMARY KEY,        -- sha256:<hex>
    media_type  TEXT        NOT NULL,
    content     TEXT        NOT NULL,
    size_bytes  BIGINT      NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
