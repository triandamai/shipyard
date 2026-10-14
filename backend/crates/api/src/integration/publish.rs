//! Publishing an app environment from source.
//!
//! `POST /orgs/:org/apps/:app/envs/:env/publish` (multipart):
//! - `manifest`: JSON [`Manifest`] describing services, env vars, database
//!   and domain.
//! - `source`: optional zip of the app. Each service's `path` inside it is
//!   committed to a Shipyard-managed git repo and built by the normal git
//!   pipeline (Dockerfile, or auto-detected SvelteKit/Next/Nuxt/...). Omit
//!   `source` to re-apply a manifest (e.g. new env vars) to the last upload.
//!
//! The call returns once everything is configured; builds and deploys run in
//! the background, database first, and report through `publish.*` and
//! `deployment.*` events and `GET .../publishes/:id`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::crypto;
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::events::{self, Scope};
use shipyard_engine::DeploymentEngine;

use super::secrets::{bind_env, find_secret, generate, upsert_secret, validate_env_name, Generate};
use super::{
    app_ref, db_err, env_project, random_alnum, validate_slug, AppRef, IntegrationCaller,
    SCOPE_MANAGE,
};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/orgs/:org/apps/:app/envs/:env/publish",
            // The source zip can be large; the real limit is checked while streaming.
            post(publish_route).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/orgs/:org/apps/:app/envs/:env/publishes/:publish_id",
            get(get_publish),
        )
        .route(
            "/orgs/:org/apps/:app/envs/:env/duplicate",
            post(duplicate_route),
        )
}

// ─── Manifest ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub services: Vec<ServiceIn>,
    #[serde(default)]
    pub database: Option<DatabaseIn>,
    #[serde(default)]
    pub domain: Option<DomainIn>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    /// Serves HTTP on `port`; gets the app's public URL.
    Web,
    /// Background process, no public URL.
    Worker,
    /// Static site served by static nginx.
    Static,
}

