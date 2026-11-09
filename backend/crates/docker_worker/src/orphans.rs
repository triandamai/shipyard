use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{info, warn};
use uuid::Uuid;

use shipyard_common::error::{AppError, AppResult};
use shipyard_docker::engine::DockerEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrphanType {
    Org,
    Project,
    Service,
    Volume,
    Directory,
}

impl OrphanType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrphanType::Org => "org",
            OrphanType::Project => "project",
            OrphanType::Service => "service",
            OrphanType::Volume => "volume",
            OrphanType::Directory => "directory",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "org" => Some(OrphanType::Org),
            "project" => Some(OrphanType::Project),
            "service" => Some(OrphanType::Service),
            "volume" => Some(OrphanType::Volume),
            "directory" => Some(OrphanType::Directory),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanResource {
    pub id: Uuid,
    pub orphan_type: OrphanType,
    pub identifier: String,
    pub path: String,
    pub size_bytes: i64,
    pub details: Option<String>,
    pub detected_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredOrphan {
    pub orphan_type: OrphanType,
    pub identifier: String,
    pub path: String,
    pub size_bytes: i64,
    pub details: Option<String>,
}

/// Recursively calculates the total disk size in bytes of a path.
pub async fn dir_size_bytes(path: &Path) -> i64 {
    if !path.exists() {
        return 0;
    }
    if path.is_file() || path.is_symlink() {
        return tokio::fs::metadata(path)
            .await
            .map(|m| m.len() as i64)
            .unwrap_or(0);
    }

    let mut total: i64 = 0;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&current_dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let entry_path = entry.path();
            if entry_path.is_symlink() {
                continue;
            }
            if let Ok(meta) = entry.metadata().await {
                if meta.is_dir() {
                    stack.push(entry_path);
                } else {
                    total += meta.len() as i64;
                }
            }
        }
    }

    total
}

/// Verifies whether an orphan path is safe to delete.
/// Prevents deleting root directories or path-traversal escapes.
pub fn is_safe_to_delete(path_str: &str, data_dir: &str) -> bool {
    if path_str.starts_with("docker:") {
        let vol_name = path_str.trim_start_matches("docker:");
        return !vol_name.is_empty()
            && !vol_name.contains('/')
            && !vol_name.contains('\\')
            && !vol_name.contains("..");
    }

    let candidate = PathBuf::from(path_str);
    if !candidate.is_absolute() {
        return false;
    }

    // Must not be root or standard system top-level folders
    let forbidden = [
        "/", "/etc", "/usr", "/var", "/bin", "/sbin", "/lib", "/boot", "/dev",
        "/proc", "/sys", "/home", "/root", "/opt", "/tmp",
    ];
    let path_str_clean = candidate.to_string_lossy();
    for f in forbidden {
        if path_str_clean == f {
            return false;
        }
    }

    // Must not be the data_dir itself or shipyard base dir itself
    let trimmed_candidate = path_str_clean.trim_end_matches('/');
    let trimmed_data = data_dir.trim_end_matches('/');
    if trimmed_candidate == trimmed_data
        || trimmed_candidate == "/shipyard"
        || trimmed_candidate == "/opt/shipyard"
    {
        return false;
    }

    // Check if path contains ..
    if candidate.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }

    // Must be a descendant of data_dir or /shipyard or /opt/shipyard
    let allowed_prefixes = [
        trimmed_data,
        "/shipyard",
        "/opt/shipyard",
    ];

    allowed_prefixes.iter().any(|prefix| {
        !prefix.is_empty()
            && (trimmed_candidate.starts_with(&format!("{prefix}/"))
                || trimmed_candidate.starts_with(&format!("{prefix}\\")))
    })
}

