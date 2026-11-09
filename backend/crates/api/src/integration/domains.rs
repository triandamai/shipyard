//! Choosing and changing an environment's public address.
//!
//! Every web environment gets a hostname on publish: the full `hostname` the
//! manifest asks for, or `<subdomain>.<apps base domain>`. These endpoints let
//! the product ask "is this name free?", get suggestions when it isn't, and
//! move an environment to a new name without a republish. Hostnames are full
//! domains, so the product can move its apps to another domain at any time.

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

/// The apps base domain, when the operator configured one. Only needed for
/// `subdomain` requests; a full `hostname` works without it.
fn base_domain(state: &AppState) -> Option<String> {
    state.config.integration.apps_base_domain.clone()
}

fn require_base(state: &AppState) -> Result<String, ApiAppError> {
    base_domain(state).ok_or_else(|| {
        ApiAppError(AppError::BadRequest(
            "This Shipyard has no apps base domain configured (integration.apps_base_domain), so a `subdomain` can't be turned into a hostname. Send a full `hostname` instead".into(),
        ))
    })
}

/// Normalizes a full hostname (`" Shop.Example.COM. "` → `shop.example.com`)
/// and says why it can't be used when it can't. A direct child of the apps
/// base domain also has to pass [`normalize_label`] (length, reserved names).
pub fn normalize_hostname(input: &str, base: Option<&str>) -> Result<String, String> {
    let host = input.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err("Enter a domain".into());
    }
    if host.len() > 253 {
        return Err("A domain can be at most 253 characters".into());
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return Err("Enter a full domain, e.g. my-app.example.com".into());
    }
    for l in &labels {
        if l.is_empty() || l.len() > 63 {
            return Err("Each part of a domain must be 1 to 63 characters".into());
        }
        if !l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return Err("Use only letters a-z, digits 0-9, '-' and '.'".into());
        }
        if l.starts_with('-') || l.ends_with('-') {
            return Err("A part of a domain can't start or end with '-'".into());
        }
    }
    if labels.last().is_some_and(|tld| tld.bytes().all(|b| b.is_ascii_digit())) {
        return Err("Use a domain name, not an IP address".into());
    }
    if let Some(base) = base {
        if host == base {
            return Err(format!("'{host}' is the platform's base domain"));
        }
        if let Some(label) = host.strip_suffix(&format!(".{base}")) {
            if !label.contains('.') {
                normalize_label(label)?;
            }
        }
    }
    Ok(host)
}

/// `(first label, parent)` of a hostname: `shop.example.com` → `("shop", "example.com")`.
fn split_host(hostname: &str) -> (&str, &str) {
    hostname.split_once('.').unwrap_or((hostname, ""))
}

/// The label typed in a rejected input, made safe enough to seed suggestions.
fn seed_label(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

async fn is_taken(state: &AppState, hostname: &str) -> Result<bool, ApiAppError> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM domains WHERE hostname = $1)")
        .bind(hostname)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)
}

/// Up to `want` free names near `label` under `parent`, as `{subdomain, hostname}`.
async fn suggest(
    state: &AppState,
    parent: &str,
    label: &str,
    want: usize,
) -> Result<Vec<serde_json::Value>, ApiAppError> {
    let mut out = vec![];
    for c in candidate_labels(label, &mut || random_alnum(4)) {
        let hostname = format!("{c}.{parent}");
        if hostname.len() > 253 {
            continue;
        }
        if !is_taken(state, &hostname).await? {
            out.push(serde_json::json!({ "subdomain": c, "hostname": hostname }));
            if out.len() == want {
                break;
            }
        }
    }
    Ok(out)
}

/// Suggestions for an input that isn't a valid name: in hostname mode keep the
/// parent domain when what's after the first dot is usable, else fall back to
/// the apps base domain.
async fn suggest_for_invalid(
    state: &AppState,
    input: &str,
    hostname_mode: bool,
) -> Result<Vec<serde_json::Value>, ApiAppError> {
    let base = base_domain(state);
    let (seed, parent) = if hostname_mode {
        let trimmed = input.trim().trim_end_matches('.').to_ascii_lowercase();
        let (first, rest) = split_host(&trimmed);
        let parent = normalize_hostname(rest, None).ok().or(base);
        (seed_label(first), parent)
    } else {
        (seed_label(input), base)
    };
    match parent {
        Some(parent) if seed.len() >= 3 => suggest(state, &parent, &seed, 5).await,
        _ => Ok(vec![]),
    }
}

/// The hostname the integration manages for an environment (its platform
/// address): flagged `integration_managed`, or from before that flag, under
/// the apps base domain.
pub(crate) const MANAGED_SQL: &str =
    "(d.integration_managed OR ($2::text IS NOT NULL AND d.hostname LIKE '%.' || $2))";

