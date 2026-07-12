use std::collections::HashMap;
use std::path::Path;

use uuid::Uuid;

use shipyard_common::crypto::decrypt_or_passthrough;
use shipyard_common::error::AppError;
use shipyard_engine::edge_fn_detector;

use crate::AppState;

use super::models::{DeployReport, EdgeFunctionGroup, FunctionManifestEntry};
use super::quota::quota_for_tier;

/// Ensure the org's runtime container is running, then POST /reload on it.
pub async fn reload_runtime(state: &AppState, org_id: Uuid) -> Result<(), AppError> {
    let runtime_url = match runtime_url_for_org(state, org_id).await {
        Some(u) => u,
        None => return Ok(()),
    };

    let secret = state
        .config
        .edge_functions
        .runtime_secret
        .as_deref()
        .unwrap_or("");

    let res = state
        .http_client
        .post(format!("{runtime_url}/reload"))
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("runtime reload failed: {e}")))?;

    if !res.status().is_success() {
        return Err(AppError::Internal(format!(
            "runtime reload returned {}",
            res.status()
        )));
    }
    Ok(())
}

/// Resolve the internal URL of the org's runtime container via Swarm DNS.
pub async fn runtime_url_for_org(state: &AppState, org_id: Uuid) -> Option<String> {
    let short = &org_id.to_string()[..8];
    let service_name = format!("shipyard-edge-{short}");
    let tasks = state.docker.list_tasks(&service_name).await.unwrap_or_default();
    if tasks.is_empty() { return None; }
    Some(format!("http://{service_name}:8000"))
}

/// Check that the org has not exceeded its function count quota.
pub async fn check_function_quota(state: &AppState, org_id: Uuid) -> Result<(), AppError> {
    let tier = get_org_tier(state, org_id).await?;
    let quota = quota_for_tier(&tier, &state.config.edge_functions);

    let Some(max) = quota.max_functions else {
        return Ok(());
    };

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edge_functions WHERE org_id = $1 AND status != 'inactive'"
    )
    .bind(org_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    if count >= max as i64 {
        return Err(AppError::BadRequest(format!(
            "function limit reached: {tier} tier allows {max} function(s)"
        )));
    }
    Ok(())
}