fn default_kind() -> ServiceKind {
    ServiceKind::Web
}
fn default_path() -> String {
    ".".to_string()
}
fn default_replicas() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceIn {
    /// Service name, `a-z0-9-`. Stable across publishes: a service is
    /// updated, not recreated, when its name stays the same.
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: ServiceKind,
    /// Directory inside the source zip holding this service (monorepos).
    #[serde(default = "default_path")]
    pub path: String,
    /// Dockerfile relative to `path`. Omit to use `Dockerfile` if present,
    /// otherwise framework auto-detection.
    #[serde(default)]
    pub dockerfile: Option<String>,
    /// Port the app listens on. Required for `web`.
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default = "default_replicas")]
    pub replicas: u32,
    #[serde(default)]
    pub cpu: Option<f64>,
    #[serde(default)]
    pub memory_mb: Option<i64>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvValue>,
    /// Paths whose data must survive deploys.
    #[serde(default)]
    pub volumes: Vec<VolumeIn>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeIn {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EnvValue {
    Plain(String),
    /// Generated once per environment and kept across publishes.
    Generate {
        generate: Generate,
    },
    /// A secret set with `PUT .../secrets/:name` (env-specific wins over app-wide).
    Secret {
        secret: String,
    },
    /// A field of the environment's database.
    Database {
        database: DbField,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DbField {
    Url,
    Host,
    Port,
    User,
    Password,
    Name,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseIn {
    /// Only "postgres" today.
    #[serde(default = "default_engine")]
    pub engine: String,
}

fn default_engine() -> String {
    "postgres".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DomainIn {
    /// First label of the generated hostname. Defaults to `<app>` for
    /// production and `<app>-<env>` otherwise.
    #[serde(default)]
    pub subdomain: Option<String>,
    /// Which web service gets the domain. Defaults to the first web service.
    #[serde(default)]
    pub service: Option<String>,
}

pub const DB_SERVICE: &str = "db";
const DB_USER: &str = "app";
const DB_NAME: &str = "app";

/// Check a manifest without touching anything.
pub fn validate_manifest(m: &Manifest) -> Result<(), ApiAppError> {
    let bad = |msg: String| Err(ApiAppError(AppError::BadRequest(msg)));
    if m.services.is_empty() {
        return bad("manifest.services must not be empty".into());
    }
    if m.services.len() > 20 {
        return bad("at most 20 services per environment".into());
    }
    if let Some(db) = &m.database {
        if db.engine != "postgres" {
            return bad(format!(
                "database.engine '{}' is not supported (use 'postgres')",
                db.engine
            ));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for s in &m.services {
        validate_slug("service name", &s.name)?;
        if s.name == DB_SERVICE {
            return bad(format!(
                "service name '{DB_SERVICE}' is reserved for the database"
            ));
        }
        if !seen.insert(&s.name) {
            return bad(format!("duplicate service name '{}'", s.name));
        }
        if s.kind == ServiceKind::Web && s.port.is_none() && s.dockerfile.is_none() {
            return bad(format!("web service '{}' needs a port", s.name));
        }
        if !(1..=20).contains(&s.replicas) {
            return bad(format!("service '{}': replicas must be 1-20", s.name));
        }
        validate_rel_path(&format!("service '{}' path", s.name), &s.path)?;
        if let Some(df) = &s.dockerfile {
            validate_rel_path(&format!("service '{}' dockerfile", s.name), df)?;
        }
        for v in &s.volumes {
            if !v.path.starts_with('/') || v.path.contains("..") || v.path.len() > 200 {
                return bad(format!(
                    "service '{}': volume path '{}' must be absolute",
                    s.name, v.path
                ));
            }
        }
        for (key, value) in &s.env {
            validate_env_name(key)?;
            match value {
                EnvValue::Plain(v) if v.len() > 32 * 1024 => {
                    return bad(format!("{}: value of {key} is over 32 KB", s.name))
                }
                EnvValue::Secret { secret } => validate_env_name(secret)?,
                EnvValue::Database { .. } if m.database.is_none() => {
                    return bad(format!(
                        "{}: {key} refers to the database but manifest.database is not set",
                        s.name
                    ))
                }
                _ => {}
            }
        }
    }
    if let Some(d) = &m.domain {
        if let Some(sub) = &d.subdomain {
            validate_slug("domain.subdomain", sub)?;
        }
        if let Some(svc) = &d.service {
            match m.services.iter().find(|s| &s.name == svc) {
                Some(s) if s.kind == ServiceKind::Web || s.kind == ServiceKind::Static => {}
                _ => {
                    return bad(format!(
                        "domain.service '{svc}' is not a web or static service in this manifest"
                    ))
                }
            }
        }
    }
    Ok(())
}

/// Relative path inside the source: no `..`, not absolute, sane length.
fn validate_rel_path(what: &str, p: &str) -> Result<(), ApiAppError> {
    let ok = !p.is_empty()
        && p.len() <= 200
        && !p.starts_with('/')
        && !p.contains('\\')
        && p.split('/').all(|seg| seg != "..");
    if ok {
        Ok(())
    } else {
        Err(ApiAppError(AppError::BadRequest(format!(
            "{what} '{p}' must be a relative path without '..'"
        ))))
    }
}

/// Hostname label for an environment.
pub fn default_subdomain(app_key: &str, environment: &str) -> String {
    if environment == "production" {
        app_key.to_string()
    } else {
        let label = format!("{app_key}-{environment}");
        label
            .chars()
            .take(63)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string()
    }
}

// ─── Source handling ─────────────────────────────────────────────────────────

pub enum Source {
    /// Extracted upload directory.
    Upload(PathBuf),
    /// Reuse the repos of another environment (duplicate).
    FromProject(Uuid),
    /// Keep each service's current repo.
    Unchanged,
}

pub fn source_repo_dir(data_dir: &str, service_id: Uuid) -> String {
    shipyard_engine::managed_source_repo(data_dir, service_id)
}

/// Extract a zip safely: no paths outside `dest`, no symlinks, no `.git`
/// directories, bounded size and file count. A single top-level directory is
/// treated as the root (archives made with `zip -r app.zip app/`).
pub fn extract_zip(zip_path: &FsPath, dest: &FsPath, max_bytes: u64) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("not a valid zip: {e}"))?;
    if archive.len() > 100_000 {
        return Err("zip has more than 100000 entries".into());
    }

    // Detect a single top-level directory to strip.
    let mut tops = std::collections::BTreeSet::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if let Some(p) = entry.enclosed_name() {
            if let Some(first) = p.components().next() {
                tops.insert(first.as_os_str().to_owned());
            }
        }
    }
    let strip = if tops.len() == 1 {
        let only = tops.into_iter().next().unwrap_or_default();
        // Strip it only if it's a directory (some entry lives under it).
        let mut is_dir = false;
        for i in 0..archive.len() {
            let entry = archive.by_index(i).map_err(|e| e.to_string())?;
            if let Some(p) = entry.enclosed_name() {
                if p.components().count() > 1 {
                    is_dir = true;
                    break;
                }
            }
        }
        if is_dir {
            Some(PathBuf::from(only))
        } else {
            None
        }
    } else {
        None
    };

    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else {
            return Err(format!("unsafe path in zip: {}", entry.name()));
        };
        let rel = match &strip {
            Some(prefix) => match name.strip_prefix(prefix) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            },
            None => name.to_path_buf(),
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        if rel.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }
        // Symlinks could point outside the build context; skip them.
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            continue;
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        let declared = entry.size();
        total = total.saturating_add(declared);
        if total > max_bytes {
            return Err(format!(
                "source is larger than {} MB uncompressed",
                max_bytes / (1024 * 1024)
            ));
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        // Bound the actual bytes written too, in case the header lies.
        let unix_mode = entry.unix_mode();
        let mut limited = std::io::Read::take(&mut entry, declared + 1);
        let written = std::io::copy(&mut limited, &mut f).map_err(|e| e.to_string())?;
        if written > declared {
            return Err("zip entry is larger than its header says".into());
        }
        if let Some(mode) = unix_mode {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o755));
        }
    }
    Ok(())
}

async fn git(dir: &str, args: &[&str]) -> Result<String, ApiAppError> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Shipyard",
            "-c",
            "user.email=publish@shipyard.local",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("git: {e}"))))?;
    if !out.status.success() {
        return Err(ApiAppError(AppError::Internal(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Replace the repo's working tree with `src` and commit. Returns the commit.
async fn commit_source(
    repo: &str,
    src: &FsPath,
    dockerfile: Option<&str>,
    message: &str,
) -> Result<String, ApiAppError> {
    let internal = |e: std::io::Error| ApiAppError(AppError::Internal(format!("source copy: {e}")));
    tokio::fs::create_dir_all(repo).await.map_err(internal)?;
    if !FsPath::new(repo).join(".git").exists() {
        git(repo, &["init", "-q", "-b", "main"]).await?;
    }
    // Clear everything except .git, then copy the new tree in.
    let mut rd = tokio::fs::read_dir(repo).await.map_err(internal)?;
    while let Some(entry) = rd.next_entry().await.map_err(internal)? {
        if entry.file_name() == ".git" {
            continue;
        }
        let p = entry.path();
        if entry.file_type().await.map_err(internal)?.is_dir() {
            tokio::fs::remove_dir_all(&p).await.map_err(internal)?;
        } else {
            tokio::fs::remove_file(&p).await.map_err(internal)?;
        }
    }
    let status = tokio::process::Command::new("cp")
        .arg("-a")
        .arg(format!("{}/.", src.display()))
        .arg(repo)
        .status()
        .await
        .map_err(internal)?;
    if !status.success() {
        return Err(ApiAppError(AppError::Internal(
            "copying source failed".into(),
        )));
    }
    // The build looks for ./Dockerfile; put a custom one there.
    if let Some(df) = dockerfile.filter(|d| *d != "Dockerfile") {
        let from = FsPath::new(repo).join(df);
        if !from.exists() {
            return Err(ApiAppError(AppError::BadRequest(format!(
                "dockerfile '{df}' not found in the source"
            ))));
        }
        tokio::fs::copy(&from, FsPath::new(repo).join("Dockerfile"))
            .await
            .map_err(internal)?;
    }
    git(repo, &["add", "-A"]).await?;
    git(repo, &["commit", "-q", "--allow-empty", "-m", message]).await?;
    git(repo, &["rev-parse", "HEAD"]).await
}

// ─── Publish ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PublishOut {
    pub publish_id: Uuid,
    pub app: String,
    pub environment: String,
    pub project_id: Uuid,
    pub status: &'static str,
    pub services: BTreeMap<String, Uuid>,
    /// The public URL once deploys finish (null without an apps domain).
    pub url: Option<String>,
    pub warnings: Vec<String>,
}

fn engine(state: &AppState) -> DeploymentEngine {
    DeploymentEngine::new(
        Arc::clone(&state.docker),
        state.db.clone(),
        Arc::clone(&state.mqtt),
        state.config.docker.label_prefix.clone(),
        state.config.traefik.network.clone(),
        state.config.auth.secret_key.clone(),
        state.config.docker.port_proxy,
        state.config.data_dir.clone(),
        state.config.static_server.retention_versions,
    )
    .with_registry(shipyard_registry::push::ArtifactPusher::new(
        state.db.clone(),
        Arc::clone(&state.registry_storage),
    ))
    .with_registry_hostname(state.config.registry.hostname.clone())
}

/// The environment's project, created on first use.
async fn ensure_env_project(
    state: &AppState,
    app: &AppRef,
    environment: &str,
) -> Result<(Uuid, bool), ApiAppError> {
    if let Ok(id) = env_project(&state.db, app, environment).await {
        return Ok((id, false));
    }
    let app_name: String = sqlx::query_scalar("SELECT name FROM integration_apps WHERE id = $1")
        .bind(app.app_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;
    // Same quota as dashboard projects.
    let (count, limit): (i64, Option<i32>) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM projects WHERE org_id = $1),
                (SELECT max_projects FROM org_quota WHERE org_id = $1)",
    )
    .bind(app.org_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;
    if let Some(max) = limit {
        if max != -1 && count >= max as i64 {
            return Err(ApiAppError(AppError::Forbidden(format!(
                "Plan limit reached: {max} environment(s)/project(s) for this org"
            ))));
        }
    }
    let id = Uuid::now_v7();
    let slug = format!("{}-{}", app.app_key, environment);
    sqlx::query(
        "INSERT INTO projects (id, org_id, name, slug, directory_path, app_id, environment, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())",
    )
    .bind(id)
    .bind(app.org_id)
    .bind(format!("{app_name} ({environment})"))
    .bind(&slug)
    .bind(format!("{}/{}/{}", state.config.data_dir, app.org_id, id))
    .bind(app.app_id)
    .bind(environment)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate") {
            ApiAppError(AppError::Conflict(format!("Environment '{environment}' is being created by another request")))
        } else {
            db_err(e)
        }
    })?;
    events::emit(
        &state.db,
        Scope::project(app.org_id, id),
        "project.created",
        serde_json::json!({ "app": app.app_key, "environment": environment }),
    )
    .await;
    Ok((id, true))
}

