use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;

use axum::{
    extract::State,
    http::{HeaderValue, StatusCode},
    middleware as axum_middleware,
    routing::get,
    routing::post,
    Form,
    Router,
    Json,
};
use tower_http::cors::{AllowOrigin, CorsLayer, Any};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use shipyard_common::config::AppConfig;
use shipyard_common::types::ApiResponse;
use shipyard_docker::BollardDockerEngine;
use shipyard_docker::engine::DockerEngine;
use shipyard_mqtt::MqttPublisher;

use middleware::rate_limit::SharedRateLimiter;

mod auth;
mod cache;
mod compose;
mod email;
mod error;
mod routes;
mod setup;
mod orgs;
mod projects;
mod services;
mod resources;
mod containers;
mod deployments;
mod topology;
mod logs;
mod middleware;
mod templates;
mod webhooks;
mod settings;
mod shorthand;
mod dbclient;
mod static_site;

/// Short-lived OAuth state entries keyed by state UUID → (provider, org_id, created_at).
/// `org_id` is passed through the flow so the callback redirect lands on the right org settings page.
pub type OAuthStates = Arc<DashMap<String, (String, Option<String>, Instant)>>;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: sqlx::PgPool,
    pub docker: Arc<dyn DockerEngine>,
    pub mqtt: Arc<MqttPublisher>,
    pub oauth_states: OAuthStates,
    pub redis: Option<redis::aio::ConnectionManager>,
    /// Tight per-IP rate limiter for /auth/login and /auth/register (10 req/min).
    pub auth_limiter: SharedRateLimiter,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "shipyard=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Shipyard PaaS Platform");

    // Load configuration
    let mut config = AppConfig::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config, using defaults: {e}");
        AppConfig::default()
    });
    config.apply_docker_secrets();

    // ── Security sanity checks ────────────────────────────────────────────────
    // Refuse to boot with default secrets so a misconfigured prod deploy is
    // immediately visible rather than silently exploitable.
    if config.auth.jwt_secret == "change-me-in-production" {
        tracing::error!(
            "FATAL: auth.jwt_secret is set to the default value 'change-me-in-production'. \
             Set SHIPYARD__AUTH__JWT_SECRET (or /run/secrets/shipyard_jwt_secret) to a \
             strong random value before running in production."
        );
        std::process::exit(1);
    }
    let zero_key = "0".repeat(64);
    if config.auth.secret_key == zero_key {
        tracing::error!(
            "FATAL: auth.secret_key is set to the default all-zero value. \
             Set SHIPYARD__AUTH__SECRET_KEY (or /run/secrets/shipyard_secret_key) to a \
             64-character hex string before running in production."
        );
        std::process::exit(1);
    }

    // Warn (don't exit) about wide-open CORS so dev still works out of the box.
    if config.server.cors_allowed_origins.iter().any(|o| o == "*") {
        tracing::warn!(
            "CORS is configured to allow all origins ('*'). \
             Set server.cors_allowed_origins in production."
        );
    }

    if config.redis.url.is_none() {
        tracing::warn!(
            "Redis is not configured. Refresh token revocation (logout) will not work — \
             stolen tokens remain valid until they expire. \
             Set redis.url for production use."
        );
    }

    // Ensure the data directory exists (used for git clones and compose files)
    if let Err(e) = tokio::fs::create_dir_all(&config.data_dir).await {
        tracing::error!("Failed to create data_dir '{}': {e}", config.data_dir);
        std::process::exit(1);
    }
    tracing::info!("Data directory: {}", config.data_dir);

    // Connect to database
    let pool = shipyard_db::init_pool(&config.database.url, config.database.max_connections)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to connect to database: {e}");
            std::process::exit(1);
        });

    // Run migrations
    shipyard_db::run_migrations(&pool)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to run migrations: {e}");
            std::process::exit(1);
        });

    // Mark any deployment still in 'running' state after a restart as failed.
    // These are left over from a previous process that crashed or was killed
    // before it could persist the final status.
    match sqlx::query(
        "UPDATE deployments \
         SET status = 'failed'::deployment_status, finished_at = NOW() \
         WHERE status = 'running'::deployment_status AND finished_at IS NULL",
    )
    .execute(&pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => tracing::warn!(
            count = r.rows_affected(),
            "Marked stuck 'running' deployments as failed on startup"
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to clean up stuck deployments: {e}"),
    }

    // Connect to Docker
    let docker_engine: Arc<dyn DockerEngine> = {
        let engine = match config.docker.socket_path.as_deref() {
            Some(path) => BollardDockerEngine::with_socket(path),
            None => BollardDockerEngine::new(),
        }
        .unwrap_or_else(|e| {
            tracing::error!("Failed to connect to Docker: {e}");
            std::process::exit(1);
        });

        if let Err(e) = engine.ping().await {
            tracing::warn!("Docker daemon not reachable at startup: {e}");
        }

        Arc::new(engine)
    };

    // Connect to MQTT broker
    let (mqtt_publisher, _mqtt_sub, mqtt_eventloop) = shipyard_mqtt::create_mqtt_client(
        &config.mqtt.host,
        config.mqtt.port,
        &config.mqtt.client_id,
        config.mqtt.username.as_deref(),
        config.mqtt.password.as_deref(),
    )
    .unwrap_or_else(|e| {
        tracing::error!("Failed to create MQTT client: {e}");
        std::process::exit(1);
    });

    // Spawn the MQTT event loop driver
    shipyard_mqtt::spawn_event_loop(mqtt_eventloop);

    let mqtt_publisher = Arc::new(mqtt_publisher);

    // Spawn Docker Event Worker (with exponential backoff restart loop)
    {
        let worker_docker = Arc::clone(&docker_engine);
        let worker_db = pool.clone();
        let worker_mqtt = Arc::clone(&mqtt_publisher);
        let worker_label_prefix = config.docker.label_prefix.clone();

        tokio::spawn(async move {
            let worker = match shipyard_docker_worker::DockerEventWorker::new(
                worker_docker,
                worker_db,
                worker_mqtt,
                worker_label_prefix,
            ) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!("Failed to create Docker event worker: {e}");
                    return;
                }
            };

            // Reconcile DB state with Docker on startup
            if let Err(e) = worker.reconcile_on_startup().await {
                tracing::warn!("Startup reconciliation failed: {e}");
            }

            // Event loop with exponential backoff restart
            let mut backoff = Duration::from_secs(1);
            loop {
                if let Err(e) = worker.run().await {
                    tracing::error!("Docker event worker crashed: {e}. Restarting in {}s...", backoff.as_secs());
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                } else {
                    backoff = Duration::from_secs(1);
                }
            }
        });
    }

    // Spawn docker_events retention task — purge events older than 7 days, runs daily.
    {
        let cleanup_db = pool.clone();
        tokio::spawn(async move {
            loop {
                match sqlx::query(
                    "DELETE FROM docker_events WHERE received_at < NOW() - INTERVAL '7 days'",
                )
                .execute(&cleanup_db)
                .await
                {
                    Ok(r) => tracing::info!(
                        "docker_events cleanup: {} rows deleted",
                        r.rows_affected()
                    ),
                    Err(e) => tracing::warn!("docker_events cleanup failed: {e}"),
                }
                tokio::time::sleep(Duration::from_secs(86400)).await;
            }
        });
    }

    // Connect to Redis (optional — disabled when url is not configured)
    let redis_conn = if let Some(ref url) = config.redis.url {
        match redis::Client::open(url.as_str()) {
            Ok(client) => match redis::aio::ConnectionManager::new(client).await {
                Ok(mgr) => {
                    tracing::info!("Redis connected at {url}");
                    Some(mgr)
                }
                Err(e) => {
                    tracing::warn!("Redis connection manager failed, caching disabled: {e}");
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Invalid Redis URL, caching disabled: {e}");
                None
            }
        }
    } else {
        None
    };

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let tls_enabled = config.tls.enabled;
    let tls_cert = config.tls.cert_path.clone();

    // ── CORS ──────────────────────────────────────────────────────────────────
    let cors = {
        let origins = &config.server.cors_allowed_origins;
        if origins.iter().any(|o| o == "*") {
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        } else {
            let parsed: Vec<HeaderValue> = origins
                .iter()
                .filter_map(|o| o.parse::<HeaderValue>().ok())
                .collect();
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(parsed))
                .allow_methods(Any)
                .allow_headers(Any)
        }
    };

    let auth_limiter = middleware::rate_limit::make_auth_rate_limiter();

    let state = AppState {
        config: Arc::new(config),
        db: pool,
        docker: docker_engine,
        mqtt: mqtt_publisher,
        oauth_states: Arc::new(DashMap::new()),
        redis: redis_conn,
        auth_limiter,
    };

    // ── Deployment scheduler ──────────────────────────────────────────────────
    // Polls every 5 s for queued deployments when running count is below the
    // max_parallel_deployments setting. When max is 0 (default) the gate in
    // trigger_deploy is off and no queued rows are ever created, so the loop
    // is effectively a no-op in that case.
    {
        let sched_db     = state.db.clone();
        let sched_docker = Arc::clone(&state.docker);
        let sched_mqtt   = Arc::clone(&state.mqtt);
        let sched_config = Arc::clone(&state.config);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;

                // Read current max_parallel setting
                let max_parallel: i64 = match sqlx::query_as::<_, (String,)>(
                    "SELECT value::text FROM system_config WHERE key = 'max_parallel_deployments'",
                )
                .fetch_optional(&sched_db)
                .await
                {
                    Ok(Some((v,))) => v.trim_matches('"').parse::<i64>().unwrap_or(0),
                    _ => 0,
                };

                if max_parallel <= 0 { continue; }

                let running: i64 = match sqlx::query_as::<_, (i64,)>(
                    "SELECT COUNT(*) FROM deployments WHERE status = 'running'::deployment_status",
                )
                .fetch_one(&sched_db)
                .await
                {
                    Ok((n,)) => n,
                    Err(e) => { tracing::warn!("scheduler: running count query failed: {e}"); continue; }
                };

                let slots = (max_parallel - running).max(0) as usize;
                if slots == 0 { continue; }

                let queued = match sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, String, String)>(
                    "SELECT id, service_id, triggered_by, source_ref
                     FROM deployments
                     WHERE status = 'queued'::deployment_status
                     ORDER BY created_at ASC
                     LIMIT $1",
                )
                .bind(slots as i64)
                .fetch_all(&sched_db)
                .await
                {
                    Ok(rows) => rows,
                    Err(e) => { tracing::warn!("scheduler: queued fetch failed: {e}"); continue; }
                };

                for (dep_id, svc_id, triggered_by, source_ref) in queued {
                    let engine = shipyard_engine::DeploymentEngine::new(
                        Arc::clone(&sched_docker),
                        sched_db.clone(),
                        Arc::clone(&sched_mqtt),
                        sched_config.docker.label_prefix.clone(),
                        sched_config.traefik.network.clone(),
                        sched_config.auth.secret_key.clone(),
                        sched_config.docker.port_proxy,
                        sched_config.data_dir.clone(),
                        sched_config.static_server.retention_versions,
                    );
                    tokio::spawn(async move {
                        if let Err(e) = engine.deploy_queued(dep_id, svc_id, &triggered_by, &source_ref).await {
                            tracing::error!(deployment_id = %dep_id, "scheduled deployment failed: {e}");
                        }
                    });
                }
            }
        });
    }

    // Build the API sub-router with the initialization gate middleware.
    let api = routes::api_router()
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            setup::require_initialized_middleware,
        ));

    // Build the Open API sub-router (separate state, separate auth model).
    let openapi_state = shipyard_openapi::OpenApiState {
        db:     state.db.clone(),
        config: Arc::clone(&state.config),
        docker: Arc::clone(&state.docker),
        mqtt:   Arc::clone(&state.mqtt),
    };
    let openapi = shipyard_openapi::routes().with_state(openapi_state);

    // Per-IP rate limiter shared via Extension
    let rate_limiter = middleware::rate_limit::make_rate_limiter();

    // Build the full application router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(metrics))
        // Internal MQTT auth callback — called by rmqtt-auth-http, not by clients.
        // Lives outside /api so it bypasses the init gate and rate limiter.
        .route("/internal/mqtt/auth", post(mqtt_auth))
        .nest("/api", api)
        // Public Open API — mounted separately so it can use its own state/auth.
        .nest_service("/openapi/v1", openapi)
        .layer(axum_middleware::from_fn(middleware::rate_limit::rate_limit))
        .layer(axum::Extension(rate_limiter))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    if tls_enabled {
        tracing::warn!(
            "TLS is configured (cert={tls_cert}) but direct TLS is not supported yet. \
             Use a reverse proxy (Traefik, Nginx) for TLS termination."
        );
    }

    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind listener");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .expect("Server error");
}

