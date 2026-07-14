use axum::{
    extract::{Path, Query, State},
    routing::{delete, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::ApiAppError,
    AppState,
};
use shipyard_common::{error::AppError, types::ApiResponse};

// ── AES-256-GCM credential encryption ────────────────────────────────────────

fn derive_key(secret: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(secret.as_bytes()).into()
}

pub fn encrypt_password(plaintext: &str, secret: &str) -> Result<String, String> {
    use aes_gcm::{aead::{Aead, AeadCore, KeyInit, OsRng}, Aes256Gcm};
    let key = derive_key(secret);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = cipher.encrypt(&nonce, plaintext.as_bytes()).map_err(|e| e.to_string())?;
    Ok(format!("{}:{}", hex::encode(nonce), hex::encode(ct)))
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ExternalRegistry {
    pub id:           Uuid,
    pub org_id:       Uuid,
    pub name:         String,
    pub registry_url: String,
    pub username:     Option<String>,
    pub created_at:   chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRegistryBody {
    pub name:         String,
    pub registry_url: String,
    pub username:     Option<String>,
    pub password:     Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactRow {
    pub id:              Uuid,
    pub kind:            String,
    pub repo:            String,
    pub tag:             String,
    pub size_bytes:      i64,
    pub manifest_digest: String,
    pub metadata:        serde_json::Value,
    pub pushed_at:       chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct RegistryStats {
    pub total_blobs:     i64,
    pub total_size:      i64,
    pub total_artifacts: i64,
    pub by_kind:         Vec<KindStat>,
}

#[derive(Debug, Serialize)]
pub struct KindStat {
    pub kind:  String,
    pub count: i64,
    pub size:  i64,
}

// ── Route builders ────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        // Settings sub-group
        .route("/external-registries",      get(list_external).post(create_external))
        .route("/external-registries/:id",  delete(delete_external))
        // Browse hierarchy
        .route("/namespaces",                               get(list_namespaces))
        .route("/namespaces/:ns_id/repos",                  get(list_repos))
        .route("/namespaces/:ns_id/repos/:repo",            delete(delete_repo))
        .route("/namespaces/:ns_id/repos/:repo/tags",       get(list_tags))
        // Misc
        .route("/artifacts",        get(list_artifacts))
        .route("/artifacts/search", get(search_artifacts))
        .route("/stats",            get(registry_stats))
        .route("/info",             get(registry_info))
}

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/stats",                                    get(admin_stats))
        .route("/namespaces",                               get(admin_namespaces))
        .route("/namespaces/search",                        get(admin_search))
        .route("/namespaces/:ns_id/repos",                  get(admin_list_repos))
        .route("/namespaces/:ns_id",                        delete(admin_delete_namespace))
        .route("/namespaces/:ns_id/repos/:repo",            delete(admin_delete_repo))
}

// ── Org-scoped handlers ───────────────────────────────────────────────────────

async fn list_external(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<ExternalRegistry>>>, ApiAppError> {
    check_org_access(&state, auth.user_id, org_id).await?;

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, Option<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, org_id, name, registry_url, username, created_at
         FROM external_registries WHERE org_id = $1 ORDER BY created_at ASC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let list = rows.into_iter().map(|(id, org_id, name, registry_url, username, created_at)| {
        ExternalRegistry { id, org_id, name, registry_url, username, created_at }
    }).collect();

    Ok(Json(ApiResponse::ok(list)))
}

async fn create_external(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
    Json(body): Json<CreateRegistryBody>,
) -> Result<Json<ApiResponse<ExternalRegistry>>, ApiAppError> {
    check_org_access(&state, auth.user_id, org_id).await?;

    if body.name.trim().is_empty() || body.registry_url.trim().is_empty() {
        return Err(ApiAppError(AppError::BadRequest("name and registry_url are required".into())));
    }

    let password_enc = match &body.password {
        Some(pw) if !pw.is_empty() => {
            Some(encrypt_password(pw, &state.config.auth.secret_key)
                .map_err(|e| ApiAppError(AppError::Internal(format!("encrypt: {e}"))))?)
        }
        _ => None,
    };

    let id = Uuid::now_v7();
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO external_registries (id, org_id, name, registry_url, username, password_enc, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(org_id)
    .bind(&body.name)
    .bind(&body.registry_url)
    .bind(&body.username)
    .bind(password_enc)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(ExternalRegistry {
        id,
        org_id,
        name: body.name,
        registry_url: body.registry_url,
        username: body.username,
        created_at: now,
    })))
}

async fn delete_external(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, reg_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    check_org_access(&state, auth.user_id, org_id).await?;

    sqlx::query("DELETE FROM external_registries WHERE id = $1 AND org_id = $2")
        .bind(reg_id)
        .bind(org_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({}))))
}

async fn list_artifacts(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<ArtifactRow>>>, ApiAppError> {
    check_registry_access(&state, auth.user_id, org_id, false).await?;

    let rows = sqlx::query_as::<_, (Uuid, String, String, String, i64, String, serde_json::Value, chrono::DateTime<chrono::Utc>)>(
        "SELECT a.id, a.kind, a.repo, a.tag, a.size_bytes, a.manifest_digest, a.metadata, a.pushed_at
         FROM artifacts a
         JOIN registry_namespaces ns ON ns.id = a.namespace_id
         WHERE ns.org_id = $1
         ORDER BY a.pushed_at DESC
         LIMIT 200",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let list = rows.into_iter().map(|(id, kind, repo, tag, size_bytes, manifest_digest, metadata, pushed_at)| {
        ArtifactRow { id, kind, repo, tag, size_bytes, manifest_digest, metadata, pushed_at }
    }).collect();

    Ok(Json(ApiResponse::ok(list)))
}

#[derive(Debug, Serialize)]
pub struct ArtifactSearchResult {
    pub id:              Uuid,
    pub namespace_id:    Uuid,
    pub namespace_slug:  String,
    pub repo:            String,
    pub tag:             String,
    pub kind:            String,
    pub size_bytes:      i64,
    pub pushed_at:       chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ArtifactSearchQuery {
    /// Free-text filter applied against repo name and namespace slug.
    #[serde(default)]
    pub q: String,
    /// Optional kind filter: docker_image | static_bundle | edge_function | build_cache
    pub kind: Option<String>,
}

async fn search_artifacts(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
    Query(params): Query<ArtifactSearchQuery>,
) -> Result<Json<ApiResponse<Vec<ArtifactSearchResult>>>, ApiAppError> {
    check_registry_access(&state, auth.user_id, org_id, false).await?;

    let q = if params.q.trim().is_empty() { None } else { Some(format!("%{}%", params.q.trim().to_lowercase())) };

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, String, i64, chrono::DateTime<chrono::Utc>)>(
        "SELECT a.id, ns.id, ns.slug, a.repo, a.tag, a.kind, a.size_bytes, a.pushed_at
         FROM artifacts a
         JOIN registry_namespaces ns ON ns.id = a.namespace_id
         WHERE ns.org_id = $1
           AND ($2::text IS NULL OR (LOWER(a.repo) LIKE $2 OR LOWER(ns.slug) LIKE $2))
           AND ($3::text IS NULL OR a.kind = $3)
         ORDER BY a.pushed_at DESC
         LIMIT 100",
    )
    .bind(org_id)
    .bind(&q)
    .bind(&params.kind)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let results = rows.into_iter().map(|(id, namespace_id, namespace_slug, repo, tag, kind, size_bytes, pushed_at)| {
        ArtifactSearchResult { id, namespace_id, namespace_slug, repo, tag, kind, size_bytes, pushed_at }
    }).collect();

    Ok(Json(ApiResponse::ok(results)))
}

async fn registry_stats(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<RegistryStats>>, ApiAppError> {
    check_registry_access(&state, auth.user_id, org_id, false).await?;

    let (total_blobs, total_size): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0) FROM registry_blobs",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0));

    let (total_artifacts,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM artifacts a
         JOIN registry_namespaces ns ON ns.id = a.namespace_id
         WHERE ns.org_id = $1",
    )
    .bind(org_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    let kind_rows = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT a.kind, COUNT(*) AS cnt, COALESCE(SUM(a.size_bytes), 0) AS sz
         FROM artifacts a
         JOIN registry_namespaces ns ON ns.id = a.namespace_id
         WHERE ns.org_id = $1
         GROUP BY a.kind ORDER BY cnt DESC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let by_kind = kind_rows.into_iter()
        .map(|(kind, count, size)| KindStat { kind, count, size })
        .collect();

    Ok(Json(ApiResponse::ok(RegistryStats {
        total_blobs, total_size, total_artifacts, by_kind,
    })))
}

async fn registry_info(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    check_registry_access(&state, auth.user_id, org_id, false).await?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "hostname":     state.config.registry.hostname,
        "storage_type": state.config.registry.storage,
    }))))
}