/// Find or create a service row by slug.
#[allow(clippy::too_many_arguments)]
async fn upsert_service(
    state: &AppState,
    org_id: Uuid,
    project_id: Uuid,
    slug: &str,
    svc_type: &str,
    image: &str,
    git_repo_url: Option<&str>,
    ports: serde_json::Value,
    replicas: i32,
    cpu: Option<f64>,
    memory_mb: Option<i64>,
) -> Result<Uuid, ApiAppError> {
    if let Some(id) =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM services WHERE project_id = $1 AND slug = $2")
            .bind(project_id)
            .bind(slug)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err)?
    {
        sqlx::query(
            "UPDATE services SET ports = $2, replicas = $3, cpu_limit = $4, memory_limit_mb = $5,
                    git_repo_url = COALESCE($6, git_repo_url), updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(&ports)
        .bind(replicas)
        .bind(cpu)
        .bind(memory_mb)
        .bind(git_repo_url)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
        return Ok(id);
    }
    let id = Uuid::now_v7();
    let git_repo_url = git_repo_url.map(str::to_string).unwrap_or_else(|| {
        if svc_type == "git" {
            source_repo_dir(&state.config.data_dir, id)
        } else {
            String::new()
        }
    });
    sqlx::query(
        "INSERT INTO services (id, project_id, name, slug, type, image, git_repo_url, git_branch, auto_deploy,
                               directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, created_at, updated_at)
         VALUES ($1, $2, $3, $3, $4::service_type, $5, NULLIF($6, ''), 'main', false, $7, $8, 'stopped', $9, $10, $11, NOW(), NOW())",
    )
    .bind(id)
    .bind(project_id)
    .bind(slug)
    .bind(svc_type)
    .bind(image)
    .bind(&git_repo_url)
    .bind(format!("{}/{}/{}/{}", state.config.data_dir, project_id, id, slug))
    .bind(&ports)
    .bind(replicas)
    .bind(cpu)
    .bind(memory_mb)
    .execute(&state.db)
    .await
    .map_err(db_err)?;
    events::emit(
        &state.db,
        Scope::service(org_id, project_id, id),
        "service.created",
        serde_json::json!({ "slug": slug, "type": svc_type }),
    )
    .await;
    Ok(id)
}

async fn set_plain_env(
    state: &AppState,
    service_id: Uuid,
    key: &str,
    value: &str,
) -> Result<(), ApiAppError> {
    let enc = crypto::encrypt(&state.config.auth.secret_key, value).map_err(ApiAppError)?;
    sqlx::query(
        "INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
         VALUES ($1, $2, $3, $4, FALSE, NOW())
         ON CONFLICT (service_id, key) DO UPDATE SET value_encrypted = EXCLUDED.value_encrypted, is_secret = FALSE",
    )
    .bind(Uuid::now_v7())
    .bind(service_id)
    .bind(key)
    .bind(&enc)
    .execute(&state.db)
    .await
    .map_err(db_err)?;
    sqlx::query("DELETE FROM secret_bindings WHERE service_id = $1 AND env_key = $2")
        .bind(service_id)
        .bind(key)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    Ok(())
}

