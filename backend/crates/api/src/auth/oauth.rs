use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    response::Redirect,
    routing::get,
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;

// ─── Provider definitions ─────────────────────────────────────────────────────

struct ProviderDef {
    auth_url: &'static str,
    token_url: &'static str,
    scope: &'static str,
    settings_key: &'static str,
}

fn provider_def(provider: &str) -> Option<ProviderDef> {
    match provider {
        "github" => Some(ProviderDef {
            auth_url:     "https://github.com/login/oauth/authorize",
            token_url:    "https://github.com/login/oauth/access_token",
            scope:        "repo",
            settings_key: "git_github_token",
        }),
        "gitlab" => Some(ProviderDef {
            auth_url:     "https://gitlab.com/oauth/authorize",
            token_url:    "https://gitlab.com/oauth/token",
            scope:        "read_repository",
            settings_key: "git_gitlab_token",
        }),
        "bitbucket" => Some(ProviderDef {
            auth_url:     "https://bitbucket.org/site/oauth2/authorize",
            token_url:    "https://bitbucket.org/site/oauth2/access_token",
            scope:        "repository",
            settings_key: "git_bitbucket_token",
        }),
        _ => None,
    }
}

fn client_credentials(state: &AppState, provider: &str) -> Option<(String, String)> {
    let cfg = &state.config.git;
    match provider {
        "github"    => Some((cfg.github_client_id.clone(),    cfg.github_client_secret.clone())),
        "gitlab"    => Some((cfg.gitlab_client_id.clone(),    cfg.gitlab_client_secret.clone())),
        "bitbucket" => Some((cfg.bitbucket_client_id.clone(), cfg.bitbucket_client_secret.clone())),
        _ => None,
    }
}

// ─── Routes ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/oauth/:provider",          get(initiate_oauth))
        .route("/auth/oauth/:provider/callback", get(oauth_callback))
}

// ─── Initiate ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InitiateQuery {
    /// The org ID to redirect back to after OAuth completes (e.g. the orgId from the settings URL).
    pub org_id: Option<String>,
}

/// GET /auth/oauth/:provider?org_id=...
/// Builds the authorization URL and redirects the browser to the provider.
async fn initiate_oauth(
    Path(provider): Path<String>,
    Query(query): Query<InitiateQuery>,
    State(state): State<AppState>,
) -> Redirect {
    let frontend_url = state.config.git.frontend_url.clone();
    let settings_path = settings_redirect_path(query.org_id.as_deref());
    let error_redirect = format!("{frontend_url}{settings_path}?git_error=unknown_provider");

    let Some(def) = provider_def(&provider) else {
        return Redirect::to(&error_redirect);
    };
    let Some((client_id, _)) = client_credentials(&state, &provider) else {
        return Redirect::to(&format!("{frontend_url}{settings_path}?git_error=not_configured"));
    };
    if client_id.is_empty() {
        return Redirect::to(&format!("{frontend_url}{settings_path}?git_error=not_configured"));
    }

    // Evict stale states (older than 10 min) to avoid unbounded growth
    let cutoff = Instant::now() - std::time::Duration::from_secs(600);
    state.oauth_states.retain(|_, (_, _, created)| *created > cutoff);

    let oauth_state = Uuid::now_v7().to_string();
    state.oauth_states.insert(oauth_state.clone(), (provider.clone(), query.org_id.clone(), Instant::now()));

    let api_base = &state.config.git.api_base_url;
    let redirect_uri = format!("{api_base}/api/auth/oauth/{provider}/callback");

    let url = format!(
        "{}?client_id={}&redirect_uri={}&scope={}&state={}&response_type=code",
        def.auth_url,
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(def.scope),
        urlencoding::encode(&oauth_state),
    );

    Redirect::to(&url)
}

/// Build the settings page path. If org_id is known, use the org-scoped route;
/// otherwise fall back to the root /settings path (for backwards compatibility).
fn settings_redirect_path(org_id: Option<&str>) -> String {
    match org_id {
        Some(id) if !id.is_empty() => format!("/orgs/{id}/settings"),
        _ => "/settings".to_string(),
    }
}

