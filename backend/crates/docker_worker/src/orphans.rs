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
    Git,
    File,
    StaticFile,
    Registry,
}

impl OrphanType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrphanType::Org => "org",
            OrphanType::Project => "project",
            OrphanType::Service => "service",
            OrphanType::Volume => "volume",
            OrphanType::Directory => "directory",
            OrphanType::Git => "git",
            OrphanType::File => "file",
            OrphanType::StaticFile => "static_file",
            OrphanType::Registry => "registry",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "org" => Some(OrphanType::Org),
            "project" => Some(OrphanType::Project),
            "service" => Some(OrphanType::Service),
            "volume" => Some(OrphanType::Volume),
            "directory" => Some(OrphanType::Directory),
            "git" => Some(OrphanType::Git),
            "file" => Some(OrphanType::File),
            "static_file" | "static" => Some(OrphanType::StaticFile),
            "registry" => Some(OrphanType::Registry),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupSettings {
    pub orphan_retention_days: i32,
    pub orphan_auto_destruct_enabled: bool,
    pub registry_unused_retention_days: i32,
    pub registry_auto_removal_enabled: bool,
}

impl Default for CleanupSettings {
    fn default() -> Self {
        Self {
            orphan_retention_days: 30,
            orphan_auto_destruct_enabled: true,
            registry_unused_retention_days: 90, // 3 months default
            registry_auto_removal_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryCleanupReport {
    pub deleted_artifacts: usize,
    pub reclaimed_bytes: i64,
}

pub async fn get_cleanup_settings(db: &PgPool) -> CleanupSettings {
    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT value FROM system_config WHERE key = 'cleanup_settings'"
    )
    .fetch_optional(db)
    .await
    .unwrap_or(None);

    match row {
        Some((val,)) => serde_json::from_value(val).unwrap_or_default(),
        None => CleanupSettings::default(),
    }
}

pub async fn save_cleanup_settings(db: &PgPool, settings: &CleanupSettings) -> AppResult<()> {
    let val = serde_json::to_value(settings)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO system_config (key, value, updated_at)
        VALUES ('cleanup_settings', $1, NOW())
        ON CONFLICT (key) DO UPDATE
        SET value = EXCLUDED.value, updated_at = NOW()
        "#
    )
    .bind(val)
    .execute(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Update pending orphans' auto_destruct_at to match the newly configured retention days
    let _ = sqlx::query(
        "UPDATE orphans SET auto_destruct_at = detected_at + ($1 || ' days')::INTERVAL WHERE auto_destruct_at > NOW()"
    )
    .bind(settings.orphan_retention_days)
    .execute(db)
    .await;

    Ok(())
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
    pub auto_destruct_at: DateTime<Utc>,
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

    if path_str.starts_with("registry:") {
        let reg_id = path_str.trim_start_matches("registry:");
        return !reg_id.is_empty() && !reg_id.contains("..");
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
        let file_name = entry.file_name().to_string_lossy().to_string();

        if !entry_path.is_dir() {
            if file_name.starts_with('.') {
                continue;
            }
            let size = dir_size_bytes(&entry_path).await;
            let o_type = if file_name.ends_with(".git") {
                OrphanType::Git
            } else if file_name.ends_with(".zip") || file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") || file_name.ends_with(".tar") {
                OrphanType::StaticFile
            } else {
                OrphanType::File
            };
            orphans.push(DiscoveredOrphan {
                orphan_type: o_type,
                identifier: file_name,
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: size,
                details: Some("Loose file in shipyard data directory".to_string()),
            });
            continue;
        }

        let dir_name = file_name;

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
            "sources" | "git" => {
                scan_source_repos(&entry_path, &active_service_set, &mut orphans).await;
                continue;
            }
            "files" | "uploads" => {
                scan_file_artifacts(&entry_path, &active_service_set, active_org_set, &mut orphans).await;
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
    check_shipyard_root_directories(data_dir, active_service_set, active_org_set, &mut orphans).await;

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
        let name = entry.file_name().to_string_lossy().to_string();

        if name == "logs" || name.starts_with('.') {
            continue;
        }

        if name == "uploads" && entry_path.is_dir() {
            if let Ok(mut upload_entries) = tokio::fs::read_dir(&entry_path).await {
                while let Ok(Some(up_entry)) = upload_entries.next_entry().await {
                    let up_path = up_entry.path();
                    let up_name = up_entry.file_name().to_string_lossy().to_string();
                    if up_name.starts_with('.') {
                        continue;
                    }
                    if let Ok(svc_uuid) = Uuid::parse_str(&up_name) {
                        if !active_service_set.contains_key(&svc_uuid) {
                            let size = dir_size_bytes(&up_path).await;
                            orphans.push(DiscoveredOrphan {
                                orphan_type: OrphanType::StaticFile,
                                identifier: format!("uploads/{up_name}"),
                                path: up_path.to_string_lossy().to_string(),
                                size_bytes: size,
                                details: Some("Static site uploaded artifact belongs to deleted service".to_string()),
                            });
                        }
                    } else {
                        let size = dir_size_bytes(&up_path).await;
                        orphans.push(DiscoveredOrphan {
                            orphan_type: OrphanType::StaticFile,
                            identifier: format!("uploads/{up_name}"),
                            path: up_path.to_string_lossy().to_string(),
                            size_bytes: size,
                            details: Some("Untracked artifact in static uploads".to_string()),
                        });
                    }
                }
            }
            continue;
        }

        if entry_path.is_file() {
            let size = dir_size_bytes(&entry_path).await;
            orphans.push(DiscoveredOrphan {
                orphan_type: OrphanType::StaticFile,
                identifier: name,
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: size,
                details: Some("Loose file in static files directory".to_string()),
            });
            continue;
        }

        if let Ok(svc_uuid) = Uuid::parse_str(&name) {
            if !active_service_set.contains_key(&svc_uuid) {
                let size = dir_size_bytes(&entry_path).await;
                orphans.push(DiscoveredOrphan {
                    orphan_type: OrphanType::StaticFile,
                    identifier: name,
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("Static site deployed artifact belongs to deleted service".to_string()),
                });
            }
        } else {
            let size = dir_size_bytes(&entry_path).await;
            orphans.push(DiscoveredOrphan {
                orphan_type: OrphanType::StaticFile,
                identifier: name,
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: size,
                details: Some("Untracked directory in static directory".to_string()),
            });
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

        if name.starts_with('.') {
            continue;
        }

        if name == "work" && entry_path.is_dir() {
            if let Ok(mut work_entries) = tokio::fs::read_dir(&entry_path).await {
                while let Ok(Some(w_entry)) = work_entries.next_entry().await {
                    let w_path = w_entry.path();
                    let w_name = w_entry.file_name().to_string_lossy().to_string();
                    if w_name.starts_with('.') {
                        continue;
                    }
                    if let Ok(svc_uuid) = Uuid::parse_str(&w_name) {
                        if !active_service_set.contains_key(&svc_uuid) {
                            let size = dir_size_bytes(&w_path).await;
                            orphans.push(DiscoveredOrphan {
                                orphan_type: OrphanType::Git,
                                identifier: format!("work/{w_name}"),
                                path: w_path.to_string_lossy().to_string(),
                                size_bytes: size,
                                details: Some("Git working tree belongs to deleted service".to_string()),
                            });
                        }
                    } else {
                        let size = dir_size_bytes(&w_path).await;
                        orphans.push(DiscoveredOrphan {
                            orphan_type: OrphanType::Git,
                            identifier: format!("work/{w_name}"),
                            path: w_path.to_string_lossy().to_string(),
                            size_bytes: size,
                            details: Some("Untracked working tree directory in sources/work".to_string()),
                        });
                    }
                }
            }
            continue;
        }

        if let Some(svc_id_str) = name.strip_suffix(".git") {
            if let Ok(svc_uuid) = Uuid::parse_str(svc_id_str) {
                if !active_service_set.contains_key(&svc_uuid) {
                    let size = dir_size_bytes(&entry_path).await;
                    orphans.push(DiscoveredOrphan {
                        orphan_type: OrphanType::Git,
                        identifier: name,
                        path: entry_path.to_string_lossy().to_string(),
                        size_bytes: size,
                        details: Some("Git source repository belongs to deleted service".to_string()),
                    });
                }
            } else {
                let size = dir_size_bytes(&entry_path).await;
                orphans.push(DiscoveredOrphan {
                    orphan_type: OrphanType::Git,
                    identifier: name,
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("Untracked .git directory in sources".to_string()),
                });
            }
        } else if let Ok(svc_uuid) = Uuid::parse_str(&name) {
            if !active_service_set.contains_key(&svc_uuid) {
                let size = dir_size_bytes(&entry_path).await;
                orphans.push(DiscoveredOrphan {
                    orphan_type: OrphanType::Git,
                    identifier: name,
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("Git source directory belongs to deleted service".to_string()),
                });
            }
        } else {
            let size = dir_size_bytes(&entry_path).await;
            orphans.push(DiscoveredOrphan {
                orphan_type: OrphanType::Git,
                identifier: name,
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: size,
                details: Some("Untracked repository/directory in sources".to_string()),
            });
        }
    }
}

async fn scan_file_artifacts(
    files_path: &Path,
    active_service_set: &std::collections::HashMap<Uuid, Uuid>,
    active_org_set: &std::collections::HashSet<Uuid>,
    orphans: &mut Vec<DiscoveredOrphan>,
) {
    let mut entries = match tokio::fs::read_dir(files_path).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') {
            continue;
        }

        if let Ok(uuid) = Uuid::parse_str(&name) {
            if !active_service_set.contains_key(&uuid) && !active_org_set.contains(&uuid) {
                let size = dir_size_bytes(&entry_path).await;
                orphans.push(DiscoveredOrphan {
                    orphan_type: OrphanType::File,
                    identifier: name,
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("File artifact belongs to deleted service or organization".to_string()),
                });
            }
        } else {
            let size = dir_size_bytes(&entry_path).await;
            orphans.push(DiscoveredOrphan {
                orphan_type: OrphanType::File,
                identifier: name,
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: size,
                details: Some("Untracked file or folder in uploads/files".to_string()),
            });
        }
    }
}

async fn check_shipyard_root_directories(
    data_dir: &str,
    active_service_set: &std::collections::HashMap<Uuid, Uuid>,
    active_org_set: &std::collections::HashSet<Uuid>,
    orphans: &mut Vec<DiscoveredOrphan>,
) {
    let mut candidate_roots = Vec::new();

    if let Some(parent) = Path::new(data_dir).parent() {
        if parent.exists() && (parent.ends_with("shipyard") || parent.to_string_lossy() == "/shipyard") {
            candidate_roots.push(parent.to_path_buf());
        }
    }

    let explicit_roots = [Path::new("/shipyard"), Path::new("/opt/shipyard")];
    for r in explicit_roots {
        if r.exists() && !candidate_roots.contains(&r.to_path_buf()) {
            candidate_roots.push(r.to_path_buf());
        }
    }

    let known_root_dirs = ["data", "traefik", "backups", "ssl", "certs", "config", "logs"];

    for root in candidate_roots {
        let mut entries = match tokio::fs::read_dir(&root).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with('.') {
                continue;
            }

            if entry_path.is_dir() {
                if known_root_dirs.contains(&name.as_str()) {
                    continue;
                }
                if name == "sources" || name == "git" {
                    scan_source_repos(&entry_path, active_service_set, orphans).await;
                    continue;
                }
                if name == "static" {
                    scan_static_artifacts(&entry_path, active_service_set, orphans).await;
                    continue;
                }
                if name == "files" || name == "uploads" {
                    scan_file_artifacts(&entry_path, active_service_set, active_org_set, orphans).await;
                    continue;
                }

                let size = dir_size_bytes(&entry_path).await;
                let o_type = if name.ends_with(".git") || entry_path.join(".git").exists() || entry_path.join("HEAD").exists() {
                    OrphanType::Git
                } else {
                    OrphanType::Directory
                };
                orphans.push(DiscoveredOrphan {
                    orphan_type: o_type,
                    identifier: name,
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("Unrecognized directory directly under shipyard installation root".to_string()),
                });
            } else {
                // Loose file under shipyard root
                let size = dir_size_bytes(&entry_path).await;
                let o_type = if name.ends_with(".git") {
                    OrphanType::Git
                } else if name.ends_with(".zip") || name.ends_with(".tar.gz") || name.ends_with(".tgz") || name.ends_with(".tar") {
                    OrphanType::StaticFile
                } else {
                    OrphanType::File
                };
                orphans.push(DiscoveredOrphan {
                    orphan_type: o_type,
                    identifier: name,
                    path: entry_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    details: Some("Loose file directly under shipyard installation root".to_string()),
                });
            }
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

/// Scans for unused registry artifacts older than `unused_days` threshold.
pub async fn scan_unused_registry_artifacts(
    db: &PgPool,
    unused_days: i32,
) -> AppResult<Vec<DiscoveredOrphan>> {
    let query_str = r#"
        SELECT a.id, a.repo, a.tag, a.kind, a.size_bytes, a.pushed_at, ns.slug as namespace_slug
        FROM artifacts a
        JOIN registry_namespaces ns ON ns.id = a.namespace_id
        WHERE a.pushed_at <= NOW() - ($1 || ' days')::INTERVAL
          AND NOT EXISTS (
              SELECT 1 FROM service_artifact_sources sas
              WHERE sas.namespace_id = a.namespace_id
                AND sas.repo = a.repo
                AND sas.tag = a.tag
          )
          AND NOT EXISTS (
              SELECT 1 FROM services s
              WHERE s.image IS NOT NULL
                AND (
                    s.image LIKE '%' || a.repo || ':' || a.tag
                    OR s.image LIKE '%' || ns.slug || '/' || a.repo || ':' || a.tag
                )
          )
          AND NOT EXISTS (
              SELECT 1 FROM containers c
              WHERE c.image LIKE '%' || a.repo || ':' || a.tag
                 OR c.image LIKE '%' || ns.slug || '/' || a.repo || ':' || a.tag
          )
        ORDER BY a.pushed_at ASC
    "#;

    let rows: Vec<(Uuid, String, String, String, i64, DateTime<Utc>, String)> = sqlx::query_as(query_str)
        .bind(unused_days)
        .fetch_all(db)
        .await
        .unwrap_or_default();

    let mut orphans = Vec::new();
    for (id, repo, tag, kind, size_bytes, pushed_at, ns_slug) in rows {
        let age_days = (Utc::now() - pushed_at).num_days();
        orphans.push(DiscoveredOrphan {
            orphan_type: OrphanType::Registry,
            identifier: format!("{ns_slug}/{repo}:{tag}"),
            path: format!("registry:{id}"),
            size_bytes,
            details: Some(format!(
                "Registry {kind} image '{ns_slug}/{repo}:{tag}' has not been used by any service for {age_days} days"
            )),
        });
    }

    Ok(orphans)
}

/// Automatically removes unused registry artifacts older than the configured threshold.
pub async fn cleanup_unused_registry_artifacts(
    db: &PgPool,
    unused_days: i32,
) -> AppResult<RegistryCleanupReport> {
    let unused = scan_unused_registry_artifacts(db, unused_days).await?;
    let mut deleted_artifacts = 0;
    let mut reclaimed_bytes: i64 = 0;

    for item in unused {
        let artifact_id_str = item.path.trim_start_matches("registry:");
        if let Ok(art_uuid) = Uuid::parse_str(artifact_id_str) {
            info!("Auto-removing unused registry artifact {art_uuid} ({})", item.identifier);
            let res = sqlx::query("DELETE FROM artifacts WHERE id = $1")
                .bind(art_uuid)
                .execute(db)
                .await;
            if let Ok(r) = res {
                if r.rows_affected() > 0 {
                    deleted_artifacts += 1;
                    reclaimed_bytes += item.size_bytes;
                }
            }
        }
    }

    let _ = sqlx::query(
        "DELETE FROM registry_manifests WHERE digest NOT IN (SELECT manifest_digest FROM artifacts)"
    )
    .execute(db)
    .await;

    let _ = sqlx::query(
        "DELETE FROM registry_blobs WHERE ref_count <= 0"
    )
    .execute(db)
    .await;

    Ok(RegistryCleanupReport {
        deleted_artifacts,
        reclaimed_bytes,
    })
}

/// Walks the filesystem, checks Docker volumes, synchronizes with the `orphans` table,
/// and returns the current set of orphaned resources.
pub async fn scan_and_reconcile_orphans(
    db: &PgPool,
    data_dir: &str,
    docker: Option<(&dyn DockerEngine, &str)>,
) -> AppResult<Vec<OrphanResource>> {
    let settings = get_cleanup_settings(db).await;
    let mut discovered = scan_filesystem_orphans(db, data_dir).await?;

    if let Some((d, prefix)) = docker {
        scan_docker_orphan_volumes(db, d, prefix, &mut discovered).await;
    }

    if let Ok(mut reg_orphans) = scan_unused_registry_artifacts(db, settings.registry_unused_retention_days).await {
        discovered.append(&mut reg_orphans);
    }

    let mut current_paths = std::collections::HashSet::new();

    for item in &discovered {
        current_paths.insert(item.path.clone());
        let o_type = item.orphan_type.as_str();

        let retention_days = if item.orphan_type == OrphanType::Registry {
            settings.registry_unused_retention_days
        } else {
            settings.orphan_retention_days
        };

        sqlx::query(
            r#"
            INSERT INTO orphans (id, orphan_type, identifier, path, size_bytes, details, detected_at, auto_destruct_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW() + ($7 || ' days')::INTERVAL, NOW())
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
        .bind(retention_days)
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

    // Clean up unused registry artifacts if registry auto removal is enabled
    if settings.registry_auto_removal_enabled {
        let _ = cleanup_unused_registry_artifacts(db, settings.registry_unused_retention_days).await;
    }

    // Run auto-destruct on any orphans older than retention days if auto destruct is enabled
    if settings.orphan_auto_destruct_enabled {
        let _ = auto_destruct_expired_orphans(db, data_dir, docker.map(|(d, _)| d)).await;
    }

    list_orphans(db, None, None).await
}

/// Lists recorded orphans from the database.
pub async fn list_orphans(
    db: &PgPool,
    orphan_type: Option<&str>,
    search: Option<&str>,
) -> AppResult<Vec<OrphanResource>> {
    let mut q = "SELECT id, orphan_type, identifier, path, size_bytes, details, detected_at, auto_destruct_at, updated_at FROM orphans WHERE 1=1".to_string();

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
        .map(|(id, ot, identifier, path, size_bytes, details, detected_at, auto_destruct_at, updated_at)| {
            OrphanResource {
                id,
                orphan_type: OrphanType::from_str(&ot).unwrap_or(OrphanType::Directory),
                identifier,
                path,
                size_bytes,
                details,
                detected_at,
                auto_destruct_at,
                updated_at,
            }
        })
        .collect();

    Ok(items)
}

/// Permanently deletes all orphans that have passed their auto-destruct window.
pub async fn auto_destruct_expired_orphans(
    db: &PgPool,
    data_dir: &str,
    docker: Option<&dyn DockerEngine>,
) -> AppResult<usize> {
    let settings = get_cleanup_settings(db).await;
    if !settings.orphan_auto_destruct_enabled {
        info!("Orphan auto-destruct is disabled in admin settings");
        return Ok(0);
    }

    let expired: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, path FROM orphans WHERE auto_destruct_at <= NOW() ORDER BY auto_destruct_at ASC",
    )
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let mut count = 0;
    for (id, path) in expired {
        info!("Auto-destructing orphan {id} ({path}) after retention window");
        match delete_orphan(db, id, data_dir, docker).await {
            Ok(()) => count += 1,
            Err(e) => warn!("Failed to auto-destruct orphan {id} ({path}): {e}"),
        }
    }

    Ok(count)
}

/// Deletes an orphan resource from disk (or Docker / Registry) and removes its record from the database.
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
    } else if path.starts_with("registry:") {
        let art_id_str = path.trim_start_matches("registry:");
        if let Ok(art_uuid) = Uuid::parse_str(art_id_str) {
            info!("Removing orphaned registry artifact {art_uuid}");
            let _ = sqlx::query("DELETE FROM artifacts WHERE id = $1")
                .bind(art_uuid)
                .execute(db)
                .await;
            let _ = sqlx::query(
                "DELETE FROM registry_manifests WHERE digest NOT IN (SELECT manifest_digest FROM artifacts)"
            )
            .execute(db)
            .await;
            let _ = sqlx::query("DELETE FROM registry_blobs WHERE ref_count <= 0")
                .execute(db)
                .await;
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

        // Malicious registry names
        assert!(!is_safe_to_delete("registry:", data_dir));
        assert!(!is_safe_to_delete("registry:../escape", data_dir));
    }

    #[test]
    fn test_is_safe_to_delete_allows_valid_subpaths() {
        let data_dir = "/opt/shipyard/data";

        // Legitimate child directories
        assert!(is_safe_to_delete("/opt/shipyard/data/0190a4b2-0000-7000-8000-000000000001", data_dir));
        assert!(is_safe_to_delete("/opt/shipyard/data/0190a4b2-0000-7000-8000-000000000001/proj-1", data_dir));
        assert!(is_safe_to_delete("/opt/shipyard/data/0190a4b2-0000-7000-8000-000000000001/proj-1/svc-data", data_dir));
        assert!(is_safe_to_delete("/opt/shipyard/data/sources/my-service.git", data_dir));
        assert!(is_safe_to_delete("/opt/shipyard/data/static/uploads/my-service/bundle.zip", data_dir));
        assert!(is_safe_to_delete("/shipyard/files/stray.pdf", data_dir));
        assert!(is_safe_to_delete("/shipyard/unused_cache", data_dir));
        assert!(is_safe_to_delete("docker:platform-volume-123", data_dir));
        assert!(is_safe_to_delete("registry:0190a4b2-0000-7000-8000-000000000001", data_dir));
        assert!(is_safe_to_delete("registry:acme/web:v1.0.0", data_dir));
    }

    #[test]
    fn test_cleanup_settings_defaults_and_serialization() {
        let default_settings = CleanupSettings::default();
        assert_eq!(default_settings.orphan_retention_days, 30);
        assert!(default_settings.orphan_auto_destruct_enabled);
        assert_eq!(default_settings.registry_unused_retention_days, 90);
        assert!(default_settings.registry_auto_removal_enabled);

        let json = serde_json::to_string(&default_settings).unwrap();
        let parsed: CleanupSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, default_settings);
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
        // 2 service orphans: orphan_svc in project, and orphan edge function
        assert_eq!(svc_orphans.len(), 2);

        let static_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::StaticFile).collect();
        assert_eq!(static_orphans.len(), 1);
        assert_eq!(static_orphans[0].identifier, orphan_svc.to_string());

        let dir_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::Directory).collect();
        assert_eq!(dir_orphans.len(), 1);
        assert_eq!(dir_orphans[0].identifier, "random_old_backup");

        // Clean up tempdir
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn test_scan_filesystem_orphans_detects_git_files_and_static_files() {
        let tmp = std::env::temp_dir().join(format!("shipyard-git-file-test-{}", Uuid::now_v7()));
        tokio::fs::create_dir_all(&tmp).await.unwrap();

        let orphan_svc = Uuid::now_v7();
        let valid_svc = Uuid::now_v7();

        // 1. Git orphan in sources
        let git_orphan_path = tmp.join("sources").join(format!("{orphan_svc}.git"));
        tokio::fs::create_dir_all(&git_orphan_path).await.unwrap();
        tokio::fs::write(git_orphan_path.join("HEAD"), b"ref: refs/heads/main").await.unwrap();

        // 2. Git valid repo in sources
        let git_valid_path = tmp.join("sources").join(format!("{valid_svc}.git"));
        tokio::fs::create_dir_all(&git_valid_path).await.unwrap();
        tokio::fs::write(git_valid_path.join("HEAD"), b"ref: refs/heads/main").await.unwrap();

        // 3. Git worktree orphan in sources/work
        let git_work_orphan = tmp.join("sources").join("work").join(format!("{orphan_svc}"));
        tokio::fs::create_dir_all(&git_work_orphan).await.unwrap();

        // 4. Static uploads orphan
        let static_upload_orphan = tmp.join("static").join("uploads").join(format!("{orphan_svc}"));
        tokio::fs::create_dir_all(&static_upload_orphan).await.unwrap();
        tokio::fs::write(static_upload_orphan.join("archive.zip"), b"PK0304").await.unwrap();

        // 5. Loose static bundle file
        tokio::fs::write(tmp.join("static").join("bundle.tar.gz"), b"tar-content").await.unwrap();

        // 6. Loose file in files dir
        let file_dir = tmp.join("files");
        tokio::fs::create_dir_all(&file_dir).await.unwrap();
        tokio::fs::write(file_dir.join("orphaned-document.pdf"), b"%PDF-1.4").await.unwrap();

        // 7. Loose file directly under data_dir
        tokio::fs::write(tmp.join("stray_dump.sql"), b"SELECT 1;").await.unwrap();

        let active_org_set = std::collections::HashSet::new();
        let active_project_set = std::collections::HashMap::new();
        let mut active_service_set = std::collections::HashMap::new();
        active_service_set.insert(valid_svc, Uuid::now_v7());
        let active_volume_set = std::collections::HashSet::new();
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

        // Git orphans
        let git_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::Git).collect();
        assert_eq!(git_orphans.len(), 2, "Expected 2 git orphans (sources repo + worktree)");

        // Static file orphans
        let static_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::StaticFile).collect();
        assert_eq!(static_orphans.len(), 2, "Expected 2 static file orphans (uploads/<id> + bundle.tar.gz)");

        // File orphans
        let file_orphans: Vec<_> = res.iter().filter(|o| o.orphan_type == OrphanType::File).collect();
        assert_eq!(file_orphans.len(), 2, "Expected 2 file orphans (files/doc + stray_dump.sql)");

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }
}