/// Flags a domain row as the environment's managed address.
pub(crate) async fn mark_managed(state: &AppState, domain_id: Uuid) -> Result<(), ApiAppError> {
    sqlx::query("UPDATE domains SET integration_managed = TRUE WHERE id = $1")
        .bind(domain_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// The platform hostname an environment serves on now.
struct Current {
    service_id: Uuid,
    domain_id: Uuid,
    hostname: String,
    port: Option<i32>,
}

async fn current_domain(
    state: &AppState,
    project_id: Uuid,
    base: Option<&str>,
) -> Result<Option<Current>, ApiAppError> {
    let row: Option<(Uuid, Uuid, String, Option<i32>)> = sqlx::query_as(&format!(
        "SELECT d.service_id, d.id, d.hostname, d.port
         FROM domains d JOIN services s ON s.id = d.service_id
         WHERE s.project_id = $1 AND {MANAGED_SQL} AND s.slug <> 'relay'
         ORDER BY (s.slug = 'web') DESC, d.integration_managed DESC, d.created_at LIMIT 1"
    ))
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

/// What the caller asked for: a full `hostname` (preferred) or a `subdomain`
/// under the apps base domain (legacy).
enum Requested {
    Hostname(String),
    Subdomain(String),
}

impl Requested {
    fn from(hostname: Option<String>, subdomain: Option<String>) -> Result<Self, ApiAppError> {
        match (hostname, subdomain) {
            (Some(h), None) => Ok(Self::Hostname(h)),
            (None, Some(s)) => Ok(Self::Subdomain(s)),
            (Some(_), Some(_)) => Err(ApiAppError(AppError::BadRequest(
                "Send either `hostname` or `subdomain`, not both".into(),
            ))),
            (None, None) => Err(ApiAppError(AppError::BadRequest(
                "Send `hostname` (a full domain, e.g. my-app.example.com)".into(),
            ))),
        }
    }

    fn input(&self) -> &str {
        match self {
            Self::Hostname(s) | Self::Subdomain(s) => s,
        }
    }

    /// The normalized hostname, or why it can't be used.
    fn resolve(&self, state: &AppState) -> Result<Result<String, String>, ApiAppError> {
        let base = base_domain(state);
        Ok(match self {
            Self::Hostname(h) => normalize_hostname(h, base.as_deref()),
            Self::Subdomain(s) => {
                let base = require_base(state)?;
                normalize_label(s).map(|l| format!("{l}.{base}"))
            }
        })
    }
}

/// `subdomain` in responses: the first label when the hostname is directly
/// under the apps base domain, else null.
fn subdomain_of(hostname: &str, base: Option<&str>) -> Option<String> {
    let label = hostname.strip_suffix(&format!(".{}", base?))?;
    (!label.contains('.')).then(|| label.to_string())
}

#[derive(Deserialize)]
struct CheckQuery {
    /// Full domain, e.g. `my-app.example.com`.
    hostname: Option<String>,
    /// Legacy: first label under the apps base domain.
    subdomain: Option<String>,
    /// With `app` and `env`: the environment's own current name counts as available.
    app: Option<String>,
    env: Option<String>,
}

/// GET /orgs/{org}/domains/check?hostname=my-app.example.com[&app=todo&env=production]
async fn check_domain(
    caller: IntegrationCaller,
    Path(org): Path<String>,
    Query(q): Query<CheckQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    // The org must be reachable, but the answer is about the whole platform.
    org_by_external(&state.db, &caller, &org).await?;
    let base = base_domain(&state);
    let req = Requested::from(q.hostname, q.subdomain)?;
    let hostname_mode = matches!(req, Requested::Hostname(_));

    let hostname = match req.resolve(&state)? {
        Ok(h) => h,
        Err(reason) => {
            // Still helpful: offer valid names derived from what was typed.
            let suggestions = suggest_for_invalid(&state, req.input(), hostname_mode).await?;
            return Ok(Json(ApiResponse::ok(serde_json::json!({
                "hostname": null, "input": req.input().trim().to_ascii_lowercase(),
                "subdomain": (!hostname_mode).then(|| req.input().trim().to_ascii_lowercase()),
                "available": false, "reason": "invalid", "message": reason, "suggestions": suggestions,
            }))));
        }
    };
    let subdomain = subdomain_of(&hostname, base.as_deref());

    let mut own = false;
    if let (Some(app), Some(env)) = (&q.app, &q.env) {
        let app = app_ref(&state.db, &caller, &org, app).await?;
        let project_id = env_project(&state.db, &app, env).await?;
        own = current_domain(&state, project_id, base.as_deref())
            .await?
            .is_some_and(|c| c.hostname == hostname);
    }
    if own {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "hostname": hostname, "subdomain": subdomain, "available": true, "current": true,
            "suggestions": [],
        }))));
    }
    if is_taken(&state, &hostname).await? {
        let (label, parent) = split_host(&hostname);
        let suggestions = suggest(&state, parent, label, 5).await?;
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "hostname": hostname, "subdomain": subdomain, "available": false,
            "reason": "taken", "message": format!("'{hostname}' is already in use"),
            "suggestions": suggestions,
        }))));
    }
    Ok(Json(ApiResponse::ok(serde_json::json!({
        "hostname": hostname, "subdomain": subdomain, "available": true, "suggestions": [],
    }))))
}