// ── Browse hierarchy handlers ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct NamespaceRow {
    pub id:             Uuid,
    pub slug:           String,
    pub artifact_count: i64,
    pub total_size:     i64,
    pub last_pushed:    Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct RepoRow {
    pub repo:           String,
    pub kind:           String,
    pub tag_count:      i64,
    pub total_size:     i64,
    pub last_pushed:    chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct TagRow {
    pub id:              Uuid,
    pub tag:             String,
    pub kind:            String,
    pub size_bytes:      i64,
    pub manifest_digest: String,
    pub metadata:        serde_json::Value,
    pub pushed_at:       chrono::DateTime<chrono::Utc>,
}

async fn list_namespaces(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<NamespaceRow>>>, ApiAppError> {
    check_registry_access(&state, auth.user_id, org_id, false).await?;

    let rows = sqlx::query_as::<_, (Uuid, String, i64, i64, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT ns.id, ns.slug,
                COUNT(a.id)::BIGINT                      AS artifact_count,
                COALESCE(SUM(a.size_bytes), 0)::BIGINT   AS total_size,
                MAX(a.pushed_at)                          AS last_pushed
         FROM registry_namespaces ns
         LEFT JOIN artifacts a ON a.namespace_id = ns.id
         WHERE ns.org_id = $1
         GROUP BY ns.id, ns.slug
         ORDER BY last_pushed DESC NULLS LAST",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let list = rows.into_iter().map(|(id, slug, artifact_count, total_size, last_pushed)| {
        NamespaceRow { id, slug, artifact_count, total_size, last_pushed }
    }).collect();

    Ok(Json(ApiResponse::ok(list)))
}

async fn list_repos(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, ns_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<Vec<RepoRow>>>, ApiAppError> {
    check_registry_access(&state, auth.user_id, org_id, false).await?;

    let rows = sqlx::query_as::<_, (String, String, i64, i64, chrono::DateTime<chrono::Utc>)>(
        "SELECT repo, kind,
                COUNT(*)::BIGINT                     AS tag_count,
                COALESCE(SUM(size_bytes), 0)::BIGINT AS total_size,
                MAX(pushed_at)                        AS last_pushed
         FROM artifacts
         WHERE namespace_id = $1
         GROUP BY repo, kind
         ORDER BY last_pushed DESC",
    )
    .bind(ns_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let list = rows.into_iter().map(|(repo, kind, tag_count, total_size, last_pushed)| {
        RepoRow { repo, kind, tag_count, total_size, last_pushed }
    }).collect();

    Ok(Json(ApiResponse::ok(list)))
}

async fn list_tags(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, ns_id, repo)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<ApiResponse<Vec<TagRow>>>, ApiAppError> {
    check_registry_access(&state, auth.user_id, org_id, false).await?;

    let rows = sqlx::query_as::<_, (Uuid, String, String, i64, String, serde_json::Value, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, tag, kind, size_bytes, manifest_digest, metadata, pushed_at
         FROM artifacts
         WHERE namespace_id = $1 AND repo = $2
         ORDER BY pushed_at DESC",
    )
    .bind(ns_id)
    .bind(&repo)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let list = rows.into_iter().map(|(id, tag, kind, size_bytes, manifest_digest, metadata, pushed_at)| {
        TagRow { id, tag, kind, size_bytes, manifest_digest, metadata, pushed_at }
    }).collect();

    Ok(Json(ApiResponse::ok(list)))
}

async fn delete_repo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, ns_id, repo)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    check_registry_access(&state, auth.user_id, org_id, true).await?;

    // Verify the namespace belongs to this org.
    let ns_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM registry_namespaces WHERE id = $1 AND org_id = $2)",
    )
    .bind(ns_id)
    .bind(org_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if !ns_exists {
        return Err(ApiAppError(AppError::NotFound("Namespace not found".into())));
    }

    let result = sqlx::query(
        "DELETE FROM artifacts WHERE namespace_id = $1 AND repo = $2",
    )
    .bind(ns_id)
    .bind(&repo)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "deleted": true,
        "rows": result.rows_affected(),
    }))))
}

