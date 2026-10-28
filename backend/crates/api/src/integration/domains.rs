//! Choosing and changing an environment's public address.
//!
//! Every web environment gets `<subdomain>.<apps base domain>` on publish.
//! These endpoints let the product ask "is this name free?", get suggestions
//! when it isn't, and move an environment to a new name without a republish.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use super::{
    app_ref, db_err, env_project, org_by_external, random_alnum, IntegrationCaller, SCOPE_MANAGE,
};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/:org/domains/check", get(check_domain))
        .route(
            "/orgs/:org/apps/:app/envs/:env/domain",
            get(get_domain).put(change_domain),
        )
}

/// Names the platform keeps for itself, whatever the base domain.
const RESERVED: &[&str] = &[
    "www", "api", "admin", "app", "apps", "mail", "email", "smtp", "imap", "pop", "ftp", "ns", "ns1",
    "ns2", "dns", "dashboard", "console", "shipyard", "traefik", "registry", "status", "docs", "static",
    "assets", "cdn", "root", "login", "auth", "billing", "support", "help", "blog", "mqtt", "git",
    "localhost", "internal", "test",
];
const RESERVED_PREFIXES: &[&str] = &["preview-", "xn--"];

/// Normalizes a requested name (`" My-App "` → `my-app`) and says why it can't
/// be used when it can't.
pub fn normalize_label(input: &str) -> Result<String, String> {
    let label = input.trim().to_ascii_lowercase();
    if label.is_empty() {
        return Err("Enter a name".into());
    }
    if label.len() > 63 {
        return Err("A name can be at most 63 characters".into());
    }
    if !label
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("Use only letters a-z, digits 0-9 and '-'".into());
    }
    if label.starts_with('-') || label.ends_with('-') {
        return Err("A name can't start or end with '-'".into());
    }
    if label.len() < 3 {
        return Err("A name needs at least 3 characters".into());
    }
    if RESERVED.contains(&label.as_str()) || RESERVED_PREFIXES.iter().any(|p| label.starts_with(p)) {
        return Err(format!("'{label}' is reserved"));
    }
    Ok(label)
}

/// Alternatives to try, best first. `suffix` supplies the random tail so the
/// function stays deterministic in tests.
pub fn candidate_labels(label: &str, suffix: &mut dyn FnMut() -> String) -> Vec<String> {
    let clip = |s: String| -> String {
        let s: String = s.chars().take(63).collect();
        s.trim_end_matches('-').to_string()
    };
    let base: String = label.chars().take(50).collect();
    let base = base.trim_end_matches('-');
    let mut out = vec![
        clip(format!("{base}-app")),
        clip(format!("my-{base}")),
        clip(format!("{base}-hq")),
        clip(format!("get-{base}")),
        clip(format!("{base}-2")),
        clip(format!("{base}-{}", suffix())),
        clip(format!("{base}-{}", suffix())),
        clip(format!("{base}-{}", suffix())),
    ];
    out.retain(|c| normalize_label(c).is_ok());
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.clone()));
    out
}

struct Base(String);

fn base_domain(state: &AppState) -> Result<Base, ApiAppError> {
    state
        .config
        .integration
        .apps_base_domain
        .clone()
        .map(Base)
        .ok_or_else(|| {
            ApiAppError(AppError::BadRequest(
                "This Shipyard has no apps base domain configured (integration.apps_base_domain), so apps have no platform addresses".into(),
            ))
        })
}

async fn is_taken(state: &AppState, hostname: &str) -> Result<bool, ApiAppError> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM domains WHERE hostname = $1)")
        .bind(hostname)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)
}

/// Up to `want` free names near `label`, as `{subdomain, hostname}`.
async fn suggest(
    state: &AppState,
    base: &str,
    label: &str,
    want: usize,
) -> Result<Vec<serde_json::Value>, ApiAppError> {
    let mut out = vec![];
    for c in candidate_labels(label, &mut || random_alnum(4)) {
        let hostname = format!("{c}.{base}");
        if !is_taken(state, &hostname).await? {
            out.push(serde_json::json!({ "subdomain": c, "hostname": hostname }));
            if out.len() == want {
                break;
            }
        }
    }
    Ok(out)
}