/// Scans the filesystem under `data_dir` (and base `/shipyard` if present)
/// and checks against database records to find orphaned directories,
/// orgs, projects, services, and volumes.
pub async fn scan_filesystem_orphans(db: &PgPool, data_dir: &str) -> AppResult<Vec<DiscoveredOrphan>> {
    let base_path = Path::new(data_dir);
    if !base_path.exists() {
        return Ok(Vec::new());
    }

    // Fetch active UUID sets from database
    let active_orgs: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM organizations")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let active_org_set: std::collections::HashSet<Uuid> = active_orgs.into_iter().collect();

    let active_projects: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT id, org_id FROM projects")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let active_project_set: std::collections::HashMap<Uuid, Uuid> =
        active_projects.into_iter().collect();

    let active_services: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT id, project_id FROM services")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let active_service_set: std::collections::HashMap<Uuid, Uuid> =
        active_services.into_iter().collect();

    let active_volumes: Vec<(Uuid, String)> = sqlx::query_as("SELECT service_id, name FROM volumes")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let active_volume_set: std::collections::HashSet<(Uuid, String)> =
        active_volumes.into_iter().collect();

    let active_edge_fns: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM edge_functions")
        .fetch_all(db)
        .await
        .unwrap_or_default();
    let active_edge_set: std::collections::HashSet<Uuid> = active_edge_fns.into_iter().collect();

    scan_filesystem_orphans_with_sets(
        &active_org_set,
        &active_project_set,
        &active_service_set,
        &active_volume_set,
        &active_edge_set,
        data_dir,
    )
    .await
}

/// Internal directory scanning logic taking active entity sets directly.
pub async fn scan_filesystem_orphans_with_sets(
    active_org_set: &std::collections::HashSet<Uuid>,
    active_project_set: &std::collections::HashMap<Uuid, Uuid>,
    active_service_set: &std::collections::HashMap<Uuid, Uuid>,
    active_volume_set: &std::collections::HashSet<(Uuid, String)>,
    active_edge_set: &std::collections::HashSet<Uuid>,
    data_dir: &str,
) -> AppResult<Vec<DiscoveredOrphan>> {
    let base_path = Path::new(data_dir);
    if !base_path.exists() {
        return Ok(Vec::new());
    }

    let mut orphans = Vec::new();

    // 1. Read entries directly under data_dir
    let mut entries = match tokio::fs::read_dir(base_path).await {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to read data_dir {}: {e}", base_path.display());
            return Ok(Vec::new());
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();

        // Check well-known system dirs under data_dir
        match dir_name.as_str() {
            "static" => {
                scan_static_artifacts(&entry_path, &active_service_set, &mut orphans).await;
                continue;
            }
            "edge" => {
                scan_edge_artifacts(&entry_path, &active_edge_set, &mut orphans).await;
                continue;
            }
            "sources" => {
                scan_source_repos(&entry_path, &active_service_set, &mut orphans).await;
                continue;
            }
            "registry" | "traefik" | "backups" | "lost+found" | "logs" => {
                // Known platform infrastructure directories
                continue;
            }
            _ => {}
        }

        // Try to parse dir_name as UUID (expected for org_id)
        if let Ok(potential_uuid) = Uuid::parse_str(&dir_name) {
            if !active_org_set.contains(&potential_uuid) {
                // If not an org, check if it was an ancient legacy project folder
                if active_project_set.contains_key(&potential_uuid) {
                    // Still belongs to an active project
                    continue;
                }
                // Orphan organization directory
                let size = dir_size_bytes(&entry_path).await;
                orphans.push(DiscoveredOrphan {
                    orphan_type: OrphanType::Org,
                    identifier: dir_name.clone(),
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("Organization directory exists on disk but no organization record exists in database".to_string()),
                });
            } else {
                // Organization exists -> inspect project subdirectories
                scan_org_projects(
                    &entry_path,
                    potential_uuid,
                    &active_project_set,
                    &active_service_set,
                    &active_volume_set,
                    &mut orphans,
                )
                .await;
            }
        } else {
            // Unrecognized directory directly under data_dir
            let size = dir_size_bytes(&entry_path).await;
            orphans.push(DiscoveredOrphan {
                orphan_type: OrphanType::Directory,
                identifier: dir_name,
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: size,
                details: Some("Unrecognized directory in shipyard data directory".to_string()),
            });
        }
    }

    // 2. Also check base /shipyard or /opt/shipyard for unused top-level directories
    check_shipyard_root_directories(data_dir, &mut orphans).await;

    Ok(orphans)
}

