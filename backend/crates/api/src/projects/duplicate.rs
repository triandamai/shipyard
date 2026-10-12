//! Duplicating a project: same services, settings, env vars and networks in a
//! new project, ready to deploy as another environment.
//!
//! Copied: services (config, image, ports, resources, git settings), env vars,
//! project networks (new Docker networks), artifact sources, node pins, and
//! volume definitions under new names, so the copy starts with **empty data**.
//! Not copied: domains (hostnames are unique), deployments, sandbox apps,
//! compose child services (recreated by the stack's first deploy).

use std::collections::HashMap;

use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_db::events::{self, Scope};

use super::db_error;
use crate::error::ApiAppError;
use crate::AppState;

#[derive(Debug, Clone, Default)]
pub struct DuplicateTarget {
    pub name: String,
    pub slug: String,
    /// Set by integrations: the app and environment the copy belongs to.
    pub app_id: Option<Uuid>,
    pub environment: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct DuplicateReport {
    pub project_id: Uuid,
    /// Source service id → new service id.
    pub services: HashMap<Uuid, Uuid>,
    pub networks: usize,
    pub volumes: usize,
    /// Hostnames on the source that the copy doesn't have.
    pub domains_not_copied: Vec<String>,
    pub skipped_services: Vec<String>,
}

/// Columns of `table` in the current schema, in order.
async fn columns(db: &sqlx::PgPool, table: &str) -> Result<Vec<String>, ApiAppError> {
    sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns
         WHERE table_schema = current_schema() AND table_name = $1 ORDER BY ordinal_position",
    )
    .bind(table)
    .fetch_all(db)
    .await
    .map_err(db_error)
}

/// `INSERT INTO table (all columns) SELECT ... FROM table WHERE id = $1`,
/// with some columns replaced by SQL expressions. Column names come from
/// information_schema and are quoted; override expressions are fixed strings
/// from this file, with values passed as binds.
fn copy_row_sql(table: &str, cols: &[String], overrides: &HashMap<&str, &str>) -> String {
    let list = cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let exprs = cols
        .iter()
        .map(|c| {
            overrides
                .get(c.as_str())
                .map(|e| e.to_string())
                .unwrap_or_else(|| format!("\"{c}\""))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("INSERT INTO {table} ({list}) SELECT {exprs} FROM {table} WHERE id = $1")
}

/// New name for a copied Docker object: the source service id in auto-made
/// volume names becomes the new id; anything else gets a short suffix from
/// the new service.
pub fn rename_for_copy(name: &str, old_service: Uuid, new_service: Uuid) -> String {
    let old = old_service.to_string();
    if name.contains(&old) {
        name.replace(&old, &new_service.to_string())
    } else {
        format!("{name}-{}", short_id(new_service))
    }
}

/// Last 8 hex chars of a UUID. For v7 ids these are random; the first 8 are
/// the timestamp and repeat for about a minute.
pub fn short_id(id: Uuid) -> String {
    id.simple().to_string()[24..].to_string()
}

pub async fn duplicate_project(
    state: &AppState,
    source_project_id: Uuid,
    target: DuplicateTarget,
) -> Result<DuplicateReport, ApiAppError> {
    let (org_id, src_name): (Uuid, String) =
        sqlx::query_as("SELECT org_id, name FROM projects WHERE id = $1")
            .bind(source_project_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_error)?
            .ok_or_else(|| ApiAppError(AppError::NotFound("Source project not found".into())))?;

    let new_project = Uuid::now_v7();
    let directory_path = format!("{}/{}/{}", state.config.data_dir, org_id, new_project);
    sqlx::query(
        "INSERT INTO projects (id, org_id, name, slug, directory_path, app_id, environment, duplicated_from, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())",
    )
    .bind(new_project)
    .bind(org_id)
    .bind(&target.name)
    .bind(&target.slug)
    .bind(&directory_path)
    .bind(target.app_id)
    .bind(&target.environment)
    .bind(source_project_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate") {
            ApiAppError(AppError::Conflict(format!("A project with slug '{}' already exists", target.slug)))
        } else {
            db_error(e)
        }
    })?;

    let result = copy_contents(
        state,
        org_id,
        source_project_id,
        new_project,
        &directory_path,
    )
    .await;
    let report = match result {
        Ok(r) => r,
        Err(e) => {
            // Leave nothing half-copied behind.
            let _ = super::teardown::teardown_project(
                state,
                new_project,
                super::teardown::TeardownOptions {
                    remove_volumes: true,
                },
            )
            .await;
            return Err(e);
        }
    };

    events::emit(
        &state.db,
        Scope::project(org_id, new_project),
        "project.duplicated",
        serde_json::json!({
            "from_project_id": source_project_id,
            "from_name": src_name,
            "services": report.services.len(),
            "domains_not_copied": report.domains_not_copied,
        }),
    )
    .await;
    Ok(report)
}

