use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    body::Body,
};
use bytes::Bytes;
use serde::Deserialize;
use sha2::{Sha256, Digest};
use uuid::Uuid;

use crate::{
    error::{RegistryError, Result},
    storage::{blob_key, upload_key},
    RegistryState,
};

// ── POST /v2/<name>/blobs/uploads/ ──────────────────────────────────────────

pub async fn initiate_upload(
    State(state): State<RegistryState>,
    Path((org, project, repo)): Path<(String, String, String)>,
) -> Result<Response> {
    let session_id = Uuid::new_v4().to_string();
    let key = upload_key(&session_id);

    state.storage.put(&key, Bytes::new()).await?;

    sqlx::query(
        "INSERT INTO registry_upload_sessions (id, storage_key) VALUES ($1, $2)",
    )
    .bind(Uuid::parse_str(&session_id).unwrap())
    .bind(&key)
    .execute(&state.db)
    .await?;

    let location = format!("/v2/{}/{}/{}/blobs/uploads/{}", org, project, repo, session_id);
    Ok((
        StatusCode::ACCEPTED,
        [
            ("Location", location),
            ("Docker-Upload-UUID", session_id),
        ],
    ).into_response())
}

// ── PATCH /v2/<name>/blobs/uploads/<uuid> ───────────────────────────────────

pub async fn patch_upload(
    State(state): State<RegistryState>,
    Path((org, project, repo, session_id)): Path<(String, String, String, String)>,
    body: Bytes,
) -> Result<Response> {
    let session_uuid = Uuid::parse_str(&session_id)
        .map_err(|_| RegistryError::UploadNotFound(session_id.clone()))?;

    let row = sqlx::query_as::<_, (String, i64)>(
        "SELECT storage_key, bytes_received FROM registry_upload_sessions WHERE id = $1",
    )
    .bind(session_uuid)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| RegistryError::UploadNotFound(session_id.clone()))?;

    let (storage_key, bytes_received) = row;

    let existing = if bytes_received > 0 {
        state.storage.get_bytes(&storage_key).await.unwrap_or_default()
    } else {
        Bytes::new()
    };

    let new_len = existing.len() + body.len();
    let mut combined = bytes::BytesMut::with_capacity(new_len);
    combined.extend_from_slice(&existing);
    combined.extend_from_slice(&body);
    state.storage.put(&storage_key, combined.freeze()).await?;

    sqlx::query("UPDATE registry_upload_sessions SET bytes_received = $1 WHERE id = $2")
        .bind(new_len as i64)
        .bind(session_uuid)
        .execute(&state.db)
        .await?;

    let location = format!("/v2/{}/{}/{}/blobs/uploads/{}", org, project, repo, session_id);
    let range = format!("0-{}", new_len.saturating_sub(1));
    Ok((
        StatusCode::ACCEPTED,
        [
            ("Location", location),
            ("Range", range),
            ("Docker-Upload-UUID", session_id),
        ],
    ).into_response())
}

// ── PUT /v2/<name>/blobs/uploads/<uuid>?digest=sha256:<hex> ─────────────────

#[derive(Deserialize)]
pub struct FinaliseQuery {
    pub digest: String,
}

pub async fn finalise_upload(
    State(state): State<RegistryState>,
    Path((org, project, repo, session_id)): Path<(String, String, String, String)>,
    Query(q): Query<FinaliseQuery>,
    body: Bytes,
) -> Result<Response> {
    let session_uuid = Uuid::parse_str(&session_id)
        .map_err(|_| RegistryError::UploadNotFound(session_id.clone()))?;

    let row = sqlx::query_as::<_, (String, i64)>(
        "SELECT storage_key, bytes_received FROM registry_upload_sessions WHERE id = $1",
    )
    .bind(session_uuid)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| RegistryError::UploadNotFound(session_id.clone()))?;

    let (storage_key, bytes_received) = row;

    let existing = if bytes_received > 0 {
        state.storage.get_bytes(&storage_key).await.unwrap_or_default()
    } else {
        Bytes::new()
    };

    let mut combined = bytes::BytesMut::with_capacity(existing.len() + body.len());
    combined.extend_from_slice(&existing);
    combined.extend_from_slice(&body);
    let blob_bytes = combined.freeze();

    let actual_digest = compute_digest(&blob_bytes);
    if actual_digest != q.digest {
        return Err(RegistryError::DigestMismatch { expected: q.digest, got: actual_digest });
    }

    let size = blob_bytes.len() as i64;
    let key = blob_key(&actual_digest);

    if !state.storage.exists(&key).await? {
        state.storage.put(&key, blob_bytes).await?;
    }

    sqlx::query(
        "INSERT INTO registry_blobs (digest, size_bytes, storage_key, ref_count)
         VALUES ($1, $2, $3, 0)
         ON CONFLICT (digest) DO NOTHING",
    )
    .bind(&actual_digest)
    .bind(size)
    .bind(&key)
    .execute(&state.db)
    .await?;

    let _ = state.storage.delete(&storage_key).await;
    sqlx::query("DELETE FROM registry_upload_sessions WHERE id = $1")
        .bind(session_uuid)
        .execute(&state.db)
        .await?;

    let location = format!("/v2/{}/{}/{}/blobs/{}", org, project, repo, actual_digest);
    Ok((
        StatusCode::CREATED,
        [
            ("Location", location),
            ("Docker-Content-Digest", actual_digest),
        ],
    ).into_response())
}

// ── HEAD /v2/<name>/blobs/<digest> ──────────────────────────────────────────

pub async fn head_blob(
    State(state): State<RegistryState>,
    Path((_org, _project, _repo, digest)): Path<(String, String, String, String)>,
) -> Result<Response> {
    let row = sqlx::query_as::<_, (i64,)>(
        "SELECT size_bytes FROM registry_blobs WHERE digest = $1",
    )
    .bind(&digest)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| RegistryError::BlobNotFound(digest.clone()))?;

    Ok((
        StatusCode::OK,
        [
            ("Content-Length", row.0.to_string()),
            ("Docker-Content-Digest", digest),
            ("Content-Type", "application/octet-stream".to_string()),
        ],
    ).into_response())
}

// ── GET /v2/<name>/blobs/<digest> ───────────────────────────────────────────

pub async fn get_blob(
    State(state): State<RegistryState>,
    Path((_org, _project, _repo, digest)): Path<(String, String, String, String)>,
) -> Result<Response> {
    let row = sqlx::query_as::<_, (i64, String)>(
        "SELECT size_bytes, storage_key FROM registry_blobs WHERE digest = $1",
    )
    .bind(&digest)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| RegistryError::BlobNotFound(digest.clone()))?;

    let (size, storage_key) = row;
    let stream = state.storage.get(&storage_key).await?;

    let mut headers = HeaderMap::new();
    headers.insert("Content-Length", HeaderValue::from(size as u64));
    headers.insert("Docker-Content-Digest", HeaderValue::from_str(&digest).unwrap());
    headers.insert("Content-Type", HeaderValue::from_static("application/octet-stream"));

    Ok((StatusCode::OK, headers, Body::from_stream(stream)).into_response())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

pub fn compute_digest(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}