/// Deploy a single function: write deployment row, activate, reload runtime.
pub async fn deploy_function(
    state: &AppState,
    fn_id: Uuid,
    code: &str,
    commit_sha: Option<&str>,
    deployed_by: Option<Uuid>,
) -> Result<(), AppError> {
    let org_id: Uuid = sqlx::query_scalar("SELECT org_id FROM edge_functions WHERE id = $1")
        .bind(fn_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let tier = get_org_tier(state, org_id).await?;
    let quota = quota_for_tier(&tier, &state.config.edge_functions);

    let size_kb = code.len() as u64 / 1024;
    if size_kb > quota.max_bundle_kb {
        return Err(AppError::BadRequest(format!(
            "bundle size {size_kb} KB exceeds {tier} tier limit of {} KB",
            quota.max_bundle_kb
        )));
    }

    // Mark previous live deployment as rolled_back
    sqlx::query(
        "UPDATE edge_function_deployments SET status = 'rolled_back'
         WHERE function_id = $1 AND status = 'live'"
    )
    .bind(fn_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Insert new live deployment
    sqlx::query(
        "INSERT INTO edge_function_deployments
         (id, function_id, commit_sha, code_bundle, deployed_by, status)
         VALUES ($1, $2, $3, $4, $5, 'live')"
    )
    .bind(Uuid::new_v4())
    .bind(fn_id)
    .bind(commit_sha)
    .bind(code)
    .bind(deployed_by)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Activate function
    sqlx::query(
        "UPDATE edge_functions
         SET status = 'active', last_deployed_at = NOW(), updated_at = NOW()
         WHERE id = $1"
    )
    .bind(fn_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Ensure the runtime container exists (creates it if missing), then reload.
    // Both are best-effort — a deploy succeeds even if the runtime is temporarily unavailable.
    let _ = crate::edge_functions::runtime_worker::ensure_runtime_exists(state, org_id).await;
    let _ = reload_runtime(state, org_id).await;

    Ok(())
}

/// Deploy all functions detected from a directory on disk.
pub async fn deploy_from_path(
    state: &AppState,
    org_id: Uuid,
    root: &Path,
    group_id: Option<Uuid>,
    commit_sha: Option<&str>,
    deployed_by: Option<Uuid>,
) -> Result<DeployReport, AppError> {
    let detected = edge_fn_detector::detect(root);

    let mut report = DeployReport {
        deployed: vec![],
        skipped: vec![],
        failed: vec![],
        deleted: vec![],
    };

    for f in detected {
        let fn_id: Uuid = sqlx::query_scalar(
            "INSERT INTO edge_functions (id, org_id, group_id, name, runtime, file_path, timeout_secs)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (org_id, name) DO UPDATE
               SET group_id  = EXCLUDED.group_id,
                   file_path = EXCLUDED.file_path,
                   runtime   = EXCLUDED.runtime,
                   updated_at = NOW()
             RETURNING id"
        )
        .bind(Uuid::new_v4())
        .bind(org_id)
        .bind(group_id)
        .bind(&f.name)
        .bind(&f.runtime)
        .bind(&f.file_path)
        .bind(f.timeout_secs as i32)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        match deploy_function(state, fn_id, &f.code, commit_sha, deployed_by).await {
            Ok(_) => report.deployed.push(f.name),
            Err(e) => report.failed.push((f.name, e.to_string())),
        }
    }

    Ok(report)
}

/// Deploy from a linked git group: clone, detect, deploy.
/// Always resolves the actual HEAD commit SHA from the cloned repo so the
/// canvas never shows the literal string "manual".
pub async fn deploy_from_git(
    state: &AppState,
    group: &EdgeFunctionGroup,
    fallback_sha: &str,
    deployed_by: Option<Uuid>,
) -> Result<DeployReport, AppError> {
    use shipyard_git::GitService;

    let tmp = tempfile::tempdir()
        .map_err(|e| AppError::Internal(format!("tempdir: {e}")))?;

    let tmp_path = tmp.path().to_string_lossy().to_string();
    let repo_url = group.repo_url.clone();
    let branch = group.branch.clone();
    let tmp_path2 = tmp_path.clone();
    let fallback_sha_owned = fallback_sha.to_string();

    // Clone and read the HEAD SHA in the same blocking task.
    let actual_sha = tokio::task::spawn_blocking(move || -> Result<String, AppError> {
        let git = GitService::new(&tmp_path);
        git.clone_repo(&repo_url, &tmp_path, Some(&branch), |_| {})
            .map_err(|e| AppError::Git(e.to_string()))?;
        let sha = git
            .head_commit(&tmp_path2)
            .map(|c| c.sha)
            .unwrap_or(fallback_sha_owned);
        Ok(sha)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking: {e}")))??;

    let report = deploy_from_path(
        state,
        group.org_id,
        tmp.path(),
        Some(group.id),
        Some(&actual_sha),
        deployed_by,
    )
    .await?;

    sqlx::query("UPDATE edge_function_groups SET last_deployed_sha = $1 WHERE id = $2")
        .bind(&actual_sha)
        .bind(group.id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(report)
}

/// Build the manifest JSON for the runtime container.
pub async fn build_manifest(
    state: &AppState,
    org_id: Uuid,
) -> Result<Vec<FunctionManifestEntry>, AppError> {
    let secret_key = &state.config.auth.secret_key;

    #[derive(sqlx::FromRow)]
    struct Row {
        name: String,
        code_bundle: Option<String>,
        env_vars: serde_json::Value,
        env_whitelist: Vec<String>,
        timeout_secs: i32,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"SELECT ef.name,
                  efd.code_bundle,
                  ef.env_vars,
                  ef.env_whitelist,
                  ef.timeout_secs
           FROM edge_functions ef
           LEFT JOIN edge_function_deployments efd
             ON efd.function_id = ef.id AND efd.status = 'live'
           WHERE ef.org_id = $1 AND ef.status = 'active'"#
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(code) = row.code_bundle else { continue };

        let all_env: HashMap<String, String> = row
            .env_vars
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| {
                        let enc = v.as_str()?;
                        Some((k.clone(), decrypt_or_passthrough(secret_key, enc)))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let env = if row.env_whitelist.is_empty() {
            all_env
        } else {
            all_env
                .into_iter()
                .filter(|(k, _)| row.env_whitelist.contains(k))
                .collect()
        };

        entries.push(FunctionManifestEntry {
            name: row.name,
            code,
            env,
            timeout_secs: row.timeout_secs,
        });
    }

    Ok(entries)
}

async fn get_org_tier(state: &AppState, org_id: Uuid) -> Result<String, AppError> {
    let tier: Option<String> =
        sqlx::query_scalar("SELECT tier::text FROM org_billing WHERE org_id = $1")
            .bind(org_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(tier.unwrap_or_else(|| "free".to_string()))
}
