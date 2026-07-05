use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct TopologyNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopologyEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub edge_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopologyResponse {
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

// ─── Row types for sqlx ───────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct ServiceRow {
    id: Uuid,
    name: String,
    slug: String,
    status: String,
    replicas: i32,
    #[sqlx(rename = "type")]
    service_type: String,
    ports: serde_json::Value,
    service_parent_id: Option<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
struct NetworkRow {
    id: Uuid,
    name: String,
    driver: String,
}

#[derive(Debug, sqlx::FromRow)]
struct VolumeRow {
    id: Uuid,
    name: String,
    mount_path: String,
}

#[derive(Debug, sqlx::FromRow)]
struct DomainRow {
    id: Uuid,
    service_id: Uuid,
    hostname: String,
    tls_enabled: bool,
    port: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
struct ContainerRow {
    id: Uuid,
    service_id: Uuid,
    docker_container_id: String,
    replica_index: Option<i32>,
    status: String,
    image: Option<String>,
    node_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ServiceNetworkRow {
    service_id: Uuid,
    network_id: Uuid,
}

#[derive(Debug, sqlx::FromRow)]
struct VolumeServiceRow {
    id: Uuid,
    service_id: Uuid,
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new().route("/topology", get(get_topology))
}

// ─── Handler ─────────────────────────────────────────────────────────────────

/// GET /projects/:project_id/topology
async fn get_topology(
    _auth_user: AuthUser,
    Path(project_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<TopologyResponse>>, ApiAppError> {
    // Topology is always computed fresh from the DB — no caching — because
    // MQTT-triggered re-fetches can arrive within milliseconds of a cache
    // population (before containers or status changes are fully written to DB),
    // causing stale data to persist for the full TTL.  The 7 DB queries here
    // are simple index scans and complete well under 10 ms on local hardware.
    let db = &state.db;

    // Verify project exists
    let project_exists: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if project_exists.is_none() {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Project '{}' not found",
            project_id
        ))));
    }

    // 1. Query all services in the project
    let services = sqlx::query_as::<_, ServiceRow>(
        "SELECT id, name, slug, status, replicas, type::text AS type, ports, service_parent_id
         FROM services
         WHERE project_id = $1
         ORDER BY created_at ASC",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 2. Query all networks in the project
    let networks = sqlx::query_as::<_, NetworkRow>(
        "SELECT id, name, driver
         FROM networks
         WHERE project_id = $1
         ORDER BY created_at ASC",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 3. Query all volumes in the project (standalone or via service)
    let volumes = sqlx::query_as::<_, VolumeRow>(
        "SELECT v.id, v.name, v.mount_path
         FROM volumes v
         LEFT JOIN services s ON s.id = v.service_id
         WHERE v.project_id = $1
            OR s.project_id = $1
         ORDER BY v.created_at ASC",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 4. Query all domains in the project (via service -> project join)
    let domains = sqlx::query_as::<_, DomainRow>(
        "SELECT d.id, d.service_id, d.hostname, d.tls_enabled, d.port
         FROM domains d
         JOIN services s ON s.id = d.service_id
         WHERE s.project_id = $1
         ORDER BY d.created_at ASC",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 5. Query service_networks join for service ↔ network edges
    let service_networks = sqlx::query_as::<_, ServiceNetworkRow>(
        "SELECT sn.service_id, sn.network_id
         FROM service_networks sn
         JOIN networks n ON n.id = sn.network_id
         WHERE n.project_id = $1",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 6. Query service-attached volumes for service ↔ volume edges
    let volume_services = sqlx::query_as::<_, VolumeServiceRow>(
        "SELECT v.id, v.service_id
         FROM volumes v
         JOIN services s ON s.id = v.service_id
         WHERE s.project_id = $1
           AND v.service_id IS NOT NULL",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 7a. Query static site configs for static services in this project
    let static_configs: Vec<(Uuid, String, Option<serde_json::Value>)> = sqlx::query_as::<_, (Uuid, String, Option<serde_json::Value>)>(
        "SELECT sc.service_id, sc.source, sc.deploy_config
         FROM static_site_configs sc
         JOIN services s ON s.id = sc.service_id
         WHERE s.project_id = $1",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Map service_id → static config for O(1) lookup below
    let static_cfg_map: std::collections::HashMap<Uuid, (String, Option<serde_json::Value>)> = static_configs
        .into_iter()
        .map(|(id, source, deploy_config)| (id, (source, deploy_config)))
        .collect();

    // Query latest deployment status per static service for live status indicator
    let static_last_deploy: Vec<(Uuid, String)> = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT DISTINCT ON (d.service_id) d.service_id, d.status::text
         FROM deployments d
         JOIN services s ON s.id = d.service_id
         WHERE s.project_id = $1 AND s.type = 'static'
         ORDER BY d.service_id, d.created_at DESC",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let static_deploy_map: std::collections::HashMap<Uuid, String> = static_last_deploy
        .into_iter()
        .collect();

    // Build per-static-service domain list from already-fetched domains
    let mut static_domain_map: std::collections::HashMap<Uuid, Vec<String>> = std::collections::HashMap::new();
    for dom in &domains {
        // We'll populate this only for static services
        static_domain_map.entry(dom.service_id).or_default().push(dom.hostname.clone());
    }

    // 7. Query live containers (non-terminal) for container replica nodes
    let containers = sqlx::query_as::<_, ContainerRow>(
        "SELECT c.id, c.service_id, c.docker_container_id, c.replica_index,
                c.status::text AS status, c.image, c.node_id
         FROM containers c
         JOIN services s ON s.id = c.service_id
         WHERE s.project_id = $1
           AND c.status::text NOT IN ('orphan', 'complete', 'rejected')
         ORDER BY c.service_id, c.replica_index ASC NULLS LAST",
    )
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // ── Build nodes ───────────────────────────────────────────────────────────

    let mut nodes: Vec<TopologyNode> = Vec::new();

    for svc in &services {
        if svc.service_type == "static" {
            let (source, deploy_config) = static_cfg_map
                .get(&svc.id)
                .cloned()
                .unwrap_or_else(|| ("git".into(), None));
            let deploy_status = static_deploy_map.get(&svc.id).cloned().unwrap_or_else(|| "none".into());
            let site_domains = static_domain_map.get(&svc.id).cloned().unwrap_or_default();

            nodes.push(TopologyNode {
                id: format!("svc_{}", svc.id),
                node_type: "static_site".to_string(),
                data: serde_json::json!({
                    "name":           svc.name,
                    "slug":           svc.slug,
                    "status":         svc.status,
                    "source":         source,
                    "deploy_status":  deploy_status,
                    "domains":        site_domains,
                    "deploy_config":  deploy_config,
                }),
            });
        } else {
            nodes.push(TopologyNode {
                id: format!("svc_{}", svc.id),
                node_type: "service".to_string(),
                data: serde_json::json!({
                    "name":              svc.name,
                    "slug":              svc.slug,
                    "status":            svc.status,
                    "replicas":          svc.replicas,
                    "type":              svc.service_type,
                    "ports":             svc.ports,
                    "service_parent_id": svc.service_parent_id.map(|id| format!("svc_{id}")),
                }),
            });
        }
    }

    for net in &networks {
        nodes.push(TopologyNode {
            id: format!("net_{}", net.id),
            node_type: "network".to_string(),
            data: serde_json::json!({
                "name": net.name,
                "driver": net.driver,
            }),
        });
    }

    for vol in &volumes {
        nodes.push(TopologyNode {
            id: format!("vol_{}", vol.id),
            node_type: "volume".to_string(),
            data: serde_json::json!({
                "name": vol.name,
                "mount_path": vol.mount_path,
            }),
        });
    }

    for dom in &domains {
        nodes.push(TopologyNode {
            id: format!("dom_{}", dom.id),
            node_type: "domain".to_string(),
            data: serde_json::json!({
                "hostname":   dom.hostname,
                "tls_enabled": dom.tls_enabled,
                "port":       dom.port,
                "service_id": format!("svc_{}", dom.service_id),
            }),
        });
    }

    for ctr in &containers {
        let short_id = if ctr.docker_container_id.len() >= 12 {
            ctr.docker_container_id[..12].to_string()
        } else {
            ctr.docker_container_id.clone()
        };
        nodes.push(TopologyNode {
            id: format!("ctr_{}", ctr.id),
            node_type: "container".to_string(),
            data: serde_json::json!({
                "container_id": short_id,
                "replica_index": ctr.replica_index,
                "status": ctr.status,
                "image": ctr.image,
                "node_id": ctr.node_id,
                "service_id": format!("svc_{}", ctr.service_id),
            }),
        });
    }

    // ── Build edges ───────────────────────────────────────────────────────────

    // stable_edges are persisted to topology_edges; replica_edges are dynamic (not persisted)
    let mut stable_edges: Vec<TopologyEdge> = Vec::new();
    let mut replica_edges: Vec<TopologyEdge> = Vec::new();

    // parent → child service edges (docker_compose stacks)
    for svc in &services {
        if let Some(parent_id) = svc.service_parent_id {
            stable_edges.push(TopologyEdge {
                id: format!("e_parent_{}", svc.id),
                source: format!("svc_{parent_id}"),
                target: format!("svc_{}", svc.id),
                edge_type: "compose_child".to_string(),
            });
        }
    }

    // service ↔ network edges
    for sn in &service_networks {
        stable_edges.push(TopologyEdge {
            id: format!("e_{}", Uuid::now_v7()),
            source: format!("svc_{}", sn.service_id),
            target: format!("net_{}", sn.network_id),
            edge_type: "network".to_string(),
        });
    }

    // service ↔ volume edges
    for vs in &volume_services {
        stable_edges.push(TopologyEdge {
            id: format!("e_{}", Uuid::now_v7()),
            source: format!("svc_{}", vs.service_id),
            target: format!("vol_{}", vs.id),
            edge_type: "volume".to_string(),
        });
    }

    // domain → service edges
    for dom in &domains {
        stable_edges.push(TopologyEdge {
            id: format!("e_{}", Uuid::now_v7()),
            source: format!("dom_{}", dom.id),
            target: format!("svc_{}", dom.service_id),
            edge_type: "domain".to_string(),
        });
    }

    // service → container replica edges (dynamic, not persisted)
    for ctr in &containers {
        replica_edges.push(TopologyEdge {
            id: format!("e_ctr_{}", ctr.id),
            source: format!("svc_{}", ctr.service_id),
            target: format!("ctr_{}", ctr.id),
            edge_type: "replica".to_string(),
        });
    }

    // ── Persist stable edges to topology_edges table ──────────────────────────
    // Container replica edges are transient and excluded from persistence.

    sqlx::query("DELETE FROM topology_edges WHERE project_id = $1")
        .bind(project_id)
        .execute(db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    for edge in &stable_edges {
        sqlx::query(
            "INSERT INTO topology_edges (id, project_id, source_node_id, target_node_id, edge_type, created_at)
             VALUES ($1, $2, $3, $4, $5, NOW())",
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(&edge.source)
        .bind(&edge.target)
        .bind(&edge.edge_type)
        .execute(db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    let edges: Vec<TopologyEdge> = stable_edges.into_iter().chain(replica_edges).collect();

    let response = TopologyResponse { nodes, edges };
    Ok(Json(ApiResponse::ok(response)))
}
