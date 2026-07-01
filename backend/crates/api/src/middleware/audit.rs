use uuid::Uuid;

/// Fire-and-forget audit log write. Never panics or returns an error to the caller.
pub async fn write_audit_log(
    db: &sqlx::PgPool,
    user_id: Option<Uuid>,
    action: &str,
    resource_type: Option<&str>,
    resource_id: Option<Uuid>,
    ip_address: Option<&str>,
    metadata: Option<serde_json::Value>,
) {
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs
               (id, user_id, action, resource_type, resource_id, ip_address, metadata, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())"#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(ip_address)
    .bind(metadata)
    .execute(db)
    .await;
}