async fn scan_org_projects(
    org_path: &Path,
    org_id: Uuid,
    active_project_set: &std::collections::HashMap<Uuid, Uuid>,
    active_service_set: &std::collections::HashMap<Uuid, Uuid>,
    active_volume_set: &std::collections::HashSet<(Uuid, String)>,
    orphans: &mut Vec<DiscoveredOrphan>,
) {
    let mut entries = match tokio::fs::read_dir(org_path).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();

        if let Ok(project_uuid) = Uuid::parse_str(&dir_name) {
            match active_project_set.get(&project_uuid) {
                Some(mapped_org) if *mapped_org == org_id => {
                    // Valid project in this org -> scan contents (services, volumes)
                    scan_project_contents(
                        &entry_path,
                        project_uuid,
                        active_service_set,
                        active_volume_set,
                        orphans,
                    )
                    .await;
                }
                _ => {
                    // Orphan project
                    let size = dir_size_bytes(&entry_path).await;
                    orphans.push(DiscoveredOrphan {
                        orphan_type: OrphanType::Project,
                        identifier: dir_name,
                        path: entry_path.to_string_lossy().to_string(),
                        size_bytes: size,
                        details: Some(format!(
                            "Project directory exists on disk but does not belong to active organization {org_id}"
                        )),
                    });
                }
            }
        } else {
            // Unrecognized item in org directory
            let size = dir_size_bytes(&entry_path).await;
            orphans.push(DiscoveredOrphan {
                orphan_type: OrphanType::Directory,
                identifier: dir_name,
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: size,
                details: Some(format!("Unrecognized directory in organization folder {org_id}")),
            });
        }
    }
}

async fn scan_project_contents(
    project_path: &Path,
    project_id: Uuid,
    active_service_set: &std::collections::HashMap<Uuid, Uuid>,
    active_volume_set: &std::collections::HashSet<(Uuid, String)>,
    orphans: &mut Vec<DiscoveredOrphan>,
) {
    let mut entries = match tokio::fs::read_dir(project_path).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();

        // Pattern 1: <service_id>-<volume_name>
        if dir_name.len() > 37 && dir_name.as_bytes()[36] == b'-' {
            let (svc_id_str, vol_name) = dir_name.split_at(36);
            let vol_name = &vol_name[1..]; // trim leading '-'

            if let Ok(svc_uuid) = Uuid::parse_str(svc_id_str) {
                let service_valid = active_service_set.get(&svc_uuid).map(|p| *p == project_id).unwrap_or(false);

                if !service_valid {
                    // Orphan volume for deleted or moved service
                    let size = dir_size_bytes(&entry_path).await;
                    orphans.push(DiscoveredOrphan {
                        orphan_type: OrphanType::Volume,
                        identifier: dir_name.clone(),
                        path: entry_path.to_string_lossy().to_string(),
                        size_bytes: size,
                        details: Some(format!(
                            "Volume directory '{vol_name}' references non-existent service {svc_uuid} in project {project_id}"
                        )),
                    });
                    continue;
                }

                // Service exists, verify if volume is still tracked in DB
                let volume_exists = active_volume_set.contains(&(svc_uuid, vol_name.to_string()))
                    || active_volume_set.iter().any(|(s, v)| *s == svc_uuid && (v.ends_with(vol_name) || vol_name.ends_with(v)));

                if !volume_exists {
                    let size = dir_size_bytes(&entry_path).await;
                    orphans.push(DiscoveredOrphan {
                        orphan_type: OrphanType::Volume,
                        identifier: dir_name.clone(),
                        path: entry_path.to_string_lossy().to_string(),
                        size_bytes: size,
                        details: Some(format!(
                            "Volume directory '{vol_name}' is not registered in database for service {svc_uuid}"
                        )),
                    });
                    continue;
                }

                // Volume is active and valid
                continue;
            }
        }

        // Pattern 2: Service directory (UUID)
        if let Ok(svc_uuid) = Uuid::parse_str(&dir_name) {
            let service_valid = active_service_set.get(&svc_uuid).map(|p| *p == project_id).unwrap_or(false);
            if !service_valid {
                let size = dir_size_bytes(&entry_path).await;
                orphans.push(DiscoveredOrphan {
                    orphan_type: OrphanType::Service,
                    identifier: dir_name.clone(),
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some(format!(
                        "Service directory on disk not found in project {project_id}"
                    )),
                });
            }
            continue;
        }

        // Pattern 3: Any other untracked directory in project
        let size = dir_size_bytes(&entry_path).await;
        orphans.push(DiscoveredOrphan {
            orphan_type: OrphanType::Directory,
            identifier: dir_name.clone(),
            path: entry_path.to_string_lossy().to_string(),
            size_bytes: size,
            details: Some(format!("Untracked directory in project {project_id}")),
        });
    }
}