// ── Admin handlers ────────────────────────────────────────────────────────────

async fn admin_stats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::admin::require_admin_access(&state.db, auth.user_id, "shipyard:admin:registry:view").await?;
    let (blobs, blob_size): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*)::BIGINT, COALESCE(SUM(size_bytes), 0)::BIGINT FROM registry_blobs",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0));

    let (artifacts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM artifacts")
        .fetch_one(&state.db).await.unwrap_or((0,));

    let (namespaces,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM registry_namespaces")
        .fetch_one(&state.db).await.unwrap_or((0,));

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "total_blobs":      blobs,
        "total_blob_size":  blob_size,
        "total_artifacts":  artifacts,
        "total_namespaces": namespaces,
        "hostname":         state.config.registry.hostname,
        "storage_type":     state.config.registry.storage,
    }))))
}

async fn admin_namespaces(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiAppError> {
    crate::admin::require_admin_access(&state.db, auth.user_id, "shipyard:admin:registry:view").await?;

    let rows = sqlx::query_as::<_, (Uuid, String, i64, i64, chrono::DateTime<chrono::Utc>)>(
        "SELECT ns.id, ns.slug,
                COUNT(a.id)::BIGINT                    AS artifact_count,
                COALESCE(SUM(a.size_bytes), 0)::BIGINT AS total_size,
                ns.created_at
         FROM registry_namespaces ns
         LEFT JOIN artifacts a ON a.namespace_id = ns.id
         GROUP BY ns.id, ns.slug, ns.created_at
         ORDER BY ns.created_at DESC
         LIMIT 200",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let list = rows.into_iter().map(|(id, slug, artifact_count, total_size, created_at)| {
        serde_json::json!({
            "id": id, "slug": slug,
            "artifact_count": artifact_count,
            "total_size": total_size,
            "created_at": created_at,
        })
    }).collect();

    Ok(Json(ApiResponse::ok(list)))
}

