use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde_json::Value;

use crate::{
    blobs::compute_digest,
    error::{RegistryError, Result},
    kinds,
    RegistryState,
};

// ── PUT /v2/<namespace>/<repo>/manifests/<ref> ───────────────────────────────

pub async fn put_manifest(
    State(state): State<RegistryState>,
    Path((org, project, repo, reference)): Path<(String, String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let namespace = format!("{}/{}", org, project);
    let content_type = headers
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/vnd.oci.image.manifest.v1+json")
        .to_string();

    let digest = compute_digest(&body);
    let size = body.len() as i64;
    let content = String::from_utf8(body.to_vec())
        .map_err(|e| RegistryError::Internal(e.to_string()))?;

    // Upsert the raw manifest.
    sqlx::query(
        "INSERT INTO registry_manifests (digest, media_type, content, size_bytes)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (digest) DO UPDATE
             SET media_type = EXCLUDED.media_type,
                 content    = EXCLUDED.content",
    )
    .bind(&digest)
    .bind(&content_type)
    .bind(&content)
    .bind(size)
    .execute(&state.db)
    .await?;

    let manifest_value: Value = serde_json::from_str(&content)
        .map_err(|e| RegistryError::Internal(e.to_string()))?;

    let (kind, metadata) = kinds::extract_kind_metadata(&manifest_value);

    // Sum layer + config sizes from the manifest so artifacts.size_bytes reflects
    // actual content rather than the tiny manifest JSON length.
    let content_size: i64 = {
        let layers = manifest_value.get("layers").and_then(|l| l.as_array());
        let layer_sum: i64 = layers
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.get("size").and_then(|s| s.as_i64()))
                    .sum()
            })
            .unwrap_or(0);
        let config_size: i64 = manifest_value
            .get("config")
            .and_then(|c| c.get("size"))
            .and_then(|s| s.as_i64())
            .unwrap_or(0);
        // Fall back to manifest JSON length if no layer info (e.g. bare manifests).
        if layer_sum + config_size > 0 { layer_sum + config_size } else { size }
    };

    // Resolve namespace slug → id.
    let ns_id = sqlx::query_as::<_, (uuid::Uuid,)>(
        "SELECT id FROM registry_namespaces WHERE slug = $1",
    )
    .bind(&namespace)
    .fetch_optional(&state.db)
    .await?;

    if let Some((ns_id,)) = ns_id {
        let tag = reference.clone();

        sqlx::query(
            "INSERT INTO artifacts
                 (kind, namespace_id, repo, tag, manifest_digest, size_bytes, metadata, pushed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
             ON CONFLICT (namespace_id, repo, tag, kind) DO UPDATE
                 SET manifest_digest = EXCLUDED.manifest_digest,
                     size_bytes      = EXCLUDED.size_bytes,
                     metadata        = EXCLUDED.metadata,
                     pushed_at       = EXCLUDED.pushed_at",
        )
        .bind(&kind)
        .bind(ns_id)
        .bind(&repo)
        .bind(&tag)
        .bind(&digest)
        .bind(content_size)
        .bind(&metadata)
        .execute(&state.db)
        .await?;

        // Also update the `latest` tag for non-digest, non-latest refs.
        if !tag.starts_with("sha256:") && tag != "latest" {
            sqlx::query(
                "INSERT INTO artifacts
                     (kind, namespace_id, repo, tag, manifest_digest, size_bytes, metadata, pushed_at)
                 VALUES ($1, $2, $3, 'latest', $4, $5, $6, NOW())
                 ON CONFLICT (namespace_id, repo, tag, kind) DO UPDATE
                     SET manifest_digest = EXCLUDED.manifest_digest,
                         size_bytes      = EXCLUDED.size_bytes,
                         metadata        = EXCLUDED.metadata,
                         pushed_at       = EXCLUDED.pushed_at",
            )
            .bind(&kind)
            .bind(ns_id)
            .bind(&repo)
            .bind(&digest)
            .bind(content_size)
            .bind(&metadata)
            .execute(&state.db)
            .await?;
        }

        bump_blob_refs(&state, &manifest_value).await?;
    }

    Ok((
        StatusCode::CREATED,
        [
            ("Location",              format!("/v2/{}/{}/{}/manifests/{}", org, project, repo, digest)),
            ("Docker-Content-Digest", digest),
            ("Content-Length",        "0".to_string()),
        ],
    ).into_response())
}

// ── GET /v2/<namespace>/<repo>/manifests/<ref> ───────────────────────────────

pub async fn get_manifest(
    State(state): State<RegistryState>,
    Path((org, project, repo, reference)): Path<(String, String, String, String)>,
    headers: HeaderMap,
) -> Result<Response> {
    let namespace = format!("{}/{}", org, project);
    // (digest, media_type, content, size_bytes)
    type ManifestRow = (String, String, String, i64);

    let row: Option<ManifestRow> = if reference.starts_with("sha256:") {
        sqlx::query_as::<_, ManifestRow>(
            "SELECT digest, media_type, content, size_bytes
             FROM registry_manifests WHERE digest = $1",
        )
        .bind(&reference)
        .fetch_optional(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, ManifestRow>(
            "SELECT m.digest, m.media_type, m.content, m.size_bytes
             FROM artifacts a
             JOIN registry_namespaces ns ON a.namespace_id = ns.id
             JOIN registry_manifests  m  ON a.manifest_digest = m.digest
             WHERE ns.slug = $1 AND a.repo = $2 AND a.tag = $3
             ORDER BY a.pushed_at DESC
             LIMIT 1",
        )
        .bind(&namespace)
        .bind(&repo)
        .bind(&reference)
        .fetch_optional(&state.db)
        .await?
    };

    let (digest, media_type, content, size) = row
        .ok_or_else(|| RegistryError::ManifestNotFound(reference.clone()))?;

    let accept = headers
        .get("Accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Prefer the stored media type; override only when client explicitly requests a specific one.
    let content_type = if !accept.is_empty() && !accept.contains("*/*") && !accept.contains(&media_type) {
        &media_type
    } else {
        &media_type
    };

    Ok((
        StatusCode::OK,
        [
            ("Content-Type",          content_type.to_string()),
            ("Docker-Content-Digest", digest),
            ("Content-Length",        size.to_string()),
        ],
        content,
    ).into_response())
}

// ── HEAD /v2/<namespace>/<repo>/manifests/<ref> ──────────────────────────────

pub async fn head_manifest(
    state: State<RegistryState>,
    path: Path<(String, String, String, String)>,
    headers: HeaderMap,
) -> Result<Response> {
    let resp = get_manifest(state, path, headers).await?;
    let (parts, _body) = resp.into_parts();
    Ok(Response::from_parts(parts, axum::body::Body::empty()))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn bump_blob_refs(state: &RegistryState, manifest: &Value) -> Result<()> {
    let layers = manifest
        .get("layers")
        .and_then(|l| l.as_array())
        .cloned()
        .unwrap_or_default();

    for layer in &layers {
        if let Some(d) = layer.get("digest").and_then(|d| d.as_str()) {
            sqlx::query(
                "UPDATE registry_blobs SET ref_count = ref_count + 1 WHERE digest = $1",
            )
            .bind(d)
            .execute(&state.db)
            .await?;
        }
    }

    if let Some(d) = manifest
        .get("config")
        .and_then(|c| c.get("digest"))
        .and_then(|d| d.as_str())
    {
        sqlx::query(
            "UPDATE registry_blobs SET ref_count = ref_count + 1 WHERE digest = $1",
        )
        .bind(d)
        .execute(&state.db)
        .await?;
    }

    Ok(())
}