async fn ensure_volume(
    state: &AppState,
    service_id: Uuid,
    project_id: Uuid,
    path: &str,
) -> Result<(), ApiAppError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM volumes WHERE service_id = $1 AND mount_path = $2)",
    )
    .bind(service_id)
    .bind(path)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;
    if exists {
        return Ok(());
    }
    let suffix: String = path
        .trim_matches('/')
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let name = format!(
        "{}-{}-{}",
        state.config.docker.label_prefix,
        service_id,
        if suffix.is_empty() {
            "data".into()
        } else {
            suffix
        }
    );
    sqlx::query(
        "INSERT INTO volumes (id, service_id, project_id, name, mount_path, driver, created_at)
         VALUES ($1, $2, $3, $4, $5, 'local', NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(service_id)
    .bind(project_id)
    .bind(&name)
    .bind(path)
    .execute(&state.db)
    .await
    .map_err(db_err)?;
    events::emit_for_service(
        &state.db,
        service_id,
        "volume.created",
        serde_json::json!({ "volume": name, "mount_path": path }),
    )
    .await;
    Ok(())
}

/// The environment's Postgres service, its password secret and URL secret.
struct DbParts {
    service_id: Uuid,
    host: String,
    password_secret: Uuid,
    url_secret: Uuid,
}

async fn ensure_database(
    state: &AppState,
    app: &AppRef,
    project_id: Uuid,
) -> Result<DbParts, ApiAppError> {
    let cfg = &state.config.integration;
    let service_id = upsert_service(
        state,
        app.org_id,
        project_id,
        DB_SERVICE,
        "docker",
        &cfg.postgres_image,
        None,
        serde_json::json!([]),
        1,
        None,
        None,
    )
    .await?;
    sqlx::query("UPDATE services SET image = $2 WHERE id = $1 AND image = ''")
        .bind(service_id)
        .bind(&cfg.postgres_image)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    let sk = &state.config.auth.secret_key;

    // Password: generated once, kept for the life of the environment.
    let password_secret =
        match find_secret(&state.db, app.app_id, Some(project_id), "DATABASE_PASSWORD").await? {
            Some(id) => id,
            None => {
                events::emit(
                    &state.db,
                    Scope::service(app.org_id, project_id, service_id),
                    "database.provisioned",
                    serde_json::json!({
                        "service": DB_SERVICE,
                        "host": format!("{}-{}", state.config.docker.label_prefix, service_id),
                    }),
                )
                .await;
                upsert_secret(
                    &state.db,
                    sk,
                    app.org_id,
                    Some(app.app_id),
                    Some(project_id),
                    "DATABASE_PASSWORD",
                    "database",
                    &generate(Generate::Password),
                )
                .await?
                .id
            }
        };
    let password = super::secrets::secret_value(&state.db, sk, password_secret).await?;
    let host = format!("{}-{}", state.config.docker.label_prefix, service_id);
    let url = format!("postgres://{DB_USER}:{password}@{host}:5432/{DB_NAME}");
    let url_secret = upsert_secret(
        &state.db,
        sk,
        app.org_id,
        Some(app.app_id),
        Some(project_id),
        "DATABASE_URL",
        "database",
        &url,
    )
    .await?
    .id;

    set_plain_env(state, service_id, "POSTGRES_USER", DB_USER).await?;
    set_plain_env(state, service_id, "POSTGRES_DB", DB_NAME).await?;
    bind_env(&state.db, service_id, "POSTGRES_PASSWORD", password_secret).await?;
    ensure_volume(state, service_id, project_id, "/var/lib/postgresql/data").await?;

    Ok(DbParts {
        service_id,
        host,
        password_secret,
        url_secret,
    })
}

/// Attach `<subdomain>.<apps_base_domain>` to the web service unless it
/// already has a hostname there. Falls back to a random suffix when taken.
async fn ensure_domain(
    state: &AppState,
    service_id: Uuid,
    subdomain: &str,
    port: u16,
) -> Result<Option<String>, ApiAppError> {
    let Some(base) = state.config.integration.apps_base_domain.as_deref() else {
        return Ok(None);
    };
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT hostname FROM domains WHERE service_id = $1 AND hostname LIKE '%.' || $2 LIMIT 1",
    )
    .bind(service_id)
    .bind(base)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    if let Some(h) = existing {
        return Ok(Some(format!("https://{h}")));
    }
    let mut candidates = vec![format!("{subdomain}.{base}")];
    for _ in 0..3 {
        candidates.push(format!(
            "{}-{}.{base}",
            subdomain.chars().take(52).collect::<String>(),
            random_alnum(8)
        ));
    }
    for hostname in candidates {
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM domains WHERE hostname = $1)")
                .bind(&hostname)
                .fetch_one(&state.db)
                .await
                .map_err(db_err)?;
        if taken {
            continue;
        }
        match crate::resources::attach_domain(
            state,
            service_id,
            &hostname,
            true,
            "letsencrypt",
            Some(port as i32),
        )
        .await
        {
            Ok(d) => return Ok(Some(format!("https://{}", d.hostname))),
            Err(ApiAppError(AppError::Conflict(_))) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(ApiAppError(AppError::Conflict(
        "Could not find a free hostname".into(),
    )))
}

/// Configure an environment from a manifest and start its deploys.
pub async fn publish(
    state: &AppState,
    app: &AppRef,
    environment: &str,
    manifest: Manifest,
    source: Source,
) -> Result<PublishOut, ApiAppError> {
    validate_manifest(&manifest)?;
    validate_slug("environment", environment)?;
    let (project_id, _) = ensure_env_project(state, app, environment).await?;
    let publish_id = Uuid::now_v7();
    let mut warnings = vec![];

    // Quota: replicas per service.
    if let Some(max) =
        sqlx::query_scalar::<_, i32>("SELECT max_replicas FROM org_quota WHERE org_id = $1")
            .bind(app.org_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err)?
    {
        if let Some(s) = manifest
            .services
            .iter()
            .find(|s| max != -1 && s.replicas as i32 > max)
        {
            return Err(ApiAppError(AppError::Forbidden(format!(
                "Plan limit reached: service '{}' asks for {} replicas, the plan allows {max}",
                s.name, s.replicas
            ))));
        }
    }

    // Secrets the manifest needs must exist before anything changes.
    let mut missing = vec![];
    for s in &manifest.services {
        for value in s.env.values() {
            if let EnvValue::Secret { secret } = value {
                if find_secret(&state.db, app.app_id, Some(project_id), secret)
                    .await?
                    .is_none()
                {
                    missing.push(secret.clone());
                }
            }
        }
    }
    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        return Err(ApiAppError(AppError::BadRequest(format!(
            "Missing secrets: {}. Set them with PUT /orgs/{{org}}/apps/{}/secrets/{{name}} first.",
            missing.join(", "),
            app.app_key
        ))));
    }

    sqlx::query("INSERT INTO publishes (id, org_id, project_id, manifest) VALUES ($1, $2, $3, $4)")
        .bind(publish_id)
        .bind(app.org_id)
        .bind(project_id)
        .bind(serde_json::to_value(&manifest).unwrap_or_default())
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    events::emit(
        &state.db,
        Scope::project(app.org_id, project_id),
        "publish.started",
        serde_json::json!({ "publish_id": publish_id, "services": manifest.services.iter().map(|s| &s.name).collect::<Vec<_>>() }),
    )
    .await;

    let result = configure(
        state,
        app,
        environment,
        project_id,
        publish_id,
        &manifest,
        &source,
        &mut warnings,
    )
    .await;
    let (services, url, db_service) = match result {
        Ok(v) => v,
        Err(e) => {
            finish_publish(
                state,
                app.org_id,
                project_id,
                publish_id,
                Err(e.0.to_string()),
                vec![],
                None,
            )
            .await;
            return Err(e);
        }
    };

    // Build and deploy in the background: database first, then services in
    // manifest order.
    let mut order: Vec<(String, Uuid)> = vec![];
    if let Some(db) = db_service {
        let deployed_before: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM deployments WHERE service_id = $1 AND status = 'success')",
        )
        .bind(db)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;
        // Restarting Postgres on every publish would drop connections for nothing.
        if !deployed_before {
            order.push((DB_SERVICE.to_string(), db));
        }
    }
    for s in &manifest.services {
        order.push((s.name.clone(), services[&s.name]));
    }
    let bg_state = state.clone();
    let org_id = app.org_id;
    let url_bg = url.clone();
    tokio::spawn(async move {
        run_deploys(bg_state, org_id, project_id, publish_id, order, url_bg).await;
    });

    Ok(PublishOut {
        publish_id,
        app: app.app_key.clone(),
        environment: environment.to_string(),
        project_id,
        status: "running",
        services,
        url,
        warnings,
    })
}