#[derive(Debug, Deserialize)]
struct AdminSearchQuery {
    #[serde(default)]
    q: String,
}

async fn admin_search(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<AdminSearchQuery>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiAppError> {
    crate::admin::require_admin_access(&state.db, auth.user_id, "shipyard:admin:registry:view").await?;

    let q = if params.q.trim().is_empty() { None } else { Some(format!("%{}%", params.q.trim().to_lowercase())) };

    let rows = sqlx::query_as::<_, (Uuid, String, i64, i64, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT ns.id, ns.slug,
                COUNT(a.id)::BIGINT                    AS artifact_count,
                COALESCE(SUM(a.size_bytes), 0)::BIGINT AS total_size,
                MAX(a.pushed_at)                        AS last_pushed
         FROM registry_namespaces ns
         LEFT JOIN artifacts a ON a.namespace_id = ns.id
         WHERE ($1::text IS NULL OR LOWER(ns.slug) LIKE $1)
         GROUP BY ns.id, ns.slug
         ORDER BY last_pushed DESC NULLS LAST
         LIMIT 100",
    )
    .bind(&q)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let list = rows.into_iter().map(|(id, slug, artifact_count, total_size, last_pushed)| {
        serde_json::json!({
            "id": id, "slug": slug,
            "artifact_count": artifact_count,
            "total_size": total_size,
            "last_pushed": last_pushed,
        })
    }).collect();

    Ok(Json(ApiResponse::ok(list)))
}

async fn admin_list_repos(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(ns_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiAppError> {
    crate::admin::require_admin_access(&state.db, auth.user_id, "shipyard:admin:registry:view").await?;

    let rows = sqlx::query_as::<_, (String, String, i64, i64, chrono::DateTime<chrono::Utc>)>(
        "SELECT repo, kind,
                COUNT(*)::BIGINT                     AS tag_count,
                COALESCE(SUM(size_bytes), 0)::BIGINT AS total_size,
                MAX(pushed_at)                        AS last_pushed
         FROM artifacts
         WHERE namespace_id = $1
         GROUP BY repo, kind
         ORDER BY last_pushed DESC",
    )
    .bind(ns_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let list = rows.into_iter().map(|(repo, kind, tag_count, total_size, last_pushed)| {
        serde_json::json!({ "repo": repo, "kind": kind, "tag_count": tag_count, "total_size": total_size, "last_pushed": last_pushed })
    }).collect();

    Ok(Json(ApiResponse::ok(list)))
}

async fn admin_delete_namespace(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(ns_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::admin::require_admin_access(&state.db, auth.user_id, "shipyard:admin:registry:manage").await?;

    // Delete all artifacts first (manifests/blobs stay; GC handles them).
    let artifacts = sqlx::query("DELETE FROM artifacts WHERE namespace_id = $1")
        .bind(ns_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    sqlx::query("DELETE FROM registry_namespaces WHERE id = $1")
        .bind(ns_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "deleted": true,
        "artifacts_removed": artifacts.rows_affected(),
    }))))
}

async fn admin_delete_repo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((ns_id, repo)): Path<(Uuid, String)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::admin::require_admin_access(&state.db, auth.user_id, "shipyard:admin:registry:manage").await?;

    let result = sqlx::query("DELETE FROM artifacts WHERE namespace_id = $1 AND repo = $2")
        .bind(ns_id)
        .bind(&repo)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "deleted": true,
        "rows": result.rows_affected(),
    }))))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Membership-only check — used only for external registry settings (not artifact storage).