async fn scan_static_artifacts(
    static_path: &Path,
    active_service_set: &std::collections::HashMap<Uuid, Uuid>,
    orphans: &mut Vec<DiscoveredOrphan>,
) {
    let mut entries = match tokio::fs::read_dir(static_path).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if name == "logs" || name == "uploads" {
            continue;
        }

        if let Ok(svc_uuid) = Uuid::parse_str(&name) {
            if !active_service_set.contains_key(&svc_uuid) {
                let size = dir_size_bytes(&entry_path).await;
                orphans.push(DiscoveredOrphan {
                    orphan_type: OrphanType::Service,
                    identifier: name,
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("Static site deployed artifact belongs to deleted service".to_string()),
                });
            }
        }
    }
}

async fn scan_edge_artifacts(
    edge_path: &Path,
    active_edge_set: &std::collections::HashSet<Uuid>,
    orphans: &mut Vec<DiscoveredOrphan>,
) {
    let mut entries = match tokio::fs::read_dir(edge_path).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if let Ok(fn_uuid) = Uuid::parse_str(&name) {
            if !active_edge_set.contains(&fn_uuid) {
                let size = dir_size_bytes(&entry_path).await;
                orphans.push(DiscoveredOrphan {
                    orphan_type: OrphanType::Service,
                    identifier: name,
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("Edge function deployed code belongs to deleted function".to_string()),
                });
            }
        }
    }
}

async fn scan_source_repos(
    sources_path: &Path,
    active_service_set: &std::collections::HashMap<Uuid, Uuid>,
    orphans: &mut Vec<DiscoveredOrphan>,
) {
    let mut entries = match tokio::fs::read_dir(sources_path).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if let Some(svc_id_str) = name.strip_suffix(".git") {
            if let Ok(svc_uuid) = Uuid::parse_str(svc_id_str) {
                if !active_service_set.contains_key(&svc_uuid) {
                    let size = dir_size_bytes(&entry_path).await;
                    orphans.push(DiscoveredOrphan {
                        orphan_type: OrphanType::Service,
                        identifier: name,
                        path: entry_path.to_string_lossy().to_string(),
                        size_bytes: size,
                        details: Some("Git source repository belongs to deleted service".to_string()),
                    });
                }
            }
        }
    }
}

async fn check_shipyard_root_directories(data_dir: &str, orphans: &mut Vec<DiscoveredOrphan>) {
    let shipyard_root = match Path::new(data_dir).parent() {
        Some(p) if p.exists() && (p.ends_with("shipyard") || p.to_string_lossy() == "/shipyard") => p,
        _ => return,
    };

    let mut entries = match tokio::fs::read_dir(shipyard_root).await {
        Ok(e) => e,
        Err(_) => return,
    };

    let known_root_dirs = ["data", "traefik", "backups", "ssl", "certs", "config", "logs"];

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if !known_root_dirs.contains(&name.as_str()) && !name.starts_with('.') {
            let size = dir_size_bytes(&entry_path).await;
            orphans.push(DiscoveredOrphan {
                orphan_type: OrphanType::Directory,
                identifier: name,
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: size,
                details: Some("Unrecognized directory directly under shipyard installation root".to_string()),
            });
        }
    }
}

/// Scans Docker volumes if DockerEngine is available.
pub async fn scan_docker_orphan_volumes(
    db: &PgPool,
    docker: &dyn DockerEngine,
    label_prefix: &str,
    orphans: &mut Vec<DiscoveredOrphan>,
) {
    let docker_volumes = match docker.list_all_volumes().await {
        Ok(vols) => vols,
        Err(e) => {
            warn!("Failed to list docker volumes for orphan detection: {e}");
            return;
        }
    };

    let active_db_volumes: Vec<String> = sqlx::query_scalar("SELECT name FROM volumes")
        .fetch_all(db)
        .await
        .unwrap_or_default();
    let db_vol_set: std::collections::HashSet<String> = active_db_volumes.into_iter().collect();

    let prefix_match = format!("{label_prefix}-");

    for vol in docker_volumes {
        if vol.name.starts_with(&prefix_match) && !db_vol_set.contains(&vol.name) {
            orphans.push(DiscoveredOrphan {
                orphan_type: OrphanType::Volume,
                identifier: vol.name.clone(),
                path: format!("docker:{}", vol.name),
                size_bytes: 0,
                details: Some(format!("Docker volume '{}' is not attached to any service in database", vol.name)),
            });
        }
    }
}