/// The platform hostname (under the base domain) an environment serves on now.
struct Current {
    service_id: Uuid,
    domain_id: Uuid,
    hostname: String,
    port: Option<i32>,
}

async fn current_domain(
    state: &AppState,
    project_id: Uuid,
    base: &str,
) -> Result<Option<Current>, ApiAppError> {
    let row: Option<(Uuid, Uuid, String, Option<i32>)> = sqlx::query_as(
        "SELECT d.service_id, d.id, d.hostname, d.port
         FROM domains d JOIN services s ON s.id = d.service_id
         WHERE s.project_id = $1 AND d.hostname LIKE '%.' || $2
         ORDER BY (s.slug = 'web') DESC, d.created_at LIMIT 1",
    )
    .bind(project_id)
    .bind(base)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    Ok(row.map(|(service_id, domain_id, hostname, port)| Current {
        service_id,
        domain_id,
        hostname,
        port,
    }))
}

#[derive(Deserialize)]
struct CheckQuery {
    subdomain: String,
    /// With `app` and `env`: the environment's own current name counts as available.
    app: Option<String>,
    env: Option<String>,
}

/// GET /orgs/{org}/domains/check?subdomain=my-app[&app=todo&env=production]
async fn check_domain(
    caller: IntegrationCaller,
    Path(org): Path<String>,
    Query(q): Query<CheckQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    // The org must be reachable, but the answer is about the whole platform.
    org_by_external(&state.db, &caller, &org).await?;
    let base = base_domain(&state)?.0;

    let requested = q.subdomain.trim().to_ascii_lowercase();
    let label = match normalize_label(&q.subdomain) {
        Ok(l) => l,
        Err(reason) => {
            // Still helpful: offer valid names derived from what was typed.
            let seed: String = requested
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .trim_matches('-')
                .to_string();
            let suggestions = if seed.len() >= 3 {
                suggest(&state, &base, &seed, 5).await?
            } else {
                vec![]
            };
            return Ok(Json(ApiResponse::ok(serde_json::json!({
                "subdomain": requested, "hostname": null, "available": false,
                "reason": "invalid", "message": reason, "suggestions": suggestions,
            }))));
        }
    };
    let hostname = format!("{label}.{base}");

    let mut own = false;
    if let (Some(app), Some(env)) = (&q.app, &q.env) {
        let app = app_ref(&state.db, &caller, &org, app).await?;
        let project_id = env_project(&state.db, &app, env).await?;
        own = current_domain(&state, project_id, &base)
            .await?
            .is_some_and(|c| c.hostname == hostname);
    }
    if own {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "subdomain": label, "hostname": hostname, "available": true, "current": true,
            "suggestions": [],
        }))));
    }
    if is_taken(&state, &hostname).await? {
        let suggestions = suggest(&state, &base, &label, 5).await?;
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "subdomain": label, "hostname": hostname, "available": false,
            "reason": "taken", "message": format!("'{label}' is already in use"),
            "suggestions": suggestions,
        }))));
    }
    Ok(Json(ApiResponse::ok(serde_json::json!({
        "subdomain": label, "hostname": hostname, "available": true, "suggestions": [],
    }))))
}

/// GET /orgs/{org}/apps/{app}/envs/{env}/domain
async fn get_domain(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let base = base_domain(&state)?.0;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    let current = current_domain(&state, project_id, &base).await?;
    Ok(Json(ApiResponse::ok(match current {
        Some(c) => serde_json::json!({
            "hostname": c.hostname,
            "subdomain": c.hostname.strip_suffix(&format!(".{base}")),
            "url": format!("https://{}", c.hostname),
        }),
        None => serde_json::json!({ "hostname": null, "subdomain": null, "url": null }),
    })))
}

#[derive(Deserialize)]
struct ChangeDomain {
    subdomain: String,
}

