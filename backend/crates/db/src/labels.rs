//! Key/value labels on organizations, projects and services.
//!
//! Keys under [`SYSTEM_PREFIX`] are written by Shipyard only (which
//! integration manages the object, its app and environment, the deployed
//! version). Integrations send their own keys through the integration API.

use std::collections::BTreeMap;

use sqlx::PgPool;
use uuid::Uuid;

pub type Labels = BTreeMap<String, String>;

pub const SYSTEM_PREFIX: &str = "shipyard.io/";
pub const INTEGRATION: &str = "shipyard.io/integration";
pub const APP: &str = "shipyard.io/app";
pub const ENVIRONMENT: &str = "shipyard.io/environment";
pub const VERSION: &str = "shipyard.io/version";

pub const MAX_LABELS: usize = 32;
const MAX_KEY_LEN: usize = 63;
const MAX_VALUE_LEN: usize = 256;

#[derive(Debug, Clone, Copy)]
pub enum Table {
    Organizations,
    Projects,
    Services,
}

impl Table {
    fn name(self) -> &'static str {
        match self {
            Table::Organizations => "organizations",
            Table::Projects => "projects",
            Table::Services => "services",
        }
    }
}

/// Check labels sent by an integration. A `None` value removes the key.
pub fn validate(labels: &BTreeMap<String, Option<String>>) -> Result<(), String> {
    if labels.len() > MAX_LABELS {
        return Err(format!("at most {MAX_LABELS} labels are allowed"));
    }
    for (key, value) in labels {
        if key.starts_with(SYSTEM_PREFIX) {
            return Err(format!("label '{key}': keys under '{SYSTEM_PREFIX}' are set by Shipyard"));
        }
        let valid_key = !key.is_empty()
            && key.len() <= MAX_KEY_LEN
            && key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '/' | '_' | '-'));
        if !valid_key {
            return Err(format!(
                "label '{key}': keys must be 1-{MAX_KEY_LEN} characters of a-z, 0-9, '.', '/', '_', '-'"
            ));
        }
        if let Some(v) = value {
            if v.len() > MAX_VALUE_LEN {
                return Err(format!("label '{key}': values must be at most {MAX_VALUE_LEN} characters"));
            }
        }
    }
    Ok(())
}

/// Merge integration labels into a row: set keys with a value, remove keys
/// sent as `None`. Call [`validate`] first.
pub async fn apply(
    db: &PgPool,
    table: Table,
    id: Uuid,
    labels: &BTreeMap<String, Option<String>>,
) -> Result<(), sqlx::Error> {
    if labels.is_empty() {
        return Ok(());
    }
    let set: Labels = labels
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
        .collect();
    let remove: Vec<String> = labels
        .iter()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k.clone())
        .collect();
    sqlx::query(&format!(
        "UPDATE {} SET labels = (labels || $2) - $3::text[] WHERE id = $1",
        table.name()
    ))
    .bind(id)
    .bind(sqlx::types::Json(&set))
    .bind(&remove)
    .execute(db)
    .await?;
    Ok(())
}

/// Set Shipyard's own labels on a row, overwriting those keys.
pub async fn set_system(db: &PgPool, table: Table, id: Uuid, labels: &Labels) -> Result<(), sqlx::Error> {
    if labels.is_empty() {
        return Ok(());
    }
    sqlx::query(&format!(
        "UPDATE {} SET labels = labels || $2 WHERE id = $1",
        table.name()
    ))
    .bind(id)
    .bind(sqlx::types::Json(labels))
    .execute(db)
    .await?;
    Ok(())
}

/// The version of an image reference: its tag (`shipyard/web:3f2a91c` →
/// `3f2a91c`). None for digest references and refs without a tag.
pub fn image_version(image_ref: &str) -> Option<String> {
    if image_ref.contains('@') {
        return None;
    }
    let last = image_ref.rsplit('/').next()?;
    let (_, tag) = last.rsplit_once(':')?;
    (!tag.is_empty()).then(|| tag.to_string())
}

/// Record the version a service now runs.
pub async fn set_version(db: &PgPool, service_id: Uuid, version: &str) -> Result<(), sqlx::Error> {
    set_system(
        db,
        Table::Services,
        service_id,
        &Labels::from([(VERSION.to_string(), version.to_string())]),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_version_takes_the_tag() {
        assert_eq!(image_version("shipyard/web:3f2a91c").as_deref(), Some("3f2a91c"));
        assert_eq!(image_version("registry.local:5000/ns/web:1.4.2").as_deref(), Some("1.4.2"));
        assert_eq!(image_version("registry.local:5000/ns/web"), None);
        assert_eq!(image_version("nginx"), None);
        assert_eq!(image_version("nginx@sha256:abc"), None);
    }

    #[test]
    fn validate_rejects_system_and_bad_keys() {
        let ok = BTreeMap::from([("version".to_string(), Some("1.0".to_string()))]);
        assert!(validate(&ok).is_ok());
        let system = BTreeMap::from([(VERSION.to_string(), Some("1".to_string()))]);
        assert!(validate(&system).is_err());
        let upper = BTreeMap::from([("Version".to_string(), None)]);
        assert!(validate(&upper).is_err());
    }
}