/// Health check endpoint — returns JSON with component statuses.
async fn health_check(State(state): State<AppState>) -> Json<ApiResponse<serde_json::Value>> {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();

    let docker_ok = state.docker.ping().await.is_ok();

    let status = if db_ok && docker_ok { "ok" } else { "degraded" };

    Json(ApiResponse::ok(serde_json::json!({
        "status": status,
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "database": if db_ok { "ok" } else { "error" },
            "docker":   if docker_ok { "ok" } else { "error" },
        }
    })))
}

/// Prometheus text format metrics endpoint.
async fn metrics(State(state): State<AppState>) -> String {
    let svc_count: i64 = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM services")
        .fetch_one(&state.db)
        .await
        .map(|(n,)| n)
        .unwrap_or(0);

    let running_count: i64 = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM services WHERE status = 'running'",
    )
    .fetch_one(&state.db)
    .await
    .map(|(n,)| n)
    .unwrap_or(0);

    let deploy_total: i64 = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM deployments")
        .fetch_one(&state.db)
        .await
        .map(|(n,)| n)
        .unwrap_or(0);

    let deploy_success: i64 = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM deployments WHERE status::text = 'success'",
    )
    .fetch_one(&state.db)
    .await
    .map(|(n,)| n)
    .unwrap_or(0);

    let deploy_failed: i64 = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM deployments WHERE status::text = 'failed'",
    )
    .fetch_one(&state.db)
    .await
    .map(|(n,)| n)
    .unwrap_or(0);

    let deploy_running: i64 = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM deployments WHERE status::text = 'running'",
    )
    .fetch_one(&state.db)
    .await
    .map(|(n,)| n)
    .unwrap_or(0);

    let container_running: i64 = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM containers WHERE status::text = 'running'",
    )
    .fetch_one(&state.db)
    .await
    .map(|(n,)| n)
    .unwrap_or(0);

    format!(
        "# HELP shipyard_services_total Total number of services\n\
         # TYPE shipyard_services_total gauge\n\
         shipyard_services_total {svc_count}\n\
         # HELP shipyard_services_running Number of running services\n\
         # TYPE shipyard_services_running gauge\n\
         shipyard_services_running {running_count}\n\
         # HELP shipyard_deployments_total Total deployments by status\n\
         # TYPE shipyard_deployments_total gauge\n\
         shipyard_deployments_total{{status=\"success\"}} {deploy_success}\n\
         shipyard_deployments_total{{status=\"failed\"}} {deploy_failed}\n\
         shipyard_deployments_total{{status=\"running\"}} {deploy_running}\n\
         shipyard_deployments_total{{status=\"all\"}} {deploy_total}\n\
         # HELP shipyard_containers_running Number of running containers\n\
         # TYPE shipyard_containers_running gauge\n\
         shipyard_containers_running {container_running}\n"
    )
}