/// PUT /orgs/{org}/apps/{app}/envs/{env}/domain  `{ "subdomain": "my-app" }`
///
/// Moves the environment to `<subdomain>.<base>`. The new name is attached
/// before the old one is removed, so the app never goes unreachable. A name
/// that is invalid or taken answers 409 with free alternatives and changes
/// nothing.
async fn change_domain(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
    Json(body): Json<ChangeDomain>,
) -> Result<Response, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let base = base_domain(&state)?.0;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;

    let unavailable = |reason: &str, message: String, suggestions: Vec<serde_json::Value>| {
        (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "data": { "available": false, "reason": reason, "suggestions": suggestions },
                "error": { "code": "CONFLICT", "message": message },
            })),
        )
            .into_response()
    };

    let label = match normalize_label(&body.subdomain) {
        Ok(l) => l,
        Err(message) => {
            let seed: String = body
                .subdomain
                .trim()
                .to_ascii_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .trim_matches('-')
                .to_string();
            let suggestions = if seed.len() >= 3 {
                suggest(&state, &base, &seed, 5).await?
            } else {
                vec![]
            };
            return Ok(unavailable("invalid", message, suggestions));
        }
    };
    let hostname = format!("{label}.{base}");

    let Some(current) = current_domain(&state, project_id, &base).await? else {
        return Err(ApiAppError(AppError::Conflict(
            "This environment has no platform address yet. Publish it first.".into(),
        )));
    };
    if current.hostname == hostname {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "hostname": hostname, "url": format!("https://{hostname}"),
            "previous_hostname": hostname, "changed": false,
        })))
        .into_response());
    }

    let taken_msg = format!("'{label}' is already in use");
    if is_taken(&state, &hostname).await? {
        let suggestions = suggest(&state, &base, &label, 5).await?;
        return Ok(unavailable("taken", taken_msg, suggestions));
    }

    match crate::resources::attach_domain(
        &state,
        current.service_id,
        &hostname,
        true,
        "letsencrypt",
        current.port,
    )
    .await
    {
        Ok(_) => {}
        // Lost a race with someone taking the same name.
        Err(ApiAppError(AppError::Conflict(_))) => {
            let suggestions = suggest(&state, &base, &label, 5).await?;
            return Ok(unavailable("taken", taken_msg, suggestions));
        }
        Err(e) => return Err(e),
    }
    crate::resources::detach_domain(&state, current.service_id, current.domain_id).await?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "hostname": hostname, "url": format!("https://{hostname}"),
        "previous_hostname": current.hostname, "changed": true,
    })))
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_rejects_names() {
        assert_eq!(normalize_label("  My-App ").unwrap(), "my-app");
        assert!(normalize_label("").is_err());
        assert!(normalize_label("ab").is_err());
        assert!(normalize_label("-bad").is_err());
        assert!(normalize_label("bad-").is_err());
        assert!(normalize_label("has space").is_err());
        assert!(normalize_label("under_score").is_err());
        assert!(normalize_label(&"a".repeat(64)).is_err());
        assert!(normalize_label("www").is_err());
        assert!(normalize_label("preview-abc").is_err());
        assert!(normalize_label("cool-shop").is_ok());
    }

    #[test]
    fn candidates_are_valid_distinct_and_keep_the_name() {
        let mut n = 0;
        let mut suffix = || {
            n += 1;
            format!("x{n}")
        };
        let c = candidate_labels("shop", &mut suffix);
        assert!(c.len() >= 5);
        assert_eq!(c[0], "shop-app");
        for name in &c {
            assert!(normalize_label(name).is_ok(), "{name}");
            assert!(name.starts_with("shop") || name.contains("shop"), "{name}");
        }
        let set: std::collections::HashSet<_> = c.iter().collect();
        assert_eq!(set.len(), c.len());
    }

    #[test]
    fn long_names_are_clipped_to_a_valid_label() {
        let name = format!("{}-", "a".repeat(62));
        let label = name.trim_end_matches('-');
        let c = candidate_labels(label, &mut || "zz".into());
        assert!(!c.is_empty());
        assert!(c.iter().all(|n| n.len() <= 63 && !n.ends_with('-')));
    }
}