async fn check_org_access(state: &AppState, user_id: Uuid, org_id: Uuid) -> Result<(), ApiAppError> {
    let row = sqlx::query_as::<_, (bool,)>(
        "SELECT EXISTS(SELECT 1 FROM org_members WHERE org_id = $1 AND user_id = $2)",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if !row.0 {
        return Err(ApiAppError(AppError::Forbidden("not a member of this org".into())));
    }
    Ok(())
}

/// Registry-specific access check.
/// `require_manage = false` → `registry:view` or `registry:manage` suffice.
/// `require_manage = true`  → only `registry:manage` is accepted.
/// Owners and admins bypass; superadmins bypass all orgs.
async fn check_registry_access(
    state: &AppState,
    user_id: Uuid,
    org_id: Uuid,
    require_manage: bool,
) -> Result<(), ApiAppError> {
    use crate::middleware::rbac::{get_user_org_role};

    // Superadmin bypass
    let is_super: Option<(bool,)> = sqlx::query_as(
        "SELECT is_superadmin FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    if matches!(is_super, Some((true,))) { return Ok(()); }

    let role = get_user_org_role(&state.db, user_id, org_id).await
        .ok_or_else(|| ApiAppError(AppError::Forbidden("not a member of this org".into())))?;

    if role == "owner" || role == "admin" { return Ok(()); }

    let manage_perm = format!("shipyard:{org_id}:registry:manage");
    let view_perm   = format!("shipyard:{org_id}:registry:view");

    // Check manage perm (satisfies both view and manage requirements)
    let has_manage: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM org_member_permissions
          WHERE org_id = $1 AND user_id = $2 AND permission = $3)",
    )
    .bind(org_id).bind(user_id).bind(&manage_perm)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if has_manage { return Ok(()); }
    if require_manage {
        return Err(ApiAppError(AppError::Forbidden(
            "registry:manage permission required".into(),
        )));
    }

    // Check view perm
    let has_view: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM org_member_permissions
          WHERE org_id = $1 AND user_id = $2 AND permission = $3)",
    )
    .bind(org_id).bind(user_id).bind(&view_perm)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if has_view { Ok(()) } else {
        Err(ApiAppError(AppError::Forbidden(
            "registry:view or registry:manage permission required".into(),
        )))
    }
}