async fn copy_contents(
    state: &AppState,
    org_id: Uuid,
    src: Uuid,
    dst: Uuid,
    dst_dir: &str,
) -> Result<DuplicateReport, ApiAppError> {
    let db = &state.db;
    let mut report = DuplicateReport {
        project_id: dst,
        services: HashMap::new(),
        networks: 0,
        volumes: 0,
        domains_not_copied: vec![],
        skipped_services: vec![],
    };

    // ── Networks ─────────────────────────────────────────────────────────────
    let mut network_map: HashMap<Uuid, Uuid> = HashMap::new();
    let nets: Vec<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT id, name, driver, subnet FROM networks WHERE project_id = $1")
            .bind(src)
            .fetch_all(db)
            .await
            .map_err(db_error)?;
    for (old_id, name, driver, subnet) in nets {
        let new_name = format!("{name}-{}", short_id(dst));
        let docker_id = state
            .docker
            .create_network(&new_name, &driver)
            .await
            .map_err(|e| ApiAppError(AppError::Docker(e.to_string())))?;
        let new_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO networks (id, project_id, name, driver, subnet, docker_network_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, NOW())",
        )
        .bind(new_id)
        .bind(dst)
        .bind(&new_name)
        .bind(&driver)
        .bind(&subnet)
        .bind(&docker_id)
        .execute(db)
        .await
        .map_err(db_error)?;
        network_map.insert(old_id, new_id);
        report.networks += 1;
    }

    // ── Services ─────────────────────────────────────────────────────────────
    let services: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, slug, type::text FROM services
         WHERE project_id = $1 AND service_parent_id IS NULL ORDER BY created_at",
    )
    .bind(src)
    .fetch_all(db)
    .await
    .map_err(db_error)?;

    let sql = service_copy_sql(db).await?;
    for (old_id, slug, svc_type) in services {
        if svc_type == "sandbox_app" {
            report.skipped_services.push(slug);
            continue;
        }
        let copied = copy_service(
            state,
            &sql,
            org_id,
            old_id,
            dst,
            dst_dir,
            None,
            &NetworkTarget::Map(&network_map),
        )
        .await?;
        report.services.insert(old_id, copied.service_id);
        report.volumes += copied.volumes;
        report.domains_not_copied.extend(copied.domains_not_copied);
    }

    Ok(report)
}

/// Where a copied service's network memberships go.
pub enum NetworkTarget<'a> {
    /// Same project: keep the same networks.
    Same,
    /// Source network id → copy's network id; unmapped networks are dropped.
    Map(&'a HashMap<Uuid, Uuid>),
}

pub struct CopiedService {
    pub service_id: Uuid,
    pub name: String,
    pub slug: String,
    pub volumes: usize,
    pub networks_dropped: usize,
    pub domains_not_copied: Vec<String>,
}

/// INSERT ... SELECT copying one service row, with id, project, directory,
/// name and slug supplied as $2..$6.
pub async fn service_copy_sql(db: &sqlx::PgPool) -> Result<String, ApiAppError> {
    let cols = columns(db, "services").await?;
    let mut overrides: HashMap<&str, &str> = HashMap::new();
    overrides.insert("id", "$2");
    overrides.insert("project_id", "$3");
    overrides.insert("directory_path", "$4");
    overrides.insert("name", "$5");
    overrides.insert("slug", "$6");
    overrides.insert("status", "'stopped'");
    overrides.insert("service_parent_id", "NULL");
    overrides.insert("created_at", "NOW()");
    overrides.insert("updated_at", "NOW()");
    Ok(copy_row_sql("services", &cols, &overrides))
}

