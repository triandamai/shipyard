use std::collections::HashMap;
use std::path::Path;

use uuid::Uuid;

use shipyard_common::crypto::decrypt_or_passthrough;
use shipyard_common::error::AppError;
use shipyard_engine::artifact::{ArtifactStore, EdgeArtifact};
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
#[allow(dead_code)]
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

/// Deploy a single function: write artifact to disk, update symlink, record deployment row.
pub async fn deploy_function(
    state: &AppState,
    fn_id: Uuid,
    filename: &str,
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

    // Pre-generate deploy_id so we can use it for both the version dir and the DB row.
    let deploy_id = Uuid::new_v4();

    // Write files to disk and swap the `current` symlink (blocking IO → spawn_blocking).
    let data_dir = state.config.data_dir.clone();
    let filename_owned = filename.to_string();
    let code_owned = code.to_string();
    let artifact_path = tokio::task::spawn_blocking(move || -> Result<String, AppError> {
        let store = EdgeArtifact::new(&data_dir);
        let files = vec![(filename_owned, code_owned)];
        let version_dir = store.write_version(fn_id, deploy_id, &files)?;
        store.update_current(fn_id, &version_dir)?;
        Ok(store.current_path(fn_id).to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking: {e}")))??;

    // Push edge function bundle to registry (best-effort, non-fatal).
    {
        let pusher = shipyard_registry::push::ArtifactPusher::new(
            state.db.clone(),
            std::sync::Arc::clone(&state.registry_storage),
        );
        let bundle = axum::body::Bytes::from(code.as_bytes().to_vec());
        let metadata = serde_json::json!({ "runtime": "js", "entry_point": filename });
        if let Err(e) = pusher.push_edge_function(
            org_id,
            org_id, // use org_id as project_id placeholder for edge functions
            fn_id.to_string().as_str(),
            deploy_id.to_string().as_str(),
            bundle,
            metadata,
        ).await {
            tracing::warn!(fn_id = %fn_id, "edge function registry push failed (non-fatal): {e}");
        }
    }

    // Mark previous live deployment as rolled_back.
    sqlx::query(
        "UPDATE edge_function_deployments SET status = 'rolled_back'
         WHERE function_id = $1 AND status = 'live'"
    )
    .bind(fn_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Insert new deployment row — code_bundle left NULL, artifact_path points to current symlink.
    sqlx::query(
        "INSERT INTO edge_function_deployments
         (id, function_id, commit_sha, artifact_path, deployed_by, status)
         VALUES ($1, $2, $3, $4, $5, 'live')"
    )
    .bind(deploy_id)
    .bind(fn_id)
    .bind(commit_sha)
    .bind(&artifact_path)
    .bind(deployed_by)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Activate function.
    sqlx::query(
        "UPDATE edge_functions
         SET status = 'active', last_deployed_at = NOW(), updated_at = NOW()
         WHERE id = $1"
    )
    .bind(fn_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Prune old version dirs (non-fatal — keep the configured retention count).
    let data_dir_prune = state.config.data_dir.clone();
    let retention = state.config.edge_functions.retention_versions;
    tokio::task::spawn_blocking(move || {
        let store = EdgeArtifact::new(&data_dir_prune);
        let _ = store.prune(fn_id, retention);
    })
    .await
    .ok();

    Ok(())
}

/// Deploy all functions detected from a directory on disk.
/// Starts/reloads the runtime ONCE after all functions are written — avoids
/// concurrent Docker "create service" conflicts when there are multiple functions.
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

        let filename = Path::new(&f.file_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.ts", f.name));
        match deploy_function(state, fn_id, &filename, &f.code, commit_sha, deployed_by).await {
            Ok(_) => report.deployed.push(f.name),
            Err(e) => report.failed.push((f.name, e.to_string())),
        }
    }

    // Ensure the runtime exists once for the whole batch, then reload it.
    if !report.deployed.is_empty() {
        match crate::edge_functions::runtime_worker::ensure_runtime_exists(state, org_id).await {
            Ok(newly_created) if newly_created => {
                // New container — wait for it to be ready before sending reload
                // so the POST actually reaches Deno instead of failing silently.
                crate::edge_functions::runtime_worker::wait_for_runtime_ready(state, org_id).await;
            }
            Err(e) => tracing::warn!("edge runtime start failed for org {org_id}: {e}"),
            _ => {}
        }
        let _ = reload_runtime(state, org_id).await;
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

    // Build authenticated URL from linked git provider if available.
    let authenticated_url = if let Some(prov_id) = group.git_provider_id {
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT provider_type, token FROM git_providers WHERE id = $1"
        )
        .bind(prov_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        if let Some((provider_type, token)) = row {
            if repo_url.starts_with("https://") {
                let domain = repo_url.strip_prefix("https://").unwrap_or(&repo_url);
                let domain = if let Some(idx) = domain.find('@') { &domain[idx + 1..] } else { domain };
                match provider_type.as_str() {
                    "github"    => format!("https://x-access-token:{}@{}", token, domain),
                    "gitlab"    => format!("https://oauth2:{}@{}", token, domain),
                    "bitbucket" => format!("https://x-token-auth:{}@{}", token, domain),
                    _           => format!("https://{}@{}", token, domain),
                }
            } else { repo_url.clone() }
        } else { repo_url.clone() }
    } else { repo_url.clone() };

    // Clone and read the HEAD SHA in the same blocking task.
    let actual_sha = tokio::task::spawn_blocking(move || -> Result<String, AppError> {
        let git = GitService::new(&tmp_path);
        git.clone_repo(&authenticated_url, &tmp_path, Some(&branch), |_| {})
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
        artifact_path: Option<String>,
        code_bundle: Option<String>,
        env_vars: serde_json::Value,
        env_whitelist: Vec<String>,
        timeout_secs: i32,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"SELECT ef.name,
                  efd.artifact_path,
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
        // Skip rows with neither artifact_path nor code_bundle (undeployed).
        if row.artifact_path.is_none() && row.code_bundle.is_none() {
            continue;
        }

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
            artifact_path: row.artifact_path,
            code: row.code_bundle,
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