#[allow(clippy::too_many_arguments)]
async fn configure(
    state: &AppState,
    app: &AppRef,
    environment: &str,
    project_id: Uuid,
    publish_id: Uuid,
    manifest: &Manifest,
    source: &Source,
    warnings: &mut Vec<String>,
) -> Result<(BTreeMap<String, Uuid>, Option<String>, Option<Uuid>), ApiAppError> {
    let sk = &state.config.auth.secret_key;

    let db = match &manifest.database {
        Some(_) => Some(ensure_database(state, app, project_id).await?),
        None => {
            let has_db: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM services WHERE project_id = $1 AND slug = $2)",
            )
            .bind(project_id)
            .bind(DB_SERVICE)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;
            if has_db {
                warnings.push("manifest has no database, but this environment has one; it was left running with its data".into());
            }
            None
        }
    };

    let mut services = BTreeMap::new();
    let mut detected_ports = HashMap::new();
    let mut is_static_map = HashMap::new();

    for s in &manifest.services {
        let maybe_src = match source {
            Source::Upload(root) => {
                let src = if s.path == "." {
                    root.clone()
                } else {
                    root.join(&s.path)
                };
                if src.is_dir() {
                    Some(src)
                } else {
                    None
                }
            }
            _ => None,
        };

        // Static site detection: explicit kind or auto-detected as static site
        let is_static = if s.kind == ServiceKind::Static {
            true
        } else if s.kind == ServiceKind::Worker {
            false
        } else if let Some(src) = &maybe_src {
            let has_dockerfile = s
                .dockerfile
                .as_ref()
                .map(|df| src.join(df).exists())
                .unwrap_or_else(|| src.join("Dockerfile").exists());
            if !has_dockerfile && shipyard_engine::ssr::detect_ssr_framework(src).is_none() {
                src.join("index.html").exists()
                    || shipyard_engine::static_site::detect_build_config(src).is_some()
            } else {
                false
            }
        } else {
            false
        };
        is_static_map.insert(s.name.clone(), is_static);

        let svc_type = if is_static { "static" } else { "git" };

        let detected_port = if is_static {
            None
        } else if let Some(p) = s.port {
            Some(p)
        } else if let Some(src) = &maybe_src {
            let df_content = if let Some(df) = &s.dockerfile {
                std::fs::read_to_string(src.join(df)).ok()
            } else if src.join("Dockerfile").exists() {
                std::fs::read_to_string(src.join("Dockerfile")).ok()
            } else {
                None
            };
            if let Some(content) = df_content {
                shipyard_engine::detect_dockerfile_port(&content)
            } else if let Some(ssr) = shipyard_engine::ssr::detect_ssr_framework(src) {
                Some(ssr.default_port)
            } else if s.kind == ServiceKind::Web {
                Some(3000)
            } else {
                None
            }
        } else if s.kind == ServiceKind::Web {
            Some(3000)
        } else {
            None
        };

        if let Some(p) = detected_port {
            detected_ports.insert(s.name.clone(), p);
        }

        let ports = match detected_port {
            Some(p) => serde_json::json!([p.to_string()]),
            None => serde_json::json!([]),
        };

        let id = upsert_service(
            state,
            app.org_id,
            project_id,
            &s.name,
            svc_type,
            "",
            None,
            ports,
            s.replicas as i32,
            s.cpu,
            s.memory_mb,
        )
        .await?;

        if is_static {
            let _ = sqlx::query(
                "INSERT INTO static_site_configs (service_id, source, framework, build_command, output_dir, install_command, node_version)
                 VALUES ($1, 'git', 'custom', 'bun run build', 'dist', 'bun install', '1')
                 ON CONFLICT (service_id) DO NOTHING",
            )
            .bind(id)
            .execute(&state.db)
            .await;
        }

        let repo = source_repo_dir(&state.config.data_dir, id);
        // Point the service at its managed repo (also fixes services created before).
        sqlx::query("UPDATE services SET git_repo_url = $2, git_branch = 'main' WHERE id = $1")
            .bind(id)
            .bind(&repo)
            .execute(&state.db)
            .await
            .map_err(db_err)?;

        match source {
            Source::Upload(root) => {
                let src = if s.path == "." {
                    root.clone()
                } else {
                    root.join(&s.path)
                };
                if !src.is_dir() {
                    return Err(ApiAppError(AppError::BadRequest(format!(
                        "service '{}': path '{}' is not a directory in the source",
                        s.name, s.path
                    ))));
                }
                commit_source(
                    &repo,
                    &src,
                    s.dockerfile.as_deref(),
                    &format!("Publish {publish_id}"),
                )
                .await?;
            }
            Source::FromProject(src_project) => {
                let src_service: Option<Uuid> = sqlx::query_scalar(
                    "SELECT id FROM services WHERE project_id = $1 AND slug = $2",
                )
                .bind(src_project)
                .bind(&s.name)
                .fetch_optional(&state.db)
                .await
                .map_err(db_err)?;
                let src_repo = src_service.map(|sid| source_repo_dir(&state.config.data_dir, sid));
                match src_repo.filter(|r| FsPath::new(r).join(".git").exists()) {
                    Some(src_repo) => {
                        let _ = tokio::fs::remove_dir_all(&repo).await;
                        let status = tokio::process::Command::new("cp")
                            .args(["-a", &src_repo, &repo])
                            .status()
                            .await
                            .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
                        if !status.success() {
                            return Err(ApiAppError(AppError::Internal(
                                "copying source repo failed".into(),
                            )));
                        }
                    }
                    None => {
                        return Err(ApiAppError(AppError::BadRequest(format!(
                            "service '{}' has no published source in the environment being copied",
                            s.name
                        ))))
                    }
                }
            }
            Source::Unchanged => {
                if !FsPath::new(&repo).join(".git").exists() {
                    return Err(ApiAppError(AppError::BadRequest(format!(
                        "service '{}' has no source yet; include 'source' in this publish",
                        s.name
                    ))));
                }
            }
        }

        // Env vars: the manifest owns the service's env.
        let keep: Vec<String> = s.env.keys().cloned().collect();
        sqlx::query("DELETE FROM service_envs WHERE service_id = $1 AND NOT (key = ANY($2)) AND key <> '__IMAGE__'")
            .bind(id)
            .bind(&keep)
            .execute(&state.db)
            .await
            .map_err(db_err)?;
        sqlx::query(
            "DELETE FROM secret_bindings WHERE service_id = $1 AND NOT (env_key = ANY($2))",
        )
        .bind(id)
        .bind(&keep)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
        for (key, value) in &s.env {
            match value {
                EnvValue::Plain(v) => set_plain_env(state, id, key, v).await?,
                EnvValue::Generate { generate: kind } => {
                    let secret =
                        match find_secret(&state.db, app.app_id, Some(project_id), key).await? {
                            Some(sid) => sid,
                            None => {
                                upsert_secret(
                                    &state.db,
                                    sk,
                                    app.org_id,
                                    Some(app.app_id),
                                    Some(project_id),
                                    key,
                                    "generated",
                                    &generate(*kind),
                                )
                                .await?
                                .id
                            }
                        };
                    bind_env(&state.db, id, key, secret).await?;
                }
                EnvValue::Secret { secret } => {
                    let sid = find_secret(&state.db, app.app_id, Some(project_id), secret)
                        .await?
                        .ok_or_else(|| {
                            ApiAppError(AppError::BadRequest(format!("Missing secret {secret}")))
                        })?;
                    bind_env(&state.db, id, key, sid).await?;
                }
                EnvValue::Database { database: field } => {
                    let parts = db
                        .as_ref()
                        .ok_or_else(|| ApiAppError(AppError::BadRequest("no database".into())))?;
                    match field {
                        DbField::Url => bind_env(&state.db, id, key, parts.url_secret).await?,
                        DbField::Password => {
                            bind_env(&state.db, id, key, parts.password_secret).await?
                        }
                        DbField::Host => set_plain_env(state, id, key, &parts.host).await?,
                        DbField::Port => set_plain_env(state, id, key, "5432").await?,
                        DbField::User => set_plain_env(state, id, key, DB_USER).await?,
                        DbField::Name => set_plain_env(state, id, key, DB_NAME).await?,
                    }
                }
            }
        }

        for v in &s.volumes {
            ensure_volume(state, id, project_id, &v.path).await?;
        }
        services.insert(s.name.clone(), id);
    }

    // Domain on the chosen web service.
    let domain_cfg = manifest.domain.clone().unwrap_or_default();
    let web = match &domain_cfg.service {
        Some(name) => manifest.services.iter().find(|s| &s.name == name),
        None => manifest
            .services
            .iter()
            .find(|s| s.kind == ServiceKind::Web || s.kind == ServiceKind::Static),
    };
    let url = match web {
        Some(w) => {
            let sub = domain_cfg
                .subdomain
                .clone()
                .unwrap_or_else(|| default_subdomain(&app.app_key, environment));
            let port = if w.kind == ServiceKind::Static || is_static_map.get(&w.name).copied().unwrap_or(false) {
                80
            } else {
                detected_ports.get(&w.name).copied().or(w.port).unwrap_or(3000)
            };
            let url = ensure_domain(state, services[&w.name], &sub, port).await?;
            if url.is_none() {
                warnings
                    .push("no apps base domain is configured, so the app has no public URL".into());
            }
            url
        }
        None => None,
    };

    Ok((services, url, db.map(|d| d.service_id)))
}

