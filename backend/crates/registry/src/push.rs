use std::sync::Arc;

use bytes::Bytes;
use sha2::{Sha256, Digest};
use sqlx::PgPool;
use uuid::Uuid;

use crate::storage::{StorageBackend, StorageError, blob_key};
use crate::kinds;

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Direct push helper for the build engine. Bypasses HTTP and writes blobs +
/// manifest rows directly into the DB and storage backend.
#[derive(Clone)]
pub struct ArtifactPusher {
    pub db: PgPool,
    pub storage: Arc<dyn StorageBackend>,
}

impl ArtifactPusher {
    pub fn new(db: PgPool, storage: Arc<dyn StorageBackend>) -> Self {
        Self { db, storage }
    }

    /// Get or create the registry namespace for an org/project pair.
    /// Slug format: `{org_slug}/{project_slug}` — matches the OCI path layout.
    async fn ensure_namespace(&self, org_id: Uuid, project_id: Uuid) -> Result<Uuid, PushError> {
        let org_slug: String = sqlx::query_scalar(
            "SELECT slug FROM organizations WHERE id = $1",
        )
        .bind(org_id)
        .fetch_one(&self.db)
        .await?;

        let project_slug: String = sqlx::query_scalar(
            "SELECT slug FROM projects WHERE id = $1",
        )
        .bind(project_id)
        .fetch_one(&self.db)
        .await?;

        let slug = format!("{}/{}", org_slug, project_slug);
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO registry_namespaces (id, org_id, project_id, slug)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (org_id, project_id) DO UPDATE SET slug = EXCLUDED.slug
             RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(org_id)
        .bind(project_id)
        .bind(&slug)
        .fetch_one(&self.db)
        .await?;
        Ok(row.0)
    }

    /// Write a blob to storage and upsert its DB record.
    /// Returns `(digest, size_bytes)`.
    async fn push_blob(&self, data: &Bytes) -> Result<(String, i64), PushError> {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let digest = format!("sha256:{}", hex::encode(hasher.finalize()));
        let size = data.len() as i64;
        let key = blob_key(&digest);

        if !self.storage.exists(&key).await.unwrap_or(false) {
            self.storage.put(&key, data.clone()).await?;
        }

        sqlx::query(
            "INSERT INTO registry_blobs (digest, size_bytes, storage_key, ref_count)
             VALUES ($1, $2, $3, 0)
             ON CONFLICT (digest) DO NOTHING",
        )
        .bind(&digest)
        .bind(size)
        .bind(&key)
        .execute(&self.db)
        .await?;

        Ok((digest, size))
    }

    fn manifest_digest(manifest_json: &str) -> String {
        let mut h = Sha256::new();
        h.update(manifest_json.as_bytes());
        format!("sha256:{}", hex::encode(h.finalize()))
    }

    /// Push a zip bundle as a `static_bundle` OCI artifact.
    ///
    /// Creates the namespace if needed, stores the blob, wraps it in a minimal
    /// OCI manifest, and upserts rows in `registry_manifests` + `artifacts`.
    /// Also writes a `latest` tag that always points to the most recent push.
    pub async fn push_static_bundle(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        repo: &str,
        tag: &str,
        zip_bytes: Bytes,
        metadata: serde_json::Value,
    ) -> Result<String, PushError> {
        let ns_id = self.ensure_namespace(org_id, project_id).await?;
        let total_size = zip_bytes.len() as i64;

        let (blob_digest, blob_size) = self.push_blob(&zip_bytes).await?;

        let manifest = kinds::wrap_single_blob(
            &blob_digest,
            blob_size,
            kinds::kind::STATIC_BUNDLE,
            serde_json::Map::new(),
        );
        let manifest_json = serde_json::to_string(&manifest)?;
        let manifest_digest = Self::manifest_digest(&manifest_json);

        sqlx::query(
            "INSERT INTO registry_manifests (digest, media_type, content, size_bytes)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (digest) DO NOTHING",
        )
        .bind(&manifest_digest)
        .bind("application/vnd.oci.image.manifest.v1+json")
        .bind(&manifest_json)
        .bind(manifest_json.len() as i64)
        .execute(&self.db)
        .await?;

        sqlx::query(
            "UPDATE registry_blobs SET ref_count = ref_count + 1 WHERE digest = $1",
        )
        .bind(&blob_digest)
        .execute(&self.db)
        .await?;

        self.upsert_artifact(
            ns_id, kinds::kind::STATIC_BUNDLE, repo, tag,
            &manifest_digest, total_size, &metadata,
        ).await?;

        if tag != "latest" {
            self.upsert_artifact(
                ns_id, kinds::kind::STATIC_BUNDLE, repo, "latest",
                &manifest_digest, total_size, &metadata,
            ).await?;
        }

        Ok(manifest_digest)
    }