/// GET /orgs/{org}/apps/{app}/envs/{env}/domain
async fn get_domain(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let base = base_domain(&state);
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    let current = current_domain(&state, project_id, base.as_deref()).await?;
    Ok(Json(ApiResponse::ok(match current {
        Some(c) => serde_json::json!({
            "subdomain": subdomain_of(&c.hostname, base.as_deref()),
            "url": format!("https://{}", c.hostname),
            "hostname": c.hostname,
        }),
        None => serde_json::json!({ "hostname": null, "subdomain": null, "url": null }),
    })))
}

#[derive(Deserialize)]
struct ChangeDomain {
    /// Full domain, e.g. `my-app.example.com`.
    #[serde(default)]
    hostname: Option<String>,
    /// Legacy: first label under the apps base domain.
    #[serde(default)]
    subdomain: Option<String>,
}

/// PUT /orgs/{org}/apps/{app}/envs/{env}/domain  `{ "hostname": "my-app.example.com" }`
///
/// Moves the environment to the given hostname (or, legacy, to
/// `<subdomain>.<base>`). The new name is attached before the old one is
/// removed, so the app never goes unreachable. A name that is invalid or taken
/// answers 409 with free alternatives and changes nothing.
async fn change_domain(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
    Json(body): Json<ChangeDomain>,
) -> Result<Response, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let base = base_domain(&state);
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    let req = Requested::from(body.hostname, body.subdomain)?;
    let hostname_mode = matches!(req, Requested::Hostname(_));

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

    let hostname = match req.resolve(&state)? {
        Ok(h) => h,
        Err(message) => {
            let suggestions = suggest_for_invalid(&state, req.input(), hostname_mode).await?;
            return Ok(unavailable("invalid", message, suggestions));
        }
    };

    let Some(current) = current_domain(&state, project_id, base.as_deref()).await? else {
        return Err(ApiAppError(AppError::Conflict(
            "This environment has no platform address yet. Publish it first.".into(),
        )));
    };
    if current.hostname == hostname {
        mark_managed(&state, current.domain_id).await?;
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "hostname": hostname, "url": format!("https://{hostname}"),
            "previous_hostname": hostname, "changed": false,
        })))
        .into_response());
    }

    let taken_msg = format!("'{hostname}' is already in use");
    let (label, parent) = split_host(&hostname);
    if is_taken(&state, &hostname).await? {
        let suggestions = suggest(&state, parent, label, 5).await?;
        return Ok(unavailable("taken", taken_msg, suggestions));
    }

    let attached = match crate::resources::attach_domain(
        &state,
        current.service_id,
        &hostname,
        true,
        "letsencrypt",
        current.port,
    )
    .await
    {
        Ok(d) => d,
        // Lost a race with someone taking the same name.
        Err(ApiAppError(AppError::Conflict(_))) => {
            let suggestions = suggest(&state, parent, label, 5).await?;
            return Ok(unavailable("taken", taken_msg, suggestions));
        }
        Err(e) => return Err(e),
    };
    mark_managed(&state, attached.id).await?;
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
    fn normalizes_and_rejects_hostnames() {
        let base = Some("apps.test");
        assert_eq!(normalize_hostname(" Shop.Example.COM. ", base).unwrap(), "shop.example.com");
        assert_eq!(normalize_hostname("my-shop.apps.test", base).unwrap(), "my-shop.apps.test");
        assert_eq!(normalize_hostname("a.b.apps.test", base).unwrap(), "a.b.apps.test");
        assert!(normalize_hostname("", base).is_err());
        assert!(normalize_hostname("localhost", base).is_err());
        assert!(normalize_hostname("apps.test", base).is_err());
        assert!(normalize_hostname("www.apps.test", base).is_err());
        assert!(normalize_hostname("ab.apps.test", base).is_err());
        assert!(normalize_hostname("bad_name.example.com", base).is_err());
        assert!(normalize_hostname("-bad.example.com", base).is_err());
        assert!(normalize_hostname("a..example.com", base).is_err());
        assert!(normalize_hostname("*.example.com", base).is_err());
        assert!(normalize_hostname("10.0.0.1", base).is_err());
        assert!(normalize_hostname(&format!("{}.com", "a".repeat(64)), base).is_err());
        // Short or reserved labels are fine on a domain that isn't the base.
        assert!(normalize_hostname("www.example.com", base).is_ok());
        assert!(normalize_hostname("ab.example.com", None).is_ok());
    }

    #[test]
    fn subdomain_only_for_direct_children_of_the_base() {
        assert_eq!(subdomain_of("shop.apps.test", Some("apps.test")).as_deref(), Some("shop"));
        assert_eq!(subdomain_of("a.b.apps.test", Some("apps.test")), None);
        assert_eq!(subdomain_of("shop.example.com", Some("apps.test")), None);
        assert_eq!(subdomain_of("shop.apps.test", None), None);
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