async fn run_deploys(
    state: AppState,
    org_id: Uuid,
    project_id: Uuid,
    publish_id: Uuid,
    order: Vec<(String, Uuid)>,
    url: Option<String>,
) {
    let mut deployments = vec![];
    let mut failure: Option<String> = None;
    for (name, service_id) in order {
        let deploy_started = chrono::Utc::now();
        let deployment_id = Uuid::now_v7();
        let inserted = sqlx::query(
            "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
             VALUES ($1, $2, $3, 'manual', 'running'::deployment_status, NOW())",
        )
        .bind(deployment_id)
        .bind(service_id)
        .bind(format!("publish:{publish_id}"))
        .execute(&state.db)
        .await;
        if let Err(e) = inserted {
            failure = Some(format!("{name}: {e}"));
            break;
        }
        deployments.push(serde_json::json!({ "service": name, "service_id": service_id, "deployment_id": deployment_id }));
        let _ = sqlx::query("UPDATE publishes SET deployments = $2 WHERE id = $1")
            .bind(publish_id)
            .bind(serde_json::Value::Array(deployments.clone()))
            .execute(&state.db)
            .await;

        let result = engine(&state)
            .deploy(
                deployment_id,
                service_id,
                &format!("publish:{publish_id}"),
                "manual",
            )
            .await;
        state.swarm_sync_trigger.notify_one();
        let status: Option<String> =
            sqlx::query_scalar("SELECT status::text FROM deployments WHERE id = $1")
                .bind(deployment_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();
        if result.is_err() || status.as_deref() != Some("success") {
            failure = Some(format!(
                "{name}: {}",
                result
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| format!(
                        "deployment ended as {}",
                        status.unwrap_or_default()
                    ))
            ));
            break;
        }
        // A finished deployment means the spec was applied, not that the app
        // runs. Wait for it so `publish.succeeded` means the URL works.
        // Static sites run through shipyard-nginx-static without Swarm service tasks.
        let svc_type: Option<String> = sqlx::query_scalar("SELECT type::text FROM services WHERE id = $1")
            .bind(service_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

        if svc_type.as_deref() != Some("static") {
            let svc_name = format!("{}-{}", state.config.docker.label_prefix, service_id);
            if let Err(e) = wait_until_running(
                &*state.docker,
                &svc_name,
                deploy_started,
                std::time::Duration::from_secs(180),
            )
            .await
            {
                failure = Some(format!("{name}: {e}"));
                break;
            }
        }
    }
    let urls: Vec<String> = url.into_iter().collect();
    finish_publish(
        &state,
        org_id,
        project_id,
        publish_id,
        failure.map_or(Ok(()), Err),
        deployments,
        Some(urls),
    )
    .await;
}

/// Outcome of looking at a service's tasks once.
#[derive(Debug, PartialEq)]
pub enum Convergence {
    Running,
    Waiting,
    /// Tasks started since the deploy keep failing (crash loop, bad image).
    Failing(String),
}

/// Judge tasks created since `since`: every task Swarm wants running must be
/// running; three or more failures since the deploy is a crash loop.
pub fn convergence(
    tasks: &[shipyard_docker::types::TaskInfo],
    since: chrono::DateTime<chrono::Utc>,
) -> Convergence {
    let recent = |t: &&shipyard_docker::types::TaskInfo| {
        t.created_at
            .as_deref()
            .and_then(|c| chrono::DateTime::parse_from_rfc3339(c).ok())
            .map(|c| c >= since - chrono::Duration::seconds(5))
            .unwrap_or(true)
    };
    let failed: Vec<&shipyard_docker::types::TaskInfo> = tasks
        .iter()
        .filter(recent)
        .filter(|t| matches!(t.status.as_str(), "failed" | "rejected"))
        .collect();
    if failed.len() >= 3 {
        let last = failed
            .iter()
            .max_by_key(|t| t.updated_at.clone().unwrap_or_default())
            .and_then(|t| {
                t.error
                    .clone()
                    .or_else(|| t.exit_code.map(|c| format!("exited with code {c}")))
            })
            .unwrap_or_else(|| "tasks keep failing".into());
        return Convergence::Failing(format!("the app keeps failing to start: {last}"));
    }
    let wanted: Vec<&shipyard_docker::types::TaskInfo> = tasks
        .iter()
        .filter(|t| t.desired_state == "running")
        .collect();
    if !wanted.is_empty() && wanted.iter().all(|t| t.status == "running") {
        Convergence::Running
    } else {
        Convergence::Waiting
    }
}

async fn wait_until_running(
    docker: &dyn shipyard_docker::engine::DockerEngine,
    service_name: &str,
    since: chrono::DateTime<chrono::Utc>,
    timeout: std::time::Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_error: Option<String> = None;
    loop {
        match docker.list_tasks(service_name).await {
            Ok(tasks) => {
                match convergence(&tasks, since) {
                    Convergence::Running => return Ok(()),
                    Convergence::Failing(e) => return Err(e),
                    Convergence::Waiting => {}
                }
                last_error = tasks
                    .iter()
                    .filter(|t| t.desired_state == "running")
                    .find_map(|t| t.error.clone())
                    .or(last_error);
            }
            Err(e) => last_error = Some(e.to_string()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(match last_error {
                Some(e) => format!("not running after {}s: {e}", timeout.as_secs()),
                None => format!("not running after {}s", timeout.as_secs()),
            });
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

async fn finish_publish(
    state: &AppState,
    org_id: Uuid,
    project_id: Uuid,
    publish_id: Uuid,
    outcome: Result<(), String>,
    deployments: Vec<serde_json::Value>,
    urls: Option<Vec<String>>,
) {
    let urls = urls.unwrap_or_default();
    let (status, error) = match &outcome {
        Ok(()) => ("succeeded", None),
        Err(e) => ("failed", Some(e.clone())),
    };
    let _ = sqlx::query(
        "UPDATE publishes SET status = $2, error = $3, urls = $4, deployments = $5, finished_at = NOW() WHERE id = $1",
    )
    .bind(publish_id)
    .bind(status)
    .bind(&error)
    .bind(&urls)
    .bind(serde_json::Value::Array(deployments.clone()))
    .execute(&state.db)
    .await;
    events::emit(
        &state.db,
        Scope::project(org_id, project_id),
        if outcome.is_ok() { "publish.succeeded" } else { "publish.failed" },
        serde_json::json!({ "publish_id": publish_id, "urls": urls, "error": error, "deployments": deployments }),
    )
    .await;
}

// ─── Routes ──────────────────────────────────────────────────────────────────

/// POST /orgs/:org/apps/:app/envs/:env/publish (multipart: manifest, source?)
async fn publish_route(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ApiResponse<PublishOut>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_slug("environment", &env)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let max_bytes = state.config.integration.max_source_mb * 1024 * 1024;

    let work = PathBuf::from(format!(
        "{}/sources/work/{}",
        state.config.data_dir,
        Uuid::now_v7()
    ));
    tokio::fs::create_dir_all(&work)
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("work dir: {e}"))))?;
    let result = async {
        let mut manifest: Option<Manifest> = None;
        let mut zip_path: Option<PathBuf> = None;
        while let Some(mut field) = multipart
            .next_field()
            .await
            .map_err(|e| ApiAppError(AppError::BadRequest(format!("multipart: {e}"))))?
        {
            match field.name().unwrap_or_default() {
                "manifest" => {
                    let text = field
                        .text()
                        .await
                        .map_err(|e| ApiAppError(AppError::BadRequest(e.to_string())))?;
                    manifest = Some(serde_json::from_str(&text).map_err(|e| {
                        ApiAppError(AppError::BadRequest(format!("manifest: {e}")))
                    })?);
                }
                "source" => {
                    let path = work.join("source.zip");
                    let mut file = tokio::fs::File::create(&path)
                        .await
                        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
                    let mut written: u64 = 0;
                    while let Some(chunk) = field
                        .chunk()
                        .await
                        .map_err(|e| ApiAppError(AppError::BadRequest(format!("upload: {e}"))))?
                    {
                        written += chunk.len() as u64;
                        if written > max_bytes {
                            return Err(ApiAppError(AppError::BadRequest(format!(
                                "source is over {} MB",
                                state.config.integration.max_source_mb
                            ))));
                        }
                        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                            .await
                            .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
                    }
                    zip_path = Some(path);
                }
                _ => {}
            }
        }
        let manifest = manifest
            .ok_or_else(|| ApiAppError(AppError::BadRequest("missing 'manifest' field".into())))?;
        validate_manifest(&manifest)?;
        let source = match zip_path {
            Some(zip) => {
                let dest = work.join("src");
                let dest2 = dest.clone();
                let limit = max_bytes * 4;
                tokio::task::spawn_blocking(move || extract_zip(&zip, &dest2, limit))
                    .await
                    .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?
                    .map_err(|e| ApiAppError(AppError::BadRequest(format!("source: {e}"))))?;
                Source::Upload(dest)
            }
            None => Source::Unchanged,
        };
        publish(&state, &app, &env, manifest, source).await
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&work).await;

    let out = result?;
    caller
        .audit(&state.db, "publish", "project", Some(out.project_id), serde_json::json!({ "app": app.app_key, "environment": env, "publish_id": out.publish_id }))
        .await;
    Ok((StatusCode::ACCEPTED, Json(ApiResponse::ok(out))))
}

async fn get_publish(
    caller: IntegrationCaller,
    Path((org, app_key, env, publish_id)): Path<(String, String, String, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    #[allow(clippy::type_complexity)]
    let row: Option<(String, serde_json::Value, Vec<String>, Option<String>, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as(
            "SELECT status, deployments, urls, error, created_at, finished_at FROM publishes WHERE id = $1 AND project_id = $2",
        )
        .bind(publish_id)
        .bind(project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;
    let (status, deployments, urls, error, created_at, finished_at) =
        row.ok_or_else(|| ApiAppError(AppError::NotFound("Publish not found".into())))?;

    // Live status of each deployment.
    let mut items = vec![];
    for d in deployments.as_array().cloned().unwrap_or_default() {
        let dep_id = d["deployment_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok());
        let dep_status: Option<String> = match dep_id {
            Some(id) => sqlx::query_scalar("SELECT status::text FROM deployments WHERE id = $1")
                .bind(id)
                .fetch_optional(&state.db)
                .await
                .map_err(db_err)?,
            None => None,
        };
        let mut d = d.clone();
        d["status"] = serde_json::json!(dep_status);
        items.push(d);
    }
    Ok(Json(ApiResponse::ok(serde_json::json!({
        "publish_id": publish_id,
        "status": status,
        "urls": urls,
        "error": error,
        "deployments": items,
        "created_at": created_at,
        "finished_at": finished_at,
    }))))
}

#[derive(Deserialize)]
struct DuplicateBody {
    /// New environment name, e.g. "staging".
    to: String,
    /// Copy env-specific user secrets (API keys) from the source environment.
    /// Generated secrets and the database password are always new.
    #[serde(default = "yes")]
    copy_secrets: bool,
}

fn yes() -> bool {
    true
}

/// POST /orgs/:org/apps/:app/envs/:env/duplicate — create another
/// environment from this one's last manifest and source, with its own
/// database (empty), generated secrets and URL, and deploy it.
async fn duplicate_route(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
    Json(body): Json<DuplicateBody>,
) -> Result<(StatusCode, Json<ApiResponse<PublishOut>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_slug("environment", &body.to)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let src_project = env_project(&state.db, &app, &env).await?;
    if env_project(&state.db, &app, &body.to).await.is_ok() {
        return Err(ApiAppError(AppError::Conflict(format!(
            "Environment '{}' already exists",
            body.to
        ))));
    }
    let manifest: serde_json::Value = sqlx::query_scalar(
        "SELECT manifest FROM publishes WHERE project_id = $1
         ORDER BY (status = 'succeeded') DESC, created_at DESC LIMIT 1",
    )
    .bind(src_project)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or_else(|| {
        ApiAppError(AppError::BadRequest(format!(
            "Environment '{env}' has never been published"
        )))
    })?;
    let mut manifest: Manifest = serde_json::from_value(manifest)
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    // The copy gets its own default hostname.
    if let Some(d) = manifest.domain.as_mut() {
        d.subdomain = None;
    }

    let (new_project, _) = ensure_env_project(&state, &app, &body.to).await?;
    sqlx::query("UPDATE projects SET duplicated_from = $2 WHERE id = $1")
        .bind(new_project)
        .bind(src_project)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    if body.copy_secrets {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT name, value_encrypted FROM secrets WHERE project_id = $1 AND kind = 'user'",
        )
        .bind(src_project)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;
        for (name, enc) in rows {
            let value =
                crypto::decrypt(&state.config.auth.secret_key, &enc).map_err(ApiAppError)?;
            upsert_secret(
                &state.db,
                &state.config.auth.secret_key,
                app.org_id,
                Some(app.app_id),
                Some(new_project),
                &name,
                "user",
                &value,
            )
            .await?;
        }
    }

    let out = publish(
        &state,
        &app,
        &body.to,
        manifest,
        Source::FromProject(src_project),
    )
    .await?;
    events::emit(
        &state.db,
        Scope::project(app.org_id, new_project),
        "project.duplicated",
        serde_json::json!({ "from_environment": env, "app": app.app_key, "environment": body.to }),
    )
    .await;
    caller
        .audit(
            &state.db,
            "duplicate_environment",
            "project",
            Some(new_project),
            serde_json::json!({ "app": app.app_key, "from": env, "to": body.to }),
        )
        .await;
    Ok((StatusCode::ACCEPTED, Json(ApiResponse::ok(out))))
}

/// Services of an environment by slug, for status and logs.
pub async fn env_services(
    db: &sqlx::PgPool,
    project_id: Uuid,
) -> Result<HashMap<String, Uuid>, ApiAppError> {
    let rows: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT slug, id FROM services WHERE project_id = $1")
            .bind(project_id)
            .fetch_all(db)
            .await
            .map_err(db_err)?;
    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: serde_json::Value) -> Manifest {
        serde_json::from_value(json).expect("manifest parses")
    }

    #[test]
    fn full_manifest_parses_and_validates() {
        let m = manifest(serde_json::json!({
            "services": [{
                "name": "web",
                "port": 3000,
                "env": {
                    "NODE_ENV": "production",
                    "SESSION_SECRET": { "generate": "hex32" },
                    "STRIPE_KEY": { "secret": "STRIPE_KEY" },
                    "DATABASE_URL": { "database": "url" }
                },
                "volumes": [{ "path": "/app/uploads" }]
            }, {
                "name": "worker", "kind": "worker", "path": "apps/worker", "dockerfile": "Dockerfile.worker"
            }],
            "database": { "engine": "postgres" },
            "domain": { "subdomain": "todo" }
        }));
        if let Err(e) = validate_manifest(&m) {
            panic!("{}", e.0);
        }
        assert!(matches!(
            m.services[0].env["SESSION_SECRET"],
            EnvValue::Generate {
                generate: Generate::Hex32
            }
        ));
        assert!(matches!(
            m.services[0].env["DATABASE_URL"],
            EnvValue::Database {
                database: DbField::Url
            }
        ));
        assert!(matches!(m.services[0].env["NODE_ENV"], EnvValue::Plain(_)));
        assert_eq!(m.services[1].kind, ServiceKind::Worker);
    }

    #[test]
    fn static_manifest_parses_and_validates() {
        let m: Manifest = serde_json::from_value(serde_json::json!({
            "services": [{
                "name": "landing",
                "kind": "static",
                "path": "frontend"
            }],
            "domain": { "subdomain": "landing", "service": "landing" }
        }))
        .unwrap();
        assert!(validate_manifest(&m).is_ok());
        assert_eq!(m.services[0].kind, ServiceKind::Static);

        // Web service with dockerfile and no port is valid
        let m2: Manifest = serde_json::from_value(serde_json::json!({
            "services": [{
                "name": "web",
                "dockerfile": "Dockerfile"
            }]
        }))
        .unwrap();
        assert!(validate_manifest(&m2).is_ok());
    }

    #[test]
    fn invalid_manifests_are_rejected() {
        let cases = [
            serde_json::json!({ "services": [] }),
            serde_json::json!({ "services": [{ "name": "web" }] }),
            serde_json::json!({ "services": [{ "name": "db", "port": 1 }] }),
            serde_json::json!({ "services": [{ "name": "Web", "port": 1 }] }),
            serde_json::json!({ "services": [{ "name": "a", "port": 1 }, { "name": "a", "port": 1 }] }),
            serde_json::json!({ "services": [{ "name": "a", "port": 1, "path": "../x" }] }),
            serde_json::json!({ "services": [{ "name": "a", "port": 1, "env": { "DATABASE_URL": { "database": "url" } } }] }),
            serde_json::json!({ "services": [{ "name": "a", "port": 1, "env": { "bad-name": "x" } }] }),
            serde_json::json!({ "services": [{ "name": "a", "port": 1, "volumes": [{ "path": "rel" }] }] }),
            serde_json::json!({ "services": [{ "name": "a", "kind": "worker" }], "domain": { "service": "a" } }),
            serde_json::json!({ "services": [{ "name": "a", "port": 1 }], "database": { "engine": "mysql" } }),
        ];
        for c in cases {
            let m: Result<Manifest, _> = serde_json::from_value(c.clone());
            if let Ok(m) = m {
                assert!(validate_manifest(&m).is_err(), "should reject {c}");
            }
        }
        // Unknown fields are a parse error, so typos don't silently do nothing.
        assert!(serde_json::from_value::<Manifest>(
            serde_json::json!({ "services": [{ "name": "a", "prot": 1 }] })
        )
        .is_err());
    }

    fn task(
        status: &str,
        desired: &str,
        created: &str,
        error: Option<&str>,
    ) -> shipyard_docker::types::TaskInfo {
        shipyard_docker::types::TaskInfo {
            id: Uuid::now_v7().to_string(),
            service_id: "s".into(),
            node_id: None,
            status: status.into(),
            desired_state: desired.into(),
            container_id: None,
            image: "i".into(),
            created_at: Some(created.into()),
            updated_at: Some(created.into()),
            error: error.map(str::to_string),
            exit_code: None,
            slot: Some(1),
            labels: Default::default(),
        }
    }

    #[test]
    fn convergence_rules() {
        let since = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let new = "2026-01-01T00:00:10Z";
        let old = "2025-12-31T00:00:00Z";
        assert_eq!(
            convergence(&[task("running", "running", new, None)], since),
            Convergence::Running
        );
        assert_eq!(
            convergence(&[task("preparing", "running", new, None)], since),
            Convergence::Waiting
        );
        assert_eq!(convergence(&[], since), Convergence::Waiting);
        // Old failures from before this deploy don't count.
        let old_failures: Vec<_> = (0..5)
            .map(|_| task("failed", "shutdown", old, Some("x")))
            .collect();
        assert_eq!(convergence(&old_failures, since), Convergence::Waiting);
        let mut crash: Vec<_> = (0..3)
            .map(|_| task("failed", "shutdown", new, Some("exit 1")))
            .collect();
        crash.push(task("starting", "running", new, None));
        assert!(
            matches!(convergence(&crash, since), Convergence::Failing(e) if e.contains("exit 1"))
        );
    }

    #[test]
    fn subdomains() {
        assert_eq!(default_subdomain("todo", "production"), "todo");
        assert_eq!(default_subdomain("todo", "staging"), "todo-staging");
        assert!(default_subdomain(&"a".repeat(60), "preview-123").len() <= 63);
    }

    fn write_zip(path: &FsPath, entries: &[(&str, &[u8])]) {
        let f = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            std::io::Write::write_all(&mut w, data).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn zip_extraction_strips_single_root_and_skips_git() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("a.zip");
        write_zip(
            &zip,
            &[
                ("app/package.json", b"{}"),
                ("app/src/index.js", b"x"),
                ("app/.git/config", b"evil"),
            ],
        );
        let out = dir.path().join("out");
        extract_zip(&zip, &out, 1 << 20).unwrap();
        assert!(out.join("package.json").exists());
        assert!(out.join("src/index.js").exists());
        assert!(!out.join(".git").exists());
    }

    #[test]
    fn zip_extraction_rejects_traversal_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("evil.zip");
        write_zip(&zip, &[("../../etc/x", b"x"), ("ok.txt", b"y")]);
        assert!(extract_zip(&zip, &dir.path().join("o1"), 1 << 20).is_err());

        let big = dir.path().join("big.zip");
        let data = vec![b'a'; 4096];
        write_zip(&big, &[("a.bin", &data), ("b.bin", &data)]);
        assert!(extract_zip(&big, &dir.path().join("o2"), 6000).is_err());
    }
}