// ─── Callback ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// GET /auth/oauth/:provider/callback
/// Receives the code from the provider, exchanges it for a token, saves it to settings.
async fn oauth_callback(
    Path(provider): Path<String>,
    Query(params): Query<CallbackQuery>,
    State(state): State<AppState>,
) -> Redirect {
    let frontend_url = state.config.git.frontend_url.clone();

    // Provider denied / user cancelled — we don't have the state yet so fall back to root /settings
    if let Some(err) = params.error {
        tracing::warn!(%provider, %err, "OAuth provider returned error");
        return Redirect::to(&format!("{frontend_url}/settings?git_error={err}"));
    }

    // Validate state parameter
    let Some(oauth_state) = params.state else {
        return Redirect::to(&format!("{frontend_url}/settings?git_error=missing_state"));
    };
    let state_entry = state.oauth_states.remove(&oauth_state);
    let Some((_, (stored_provider, org_id, created_at))) = state_entry else {
        return Redirect::to(&format!("{frontend_url}/settings?git_error=invalid_state"));
    };

    // Build the return path now that we have org_id from stored state
    let settings_path = settings_redirect_path(org_id.as_deref());

    if stored_provider != provider {
        return Redirect::to(&format!("{frontend_url}{settings_path}?git_error=state_mismatch"));
    }
    if created_at.elapsed().as_secs() > 600 {
        return Redirect::to(&format!("{frontend_url}{settings_path}?git_error=state_expired"));
    }

    let Some(code) = params.code else {
        return Redirect::to(&format!("{frontend_url}{settings_path}?git_error=missing_code"));
    };

    // Exchange code for access token
    let token = match exchange_code(&state, &provider, &code).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(%provider, "OAuth token exchange failed: {e}");
            return Redirect::to(&format!("{frontend_url}{settings_path}?git_error=token_exchange_failed"));
        }
    };

    let org_uuid = match org_id.as_deref().and_then(|id| Uuid::parse_str(id).ok()) {
        Some(uuid) => uuid,
        None => {
            return Redirect::to(&format!("{frontend_url}/settings?git_error=missing_org_id"));
        }
    };

    // Persist token to git_providers table
    let username = resolve_username(&provider, &token).await;
    let name = format!("{} ({})", provider.to_uppercase(), username.as_deref().unwrap_or("OAuth"));

    let provider_id = Uuid::now_v7();
    let result = sqlx::query(
        "INSERT INTO git_providers (id, org_id, name, provider_type, auth_type, token, username, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'oauth', $5, $6, NOW(), NOW())",
    )
    .bind(provider_id)
    .bind(org_uuid)
    .bind(&name)
    .bind(&provider)
    .bind(&token)
    .bind(username)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            tracing::info!(%provider, "OAuth token saved to git_providers");
            Redirect::to(&format!("{frontend_url}{settings_path}?git_connected={provider}"))
        }
        Err(e) => {
            tracing::error!(%provider, "Failed to save OAuth token: {e}");
            Redirect::to(&format!("{frontend_url}{settings_path}?git_error=save_failed"))
        }
    }
}

async fn resolve_username(provider: &str, token: &str) -> Option<String> {
    let client = reqwest::Client::new();
    match provider {
        "github" => {
            #[derive(serde::Deserialize)]
            struct GitHubUser { login: String }
            let resp = client.get("https://api.github.com/user")
                .header("User-Agent", "shipyard-api")
                .header("Authorization", format!("token {}", token))
                .send()
                .await
                .ok()?;
            if resp.status().is_success() {
                let user: GitHubUser = resp.json().await.ok()?;
                Some(format!("@{}", user.login))
            } else {
                None
            }
        }
        "gitlab" => {
            #[derive(serde::Deserialize)]
            struct GitLabUser { username: String }
            let resp = client.get("https://gitlab.com/api/v4/user")
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .ok()?;
            if resp.status().is_success() {
                let user: GitLabUser = resp.json().await.ok()?;
                Some(format!("@{}", user.username))
            } else {
                None
            }
        }
        "bitbucket" => {
            #[derive(serde::Deserialize)]
            struct BitbucketUser { username: String }
            let resp = client.get("https://api.bitbucket.org/2.0/user")
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .ok()?;
            if resp.status().is_success() {
                let user: BitbucketUser = resp.json().await.ok()?;
                Some(format!("@{}", user.username))
            } else {
                None
            }
        }
        _ => None,
    }
}

// ─── Token exchange ───────────────────────────────────────────────────────────

async fn exchange_code(state: &AppState, provider: &str, code: &str) -> anyhow::Result<String> {
    let def = provider_def(provider).ok_or_else(|| anyhow::anyhow!("unknown provider"))?;
    let (client_id, client_secret) = client_credentials(state, provider)
        .ok_or_else(|| anyhow::anyhow!("no credentials"))?;

    let api_base = &state.config.git.api_base_url;
    let redirect_uri = format!("{api_base}/api/auth/oauth/{provider}/callback");

    let http = reqwest::Client::new();

    match provider {
        "github" => {
            let resp = http
                .post(def.token_url)
                .header("Accept", "application/json")
                .form(&[
                    ("client_id",     client_id.as_str()),
                    ("client_secret", client_secret.as_str()),
                    ("code",          code),
                    ("redirect_uri",  redirect_uri.as_str()),
                ])
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            resp["access_token"]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("no access_token in GitHub response: {resp}"))
        }

        "gitlab" => {
            let resp = http
                .post(def.token_url)
                .form(&[
                    ("client_id",     client_id.as_str()),
                    ("client_secret", client_secret.as_str()),
                    ("code",          code),
                    ("redirect_uri",  redirect_uri.as_str()),
                    ("grant_type",    "authorization_code"),
                ])
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            resp["access_token"]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("no access_token in GitLab response: {resp}"))
        }

        "bitbucket" => {
            let resp = http
                .post(def.token_url)
                .basic_auth(&client_id, Some(&client_secret))
                .form(&[
                    ("grant_type",   "authorization_code"),
                    ("code",         code),
                    ("redirect_uri", redirect_uri.as_str()),
                ])
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            resp["access_token"]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("no access_token in Bitbucket response: {resp}"))
        }

        _ => Err(anyhow::anyhow!("unsupported provider: {provider}")),
    }
}
