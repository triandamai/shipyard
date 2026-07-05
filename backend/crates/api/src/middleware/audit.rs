use uuid::Uuid;

use crate::auth::AuthUser;

/// Fire-and-forget audit log write for authenticated requests.
/// Uses the actor fields from `AuthUser` so API key actions are attributed
/// to the key (actor_type = 'api_key') rather than silently merged with user actions.
pub async fn write_audit_log(
    db:            &sqlx::PgPool,
    auth:          &AuthUser,
    action:        &str,
    resource_type: Option<&str>,
    resource_id:   Option<Uuid>,
    ip_address:    Option<&str>,
    metadata:      Option<serde_json::Value>,
) {
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs
               (id, user_id, actor_type, actor_id, actor_name,
                action, resource_type, resource_id, ip_address, metadata, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())"#,
    )
    .bind(Uuid::now_v7())
    .bind(auth.user_id_opt())
    .bind(auth.actor_type())
    .bind(auth.actor_id())
    .bind(auth.actor_name())
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(ip_address)
    .bind(metadata)
    .execute(db)
    .await;
}

/// Variant for unauthenticated contexts (login, register, invite accept) where
/// a user_id and email are available directly but no `AuthUser` was built.
pub async fn write_audit_log_user(
    db:            &sqlx::PgPool,
    user_id:       Uuid,
    actor_name:    &str,
    action:        &str,
    resource_type: Option<&str>,
    resource_id:   Option<Uuid>,
    ip_address:    Option<&str>,
    metadata:      Option<serde_json::Value>,
) {
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs
               (id, user_id, actor_type, actor_id, actor_name,
                action, resource_type, resource_id, ip_address, metadata, created_at)
           VALUES ($1, $2, 'user', $2, $3, $4, $5, $6, $7, $8, NOW())"#,
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(actor_name)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(ip_address)
    .bind(metadata)
    .execute(db)
    .await;
}
