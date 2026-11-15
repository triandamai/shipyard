use sqlx::PgPool;
use uuid::Uuid;

/// Asynchronously executes a project ownership transfer.
pub async fn execute_project_transfer(
    pool: &PgPool,
    transfer_id: Uuid,
    project_id: Uuid,
    target_org_id: Uuid,
    data_dir: &str,
) -> Result<(), String> {
    // 1. Fetch current project metadata
    let project_row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT org_id, slug, directory_path FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch project: {e}"))?;

    let (_current_org_id, original_slug, old_dir_path) = match project_row {
        Some(row) => row,
        None => {
            let msg = format!("Project '{}' not found", project_id);
            let _ = sqlx::query(
                "UPDATE project_transfers SET status = 'failed', error_message = $2, updated_at = NOW() WHERE id = $1",
            )
            .bind(transfer_id)
            .bind(&msg)
            .execute(pool)
            .await;
            return Err(msg);
        }
    };

    // 2. Resolve target slug collision if needed
    let mut target_slug = original_slug.clone();
    let mut counter = 1;
    loop {
        let exists: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM projects WHERE org_id = $1 AND slug = $2 AND id != $3",
        )
        .bind(target_org_id)
        .bind(&target_slug)
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Database error checking slug: {e}"))?;

        if exists.is_none() {
            break;
        }
        target_slug = format!("{original_slug}-{counter}");
        counter += 1;
    }

    // 3. Prepare target directory path
    let new_dir_path = format!("{}/{}/{}", data_dir, target_org_id, project_id);
    let old_path = std::path::Path::new(&old_dir_path);
    if old_path.exists() && old_dir_path != new_dir_path {
        let parent = std::path::Path::new(&new_dir_path).parent();
        if let Some(p) = parent {
            let _ = tokio::fs::create_dir_all(p).await;
        }
        if let Err(e) = tokio::fs::rename(&old_dir_path, &new_dir_path).await {
            tracing::warn!("Could not rename project directory from {} to {}: {}", old_dir_path, new_dir_path, e);
        }
    }

    // 4. Update database records in transaction
    let mut tx = pool.begin().await.map_err(|e| format!("Failed to begin tx: {e}"))?;

    // Update project row
    sqlx::query(
        "UPDATE projects SET org_id = $1, slug = $2, directory_path = $3, updated_at = NOW() WHERE id = $4",
    )
    .bind(target_org_id)
    .bind(&target_slug)
    .bind(&new_dir_path)
    .bind(project_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to update project: {e}"))?;

    // Update project_members: remove users not belonging to target org, update org_id for remainder
    sqlx::query(
        "DELETE FROM project_members WHERE project_id = $1 AND user_id NOT IN (SELECT user_id FROM org_members WHERE org_id = $2)",
    )
    .bind(project_id)
    .bind(target_org_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to prune project members: {e}"))?;

    sqlx::query(
        "UPDATE project_members SET org_id = $1 WHERE project_id = $2",
    )
    .bind(target_org_id)
    .bind(project_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to update project members org: {e}"))?;

    // Mark transfer completed
    sqlx::query(
        "UPDATE project_transfers SET status = 'completed', error_message = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(transfer_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to mark transfer completed: {e}"))?;

    tx.commit().await.map_err(|e| format!("Failed to commit tx: {e}"))?;

    Ok(())
}

/// Asynchronously executes a service transfer claim into target_project_id.
pub async fn execute_service_claim(
    pool: &PgPool,
    transfer_id: Uuid,
    service_id: Uuid,
    target_project_id: Uuid,
    target_org_id: Uuid,
) -> Result<(), String> {
    // 1. Verify service and target project
    let svc_row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT project_id, slug FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Database error fetching service: {e}"))?;

    let (source_project_id, original_slug) = match svc_row {
        Some(row) => row,
        None => {
            let msg = format!("Service '{}' not found", service_id);
            let _ = sqlx::query(
                "UPDATE service_transfers SET status = 'failed', error_message = $2, updated_at = NOW() WHERE id = $1",
            )
            .bind(transfer_id)
            .bind(&msg)
            .execute(pool)
            .await;
            return Err(msg);
        }
    };

    // Verify target project belongs to target org
    let target_proj_exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM projects WHERE id = $1 AND org_id = $2",
    )
    .bind(target_project_id)
    .bind(target_org_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Database error verifying target project: {e}"))?;

    if target_proj_exists.is_none() {
        let msg = format!("Target project '{}' does not belong to target organization", target_project_id);
        let _ = sqlx::query(
            "UPDATE service_transfers SET status = 'failed', error_message = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(transfer_id)
        .bind(&msg)
        .execute(pool)
        .await;
        return Err(msg);
    }

    // 2. Resolve service slug collision in target_project_id
    let mut target_slug = original_slug.clone();
    let mut counter = 1;
    loop {
        let exists: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM services WHERE project_id = $1 AND slug = $2 AND id != $3",
        )
        .bind(target_project_id)
        .bind(&target_slug)
        .bind(service_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Database error checking service slug: {e}"))?;

        if exists.is_none() {
            break;
        }
        target_slug = format!("{original_slug}-{counter}");
        counter += 1;
    }

    let mut tx = pool.begin().await.map_err(|e| format!("Failed to begin tx: {e}"))?;

    // 3. Move service to target project
    sqlx::query(
        "UPDATE services SET project_id = $1, slug = $2, updated_at = NOW() WHERE id = $3",
    )
    .bind(target_project_id)
    .bind(&target_slug)
    .bind(service_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to update service project: {e}"))?;

    // 4. Update attached volumes
    sqlx::query(
        "UPDATE volumes SET project_id = $1 WHERE service_id = $2",
    )
    .bind(target_project_id)
    .bind(service_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to update volume project: {e}"))?;

    // 5. Update connected networks:
    // For each network in source project attached to this service,
    // match by name in target project or create it in target project.
    let attached_networks: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT n.id, n.name, n.driver, n.subnet
         FROM service_networks sn
         JOIN networks n ON n.id = sn.network_id
         WHERE sn.service_id = $1",
    )
    .bind(service_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| format!("Failed to query attached networks: {e}"))?;

    for (old_net_id, net_name, net_driver, net_subnet) in attached_networks {
        let target_net_id: Uuid = match sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM networks WHERE project_id = $1 AND name = $2",
        )
        .bind(target_project_id)
        .bind(&net_name)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("Failed to find target network: {e}"))?
        {
            Some(id) => id,
            None => {
                let new_id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO networks (id, project_id, name, driver, subnet) VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(new_id)
                .bind(target_project_id)
                .bind(&net_name)
                .bind(&net_driver)
                .bind(&net_subnet)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("Failed to create network in target project: {e}"))?;
                new_id
            }
        };

        if target_net_id != old_net_id {
            sqlx::query(
                "DELETE FROM service_networks WHERE service_id = $1 AND network_id = $2",
            )
            .bind(service_id)
            .bind(old_net_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to remove old service_network: {e}"))?;

            sqlx::query(
                "INSERT INTO service_networks (service_id, network_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(service_id)
            .bind(target_net_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to insert new service_network: {e}"))?;
        }
    }

    // 6. Update canvas node positions in source and target projects
    let source_positions: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT node_positions FROM projects WHERE id = $1",
    )
    .bind(source_project_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    let svc_key = service_id.to_string();
    let moved_pos = if let Some(mut map) = source_positions {
        let pos = map.get(&svc_key).cloned();
        if let Some(obj) = map.as_object_mut() {
            obj.remove(&svc_key);
            let _ = sqlx::query(
                "UPDATE projects SET node_positions = $1 WHERE id = $2",
            )
            .bind(serde_json::Value::Object(obj.clone()))
            .bind(source_project_id)
            .execute(&mut *tx)
            .await;
        }
        pos
    } else {
        None
    };

    let target_positions: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT node_positions FROM projects WHERE id = $1",
    )
    .bind(target_project_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    let mut target_map = match target_positions {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    target_map.insert(
        svc_key,
        moved_pos.unwrap_or_else(|| serde_json::json!({ "x": 200, "y": 200 })),
    );

    let _ = sqlx::query(
        "UPDATE projects SET node_positions = $1 WHERE id = $2",
    )
    .bind(serde_json::Value::Object(target_map))
    .bind(target_project_id)
    .execute(&mut *tx)
    .await;

    // 7. Mark transfer completed
    sqlx::query(
        "UPDATE service_transfers SET status = 'completed', target_project_id = $1, error_message = NULL, updated_at = NOW() WHERE id = $2",
    )
    .bind(target_project_id)
    .bind(transfer_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to mark service transfer completed: {e}"))?;

    tx.commit().await.map_err(|e| format!("Failed to commit tx: {e}"))?;

    Ok(())
}