/// Walks the filesystem, checks Docker volumes, synchronizes with the `orphans` table,
/// and returns the current set of orphaned resources.
pub async fn scan_and_reconcile_orphans(
    db: &PgPool,
    data_dir: &str,
    docker: Option<(&dyn DockerEngine, &str)>,
) -> AppResult<Vec<OrphanResource>> {
    let mut discovered = scan_filesystem_orphans(db, data_dir).await?;

    if let Some((d, prefix)) = docker {
        scan_docker_orphan_volumes(db, d, prefix, &mut discovered).await;
    }

    let mut current_paths = std::collections::HashSet::new();

    for item in &discovered {
        current_paths.insert(item.path.clone());
        let o_type = item.orphan_type.as_str();

        sqlx::query(
            r#"
            INSERT INTO orphans (id, orphan_type, identifier, path, size_bytes, details, detected_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
            ON CONFLICT (path)
            DO UPDATE SET
                size_bytes = EXCLUDED.size_bytes,
                details    = EXCLUDED.details,
                updated_at = NOW()
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(o_type)
        .bind(&item.identifier)
        .bind(&item.path)
        .bind(item.size_bytes)
        .bind(&item.details)
        .execute(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    // Clean up any orphan rows whose paths were resolved / no longer detected
    let all_existing: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, path FROM orphans")
        .fetch_all(db)
        .await
        .unwrap_or_default();

    for (id, path) in all_existing {
        if !current_paths.contains(&path) {
            let _ = sqlx::query("DELETE FROM orphans WHERE id = $1")
                .bind(id)
                .execute(db)
                .await;
        }
    }

    list_orphans(db, None, None).await
}

/// Lists recorded orphans from the database.
pub async fn list_orphans(
    db: &PgPool,
    orphan_type: Option<&str>,
    search: Option<&str>,
) -> AppResult<Vec<OrphanResource>> {
    let mut q = "SELECT id, orphan_type, identifier, path, size_bytes, details, detected_at, updated_at FROM orphans WHERE 1=1".to_string();

    if orphan_type.is_some() {
        q.push_str(" AND orphan_type = $1");
    }
    if search.is_some() {
        if orphan_type.is_some() {
            q.push_str(" AND (identifier ILIKE $2 OR path ILIKE $2 OR details ILIKE $2)");
        } else {
            q.push_str(" AND (identifier ILIKE $1 OR path ILIKE $1 OR details ILIKE $1)");
        }
    }
    q.push_str(" ORDER BY detected_at DESC");

    let rows: Vec<(
        Uuid,
        String,
        String,
        String,
        i64,
        Option<String>,
        DateTime<Utc>,
        DateTime<Utc>,
    )> = match (orphan_type, search) {
        (Some(ot), Some(s)) => {
            let pattern = format!("%{s}%");
            sqlx::query_as(&q)
                .bind(ot)
                .bind(pattern)
                .fetch_all(db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?
        }
        (Some(ot), None) => {
            sqlx::query_as(&q)
                .bind(ot)
                .fetch_all(db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?
        }
        (None, Some(s)) => {
            let pattern = format!("%{s}%");
            sqlx::query_as(&q)
                .bind(pattern)
                .fetch_all(db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?
        }
        (None, None) => {
            sqlx::query_as(&q)
                .fetch_all(db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?
        }
    };

    let items = rows
        .into_iter()
        .map(|(id, ot, identifier, path, size_bytes, details, detected_at, updated_at)| {
            OrphanResource {
                id,
                orphan_type: OrphanType::from_str(&ot).unwrap_or(OrphanType::Directory),
                identifier,
                path,
                size_bytes,
                details,
                detected_at,
                updated_at,
            }
        })
        .collect();

    Ok(items)
}

/// Deletes an orphan resource from disk (or Docker) and removes its record from the database.
pub async fn delete_orphan(
    db: &PgPool,
    orphan_id: Uuid,
    data_dir: &str,
    docker: Option<&dyn DockerEngine>,
) -> AppResult<()> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT orphan_type, identifier, path FROM orphans WHERE id = $1")
            .bind(orphan_id)
            .fetch_optional(db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

    let (_orphan_type, _identifier, path) = match row {
        Some(r) => r,
        None => return Err(AppError::NotFound(format!("Orphan {orphan_id} not found"))),
    };

    if !is_safe_to_delete(&path, data_dir) {
        return Err(AppError::Validation(format!(
            "Path '{path}' is not safe to delete"
        )));
    }

    if path.starts_with("docker:") {
        let vol_name = path.trim_start_matches("docker:");
        if let Some(d) = docker {
            info!("Removing orphaned Docker volume: {vol_name}");
            let _ = d.remove_volume(vol_name).await;
        }
    } else {
        let p = Path::new(&path);
        if p.exists() {
            info!("Removing orphaned path from disk: {path}");
            if p.is_dir() {
                tokio::fs::remove_dir_all(p)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to remove directory {path}: {e}")))?;
            } else {
                tokio::fs::remove_file(p)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to remove file {path}: {e}")))?;
            }
        }
    }

    sqlx::query("DELETE FROM orphans WHERE id = $1")
        .bind(orphan_id)
        .execute(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_to_delete_rejects_dangerous_paths() {
        let data_dir = "/opt/shipyard/data";

        // Forbidden roots
        assert!(!is_safe_to_delete("/", data_dir));
        assert!(!is_safe_to_delete("/etc", data_dir));
        assert!(!is_safe_to_delete("/usr", data_dir));
        assert!(!is_safe_to_delete("/var", data_dir));
        assert!(!is_safe_to_delete("/opt", data_dir));
        assert!(!is_safe_to_delete("/opt/shipyard", data_dir));
        assert!(!is_safe_to_delete("/opt/shipyard/data", data_dir));
        assert!(!is_safe_to_delete("/opt/shipyard/data/", data_dir));
        assert!(!is_safe_to_delete("/shipyard", data_dir));

        // Relative and traversal
        assert!(!is_safe_to_delete("relative/path", data_dir));
        assert!(!is_safe_to_delete("/opt/shipyard/data/../etc/passwd", data_dir));

        // Malicious docker volume names
        assert!(!is_safe_to_delete("docker:", data_dir));
        assert!(!is_safe_to_delete("docker:../volume", data_dir));
        assert!(!is_safe_to_delete("docker:my/volume", data_dir));
    }

    #[test]
    fn test_is_safe_to_delete_allows_valid_subpaths() {
        let data_dir = "/opt/shipyard/data";

        // Legitimate child directories
        assert!(is_safe_to_delete("/opt/shipyard/data/0190a4b2-0000-7000-8000-000000000001", data_dir));
        assert!(is_safe_to_delete("/opt/shipyard/data/0190a4b2-0000-7000-8000-000000000001/proj-1", data_dir));
        assert!(is_safe_to_delete("/opt/shipyard/data/0190a4b2-0000-7000-8000-000000000001/proj-1/svc-data", data_dir));
        assert!(is_safe_to_delete("/shipyard/unused_cache", data_dir));
        assert!(is_safe_to_delete("docker:platform-volume-123", data_dir));
    }

    #[tokio::test]
    async fn test_dir_size_bytes_calculates_accurately() {
        let tmp = std::env::temp_dir().join(format!("shipyard-test-orphan-{}", Uuid::now_v7()));
        tokio::fs::create_dir_all(&tmp).await.unwrap();

        let sub = tmp.join("sub");
        tokio::fs::create_dir_all(&sub).await.unwrap();

        let file1 = tmp.join("f1.txt");
        let file2 = sub.join("f2.txt");

        tokio::fs::write(&file1, b"hello").await.unwrap(); // 5 bytes
        tokio::fs::write(&file2, b"world!!!").await.unwrap(); // 8 bytes

        let size = dir_size_bytes(&tmp).await;
        assert_eq!(size, 13);

        tokio::fs::remove_dir_all(&tmp).await.unwrap();
    }

    #[tokio::test]
    async fn test_scan_filesystem_orphans_detects_all_orphan_types() {
        let tmp = std::env::temp_dir().join(format!("shipyard-orphan-scan-{}", Uuid::now_v7()));
        tokio::fs::create_dir_all(&tmp).await.unwrap();

        let valid_org = Uuid::now_v7();
        let orphan_org = Uuid::now_v7();

        let valid_proj = Uuid::now_v7();
        let orphan_proj = Uuid::now_v7();

        let valid_svc = Uuid::now_v7();
        let orphan_svc = Uuid::now_v7();

        let orphan_fn = Uuid::now_v7();

        // 1. Orphan org folder
        let orphan_org_path = tmp.join(orphan_org.to_string());
        tokio::fs::create_dir_all(&orphan_org_path).await.unwrap();
        tokio::fs::write(orphan_org_path.join("file.txt"), b"orphan-org").await.unwrap();

        // 2. Valid org folder
        let valid_org_path = tmp.join(valid_org.to_string());
        tokio::fs::create_dir_all(&valid_org_path).await.unwrap();

        // 2a. Orphan project folder inside valid org
        let orphan_proj_path = valid_org_path.join(orphan_proj.to_string());
        tokio::fs::create_dir_all(&orphan_proj_path).await.unwrap();
        tokio::fs::write(orphan_proj_path.join("proj.txt"), b"orphan-proj").await.unwrap();

        // 2b. Valid project folder inside valid org
        let valid_proj_path = valid_org_path.join(valid_proj.to_string());
        tokio::fs::create_dir_all(&valid_proj_path).await.unwrap();

        // 2b i. Orphan volume inside valid project (referencing non-existent volume)
        let orphan_vol_path = valid_proj_path.join(format!("{valid_svc}-untrackedvol"));
        tokio::fs::create_dir_all(&orphan_vol_path).await.unwrap();
        tokio::fs::write(orphan_vol_path.join("db.dat"), b"vol-data").await.unwrap();

        // 2b ii. Valid volume inside valid project
        let valid_vol_path = valid_proj_path.join(format!("{valid_svc}-pgdata"));
        tokio::fs::create_dir_all(&valid_vol_path).await.unwrap();
        tokio::fs::write(valid_vol_path.join("pg.dat"), b"active-db").await.unwrap();

        // 2b iii. Orphan service folder inside valid project
        let orphan_svc_path = valid_proj_path.join(orphan_svc.to_string());
        tokio::fs::create_dir_all(&orphan_svc_path).await.unwrap();

        // 3. Static site orphan
        let static_dir = tmp.join("static").join(orphan_svc.to_string());
        tokio::fs::create_dir_all(&static_dir).await.unwrap();
        tokio::fs::write(static_dir.join("index.html"), b"static-html").await.unwrap();

        // 4. Edge function orphan
        let edge_dir = tmp.join("edge").join(orphan_fn.to_string());
        tokio::fs::create_dir_all(&edge_dir).await.unwrap();
        tokio::fs::write(edge_dir.join("main.ts"), b"console.log()").await.unwrap();

        // 5. Unrecognized directory under shipyard
        let extra_dir = tmp.join("random_old_backup");
        tokio::fs::create_dir_all(&extra_dir).await.unwrap();

        // Sets for database state
        let mut active_org_set = std::collections::HashSet::new();
        active_org_set.insert(valid_org);

        let mut active_project_set = std::collections::HashMap::new();
        active_project_set.insert(valid_proj, valid_org);

        let mut active_service_set = std::collections::HashMap::new();
        active_service_set.insert(valid_svc, valid_proj);

        let mut active_volume_set = std::collections::HashSet::new();
        active_volume_set.insert((valid_svc, "pgdata".to_string()));

        let active_edge_set = std::collections::HashSet::new();

        let res = scan_filesystem_orphans_with_sets(
            &active_org_set,
            &active_project_set,
            &active_service_set,
            &active_volume_set,
            &active_edge_set,
            tmp.to_str().unwrap(),
        )
        .await
        .unwrap();

        // Verify detected orphans
        let org_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::Org).collect();
        assert_eq!(org_orphans.len(), 1);
        assert_eq!(org_orphans[0].identifier, orphan_org.to_string());

        let proj_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::Project).collect();
        assert_eq!(proj_orphans.len(), 1);
        assert_eq!(proj_orphans[0].identifier, orphan_proj.to_string());

        let vol_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::Volume).collect();
        assert_eq!(vol_orphans.len(), 1);
        assert!(vol_orphans[0].identifier.contains("untrackedvol"));

        let svc_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::Service).collect();
        // 3 service orphans: orphan_svc in project, orphan static site, and orphan edge function
        assert_eq!(svc_orphans.len(), 3);

        let dir_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::Directory).collect();
        assert_eq!(dir_orphans.len(), 1);
        assert_eq!(dir_orphans[0].identifier, "random_old_backup");

        // Clean up tempdir
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }
}