// ─── MQTT internal auth callback ─────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct MqttAuthForm {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
}

/// Called by rmqtt-auth-http for every MQTT CONNECT attempt.
///
/// Two credential types are accepted:
/// - Backend service: username = configured `SHIPYARD__MQTT__USERNAME`,
///   password = `SHIPYARD__MQTT__PASSWORD`.
/// - Browser clients: username = any `shipyard-web-*` client ID,
///   password = a valid (non-expired) JWT access token.
async fn mqtt_auth(
    State(state): State<AppState>,
    Form(body): Form<MqttAuthForm>,
) -> StatusCode {
    let cfg_user = state.config.mqtt.username.as_deref().unwrap_or("shipyard-api");
    let cfg_pass = state.config.mqtt.password.as_deref().unwrap_or("");

    // Backend MQTT client
    if body.username == cfg_user {
        return if !cfg_pass.is_empty() && body.password == cfg_pass {
            StatusCode::OK
        } else {
            tracing::warn!("MQTT auth: invalid password for backend client '{}'", body.username);
            StatusCode::UNAUTHORIZED
        };
    }

    // Browser / frontend clients use their JWT as the password
    if body.username.starts_with("shipyard-web") {
        return match auth::decode_token(&body.password, &state.config.auth.jwt_secret) {
            Ok(_) => StatusCode::OK,
            Err(_) => {
                tracing::warn!("MQTT auth: invalid JWT for frontend client '{}'", body.username);
                StatusCode::UNAUTHORIZED
            }
        };
    }

    tracing::warn!("MQTT auth: unknown username '{}'", body.username);
    StatusCode::UNAUTHORIZED
}