/// Copy one service into `dst` project: config, env vars and secret bindings,
/// volumes under new names (empty data), network membership, artifact source
/// and node pin. `rename` gives the copy a new name and slug; without it the
/// source's are kept (only valid in another project).
#[allow(clippy::too_many_arguments)]
pub async fn copy_service(
    state: &AppState,
    sql: &str,
    org_id: Uuid,
    old_id: Uuid,
    dst: Uuid,
    dst_dir: &str,
    rename: Option<(&str, &str)>,
    networks: &NetworkTarget<'_>,
) -> Result<CopiedService, ApiAppError> {
    let db = &state.db;
    let (src_name, src_slug, svc_type): (String, String, String) =
        sqlx::query_as("SELECT name, slug, type::text FROM services WHERE id = $1")
            .bind(old_id)
            .fetch_one(db)
            .await
            .map_err(db_error)?;
    let (name, slug) = match rename {
        Some((n, s)) => (n.to_string(), s.to_string()),
        None => (src_name, src_slug),
    };

    let new_id = Uuid::now_v7();
    let dir = format!("{dst_dir}/{new_id}/{slug}");
    sqlx::query(sql)
        .bind(old_id)
        .bind(new_id)
        .bind(dst)
        .bind(&dir)
        .bind(&name)
        .bind(&slug)
        .execute(db)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate") {
                ApiAppError(AppError::Conflict(format!(
                    "A service with slug '{slug}' already exists in that project"
                )))
            } else {
                db_error(e)
            }
        })?;

    // Env vars (encrypted values copied as-is) and secret bindings.
    sqlx::query(
        "INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
         SELECT gen_random_uuid(), $2, key, value_encrypted, is_secret, NOW()
         FROM service_envs WHERE service_id = $1",
    )
    .bind(old_id)
    .bind(new_id)
    .execute(db)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO secret_bindings (service_id, env_key, secret_id)
         SELECT $2, env_key, secret_id FROM secret_bindings WHERE service_id = $1",
    )
    .bind(old_id)
    .bind(new_id)
    .execute(db)
    .await
    .map_err(db_error)?;

    // Volumes, renamed so the copy gets its own (empty) data.
    let vols: Vec<(String, String, String, i32)> = sqlx::query_as(
        "SELECT name, mount_path, driver, size_mb FROM volumes WHERE service_id = $1",
    )
    .bind(old_id)
    .fetch_all(db)
    .await
    .map_err(db_error)?;
    let volumes = vols.len();
    for (vname, mount_path, driver, size_mb) in vols {
        sqlx::query(
            "INSERT INTO volumes (id, service_id, name, mount_path, driver, size_mb, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, NOW())",
        )
        .bind(Uuid::now_v7())
        .bind(new_id)
        .bind(rename_for_copy(&vname, old_id, new_id))
        .bind(&mount_path)
        .bind(&driver)
        .bind(size_mb)
        .execute(db)
        .await
        .map_err(db_error)?;
    }

    // Network membership.
    let memberships: Vec<Uuid> =
        sqlx::query_scalar("SELECT network_id FROM service_networks WHERE service_id = $1")
            .bind(old_id)
            .fetch_all(db)
            .await
            .map_err(db_error)?;
    let mut networks_dropped = 0;
    for old_net in memberships {
        let target = match networks {
            NetworkTarget::Same => Some(old_net),
            NetworkTarget::Map(m) => m.get(&old_net).copied(),
        };
        match target {
            Some(net) => {
                sqlx::query("INSERT INTO service_networks (service_id, network_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                    .bind(new_id)
                    .bind(net)
                    .execute(db)
                    .await
                    .map_err(db_error)?;
            }
            None => networks_dropped += 1,
        }
    }

    // Artifact source and node pin.
    sqlx::query(
        "INSERT INTO service_artifact_sources (service_id, namespace_id, repo, tag)
         SELECT $2, namespace_id, repo, tag FROM service_artifact_sources WHERE service_id = $1",
    )
    .bind(old_id)
    .bind(new_id)
    .execute(db)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO service_node_assignments (service_id, node_id, assigned_at)
         SELECT $2, node_id, NOW() FROM service_node_assignments WHERE service_id = $1
         ON CONFLICT (service_id) DO NOTHING",
    )
    .bind(old_id)
    .bind(new_id)
    .execute(db)
    .await
    .map_err(db_error)?;

    let domains_not_copied: Vec<String> =
        sqlx::query_scalar("SELECT hostname FROM domains WHERE service_id = $1")
            .bind(old_id)
            .fetch_all(db)
            .await
            .map_err(db_error)?;

    events::emit(
        db,
        Scope::service(org_id, dst, new_id),
        "service.created",
        serde_json::json!({ "slug": slug, "type": svc_type, "duplicated_from": old_id }),
    )
    .await;

    Ok(CopiedService {
        service_id: new_id,
        name,
        slug,
        volumes,
        networks_dropped,
        domains_not_copied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_sql_quotes_columns_and_applies_overrides() {
        let cols = vec![
            "id".to_string(),
            "project_id".to_string(),
            "name".to_string(),
        ];
        let mut o = HashMap::new();
        o.insert("id", "$2");
        o.insert("project_id", "$3");
        assert_eq!(
            copy_row_sql("services", &cols, &o),
            "INSERT INTO services (\"id\", \"project_id\", \"name\") SELECT $2, $3, \"name\" FROM services WHERE id = $1"
        );
    }

    #[test]
    fn auto_volume_names_follow_the_new_service() {
        let old = Uuid::parse_str("0190a4b2-0000-7000-8000-000000000001").unwrap();
        let new = Uuid::parse_str("0190a4b2-0000-7000-8000-000000000002").unwrap();
        assert_eq!(
            rename_for_copy(&format!("platform-{old}-data"), old, new),
            format!("platform-{new}-data")
        );
        assert_eq!(rename_for_copy("pgdata", old, new), "pgdata-00000002");
    }
}