    /// Record a locally-built Docker image reference without copying layer blobs.
    ///
    /// The image lives in the Docker daemon; this just creates an artifact entry
    /// so the registry UI and deploy engine can reference it.
    pub async fn record_docker_image(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        repo: &str,
        tag: &str,
        image_ref: &str,
    ) -> Result<(), PushError> {
        let ns_id = self.ensure_namespace(org_id, project_id).await?;

        // Extract referenced tag from image_ref if present (e.g. registry/org/repo:tag)
        let ref_tag = image_ref.split(':').last().unwrap_or("");
        if !ref_tag.is_empty() {
            let existing = sqlx::query_as::<_, (String, i64, serde_json::Value)>(
                "SELECT manifest_digest, size_bytes, metadata 
                 FROM artifacts 
                 WHERE namespace_id = $1 AND repo = $2 AND tag = $3"
            )
            .bind(ns_id)
            .bind(repo)
            .bind(ref_tag)
            .fetch_optional(&self.db)
            .await?;

            if let Some((manifest_digest, size_bytes, metadata)) = existing {
                self.upsert_artifact(
                    ns_id, kinds::kind::DOCKER_IMAGE, repo, tag,
                    &manifest_digest, size_bytes, &metadata,
                ).await?;

                if tag != "latest" {
                    self.upsert_artifact(
                        ns_id, kinds::kind::DOCKER_IMAGE, repo, "latest",
                        &manifest_digest, size_bytes, &metadata,
                    ).await?;
                }
                return Ok(());
            }
        }

        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "size": 0
            },
            "layers": [],
            "annotations": {
                "org.shipyard.artifact.kind": "docker_image",
                "org.shipyard.docker.image_ref": image_ref
            }
        });
        let manifest_json = serde_json::to_string(&manifest)?;
        let manifest_digest = Self::manifest_digest(&manifest_json);

        sqlx::query(
            "INSERT INTO registry_manifests (digest, media_type, content, size_bytes)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (digest) DO NOTHING",
        )
        .bind(&manifest_digest)
        .bind("application/vnd.oci.image.manifest.v1+json")
        .bind(&manifest_json)
        .bind(manifest_json.len() as i64)
        .execute(&self.db)
        .await?;

        let metadata = serde_json::json!({ "image_ref": image_ref });
        self.upsert_artifact(
            ns_id, kinds::kind::DOCKER_IMAGE, repo, tag,
            &manifest_digest, 0, &metadata,
        ).await?;

        if tag != "latest" {
            self.upsert_artifact(
                ns_id, kinds::kind::DOCKER_IMAGE, repo, "latest",
                &manifest_digest, 0, &metadata,
            ).await?;
        }

        Ok(())
    }

    /// Save a local docker image, extract its layers, and push them to the registry storage (S3/local).
    /// Returns the OCI manifest digest of the pushed image.
    pub async fn push_docker_image(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        repo: &str,
        tag: &str,
        local_image_tag: &str,
    ) -> Result<String, PushError> {
        let ns_id = self.ensure_namespace(org_id, project_id).await?;

        // 1. Create a temporary file to save the docker tar
        let temp_dir = tempfile::tempdir().map_err(|e| PushError::Storage(StorageError::Io(e)))?;
        let tar_path = temp_dir.path().join("image.tar");

        // Run `docker save <local_image_tag> -o <tar_path>`
        let status = tokio::process::Command::new("docker")
            .args(["save", local_image_tag, "-o"])
            .arg(&tar_path)
            .status()
            .await
            .map_err(|e| PushError::Storage(StorageError::Io(e)))?;

        if !status.success() {
            return Err(PushError::Storage(StorageError::Backend(format!(
                "Failed to save docker image: {}",
                local_image_tag
            ))));
        }

        // 2. Extract the tarball using `tar` command line tool
        let extract_dir = temp_dir.path().join("extracted");
        std::fs::create_dir_all(&extract_dir).map_err(|e| PushError::Storage(StorageError::Io(e)))?;

        let extract_status = tokio::process::Command::new("tar")
            .arg("-xf")
            .arg(&tar_path)
            .arg("-C")
            .arg(&extract_dir)
            .status()
            .await
            .map_err(|e| PushError::Storage(StorageError::Io(e)))?;

        if !extract_status.success() {
            return Err(PushError::Storage(StorageError::Backend(
                "Failed to extract docker image tarball".into()
            )));
        }

        // 3. Read manifest.json
        let manifest_path = extract_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Err(PushError::Storage(StorageError::Backend(
                "docker save manifest.json not found".into()
            )));
        }

        let manifest_content = tokio::fs::read_to_string(&manifest_path)
            .await
            .map_err(|e| PushError::Storage(StorageError::Io(e)))?;

        let manifest_val: serde_json::Value = serde_json::from_str(&manifest_content)?;
        let manifest_arr = manifest_val.as_array().ok_or_else(|| {
            PushError::Storage(StorageError::Backend("Invalid manifest.json structure".into()))
        })?;

        if manifest_arr.is_empty() {
            return Err(PushError::Storage(StorageError::Backend("Empty manifest.json".into())));
        }

        let img_manifest = &manifest_arr[0];
        let config_file = img_manifest.get("Config").and_then(|c| c.as_str()).ok_or_else(|| {
            PushError::Storage(StorageError::Backend("Config file not defined in manifest".into()))
        })?;

        let layers = img_manifest.get("Layers").and_then(|l| l.as_array()).ok_or_else(|| {
            PushError::Storage(StorageError::Backend("Layers not defined in manifest".into()))
        })?;

        // 4. Push the config blob
        let config_path = extract_dir.join(config_file);
        let config_bytes = tokio::fs::read(&config_path)
            .await
            .map_err(|e| PushError::Storage(StorageError::Io(e)))?;
        let config_bytes = Bytes::from(config_bytes);
        let (config_digest, config_size) = self.push_blob(&config_bytes).await?;

        // 5. Push all layer blobs and build their OCI descriptor objects
        let mut oci_layers = Vec::new();
        let mut total_size = config_size;

        for layer_val in layers {
            let layer_rel_path = layer_val.as_str().ok_or_else(|| {
                PushError::Storage(StorageError::Backend("Invalid layer path".into()))
            })?;
            let layer_path = extract_dir.join(layer_rel_path);
            let layer_bytes = tokio::fs::read(&layer_path)
                .await
                .map_err(|e| PushError::Storage(StorageError::Io(e)))?;
            let layer_bytes = Bytes::from(layer_bytes);
            let (layer_digest, layer_size) = self.push_blob(&layer_bytes).await?;
            total_size += layer_size;

            oci_layers.push(serde_json::json!({
                "mediaType": "application/vnd.oci.image.layer.v1.tar",
                "size": layer_size,
                "digest": layer_digest
            }));
        }

        // 6. Build the OCI manifest structure
        let oci_manifest = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "size": config_size,
                "digest": config_digest
            },
            "layers": oci_layers,
            "annotations": {
                "org.shipyard.artifact.kind": "docker_image",
                "org.shipyard.docker.image_ref": local_image_tag
            }
        });

        let manifest_json = serde_json::to_string(&oci_manifest)?;
        let manifest_digest = Self::manifest_digest(&manifest_json);

        // Save manifest row in DB
        sqlx::query(
            "INSERT INTO registry_manifests (digest, media_type, content, size_bytes)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (digest) DO NOTHING",
        )
        .bind(&manifest_digest)
        .bind("application/vnd.oci.image.manifest.v1+json")
        .bind(&manifest_json)
        .bind(manifest_json.len() as i64)
        .execute(&self.db)
        .await?;

        // Increment ref count for config blob
        sqlx::query("UPDATE registry_blobs SET ref_count = ref_count + 1 WHERE digest = $1")
            .bind(&config_digest)
            .execute(&self.db)
            .await?;

        // Increment ref count for each layer
        for layer_val in layers {
            let layer_rel_path = layer_val.as_str().unwrap();
            let layer_path = extract_dir.join(layer_rel_path);
            let layer_bytes = tokio::fs::read(&layer_path).await
                .map_err(|e| PushError::Storage(StorageError::Io(e)))?;
            let mut h = Sha256::new();
            h.update(&layer_bytes);
            let digest = format!("sha256:{}", hex::encode(h.finalize()));

            sqlx::query("UPDATE registry_blobs SET ref_count = ref_count + 1 WHERE digest = $1")
                .bind(&digest)
                .execute(&self.db)
                .await?;
        }

        // Save artifact
        let metadata = serde_json::json!({
            "image_ref": local_image_tag,
            "layers": layers.len()
        });

        self.upsert_artifact(
            ns_id, kinds::kind::DOCKER_IMAGE, repo, tag,
            &manifest_digest, total_size, &metadata,
        ).await?;

        if tag != "latest" {
            self.upsert_artifact(
                ns_id, kinds::kind::DOCKER_IMAGE, repo, "latest",
                &manifest_digest, total_size, &metadata,
            ).await?;
        }

        Ok(manifest_digest)
    }


    /// Push an edge function bundle as an `edge_function` OCI artifact.
    ///
    /// Creates the namespace if needed, stores the blob, wraps it in a minimal
    /// OCI manifest, and upserts rows in `registry_manifests` + `artifacts`.
    /// Also writes a `latest` tag that always points to the most recent push.
    pub async fn push_edge_function(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        repo: &str,
        tag: &str,
        bundle_bytes: Bytes,
        metadata: serde_json::Value,
    ) -> Result<String, PushError> {
        let ns_id = self.ensure_namespace(org_id, project_id).await?;
        let total_size = bundle_bytes.len() as i64;

        let (blob_digest, blob_size) = self.push_blob(&bundle_bytes).await?;

        let manifest = kinds::wrap_single_blob(
            &blob_digest,
            blob_size,
            kinds::kind::EDGE_FUNCTION,
            serde_json::Map::new(),
        );
        let manifest_json = serde_json::to_string(&manifest)?;
        let manifest_digest = Self::manifest_digest(&manifest_json);

        sqlx::query(
            "INSERT INTO registry_manifests (digest, media_type, content, size_bytes)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (digest) DO NOTHING",
        )
        .bind(&manifest_digest)
        .bind("application/vnd.oci.image.manifest.v1+json")
        .bind(&manifest_json)
        .bind(manifest_json.len() as i64)
        .execute(&self.db)
        .await?;

        sqlx::query(
            "UPDATE registry_blobs SET ref_count = ref_count + 1 WHERE digest = $1",
        )
        .bind(&blob_digest)
        .execute(&self.db)
        .await?;

        self.upsert_artifact(
            ns_id, kinds::kind::EDGE_FUNCTION, repo, tag,
            &manifest_digest, total_size, &metadata,
        ).await?;

        if tag != "latest" {
            self.upsert_artifact(
                ns_id, kinds::kind::EDGE_FUNCTION, repo, "latest",
                &manifest_digest, total_size, &metadata,
            ).await?;
        }

        Ok(manifest_digest)
    }

    async fn upsert_artifact(
        &self,
        namespace_id: Uuid,
        kind: &str,
        repo: &str,
        tag: &str,
        manifest_digest: &str,
        size_bytes: i64,
        metadata: &serde_json::Value,
    ) -> Result<(), PushError> {
        sqlx::query(
            "INSERT INTO artifacts
                (id, kind, namespace_id, repo, tag, manifest_digest, size_bytes, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (namespace_id, repo, tag, kind) DO UPDATE
             SET manifest_digest = EXCLUDED.manifest_digest,
                 size_bytes      = EXCLUDED.size_bytes,
                 metadata        = EXCLUDED.metadata,
                 pushed_at       = now()",
        )
        .bind(Uuid::now_v7())
        .bind(kind)
        .bind(namespace_id)
        .bind(repo)
        .bind(tag)
        .bind(manifest_digest)
        .bind(size_bytes)
        .bind(metadata)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
