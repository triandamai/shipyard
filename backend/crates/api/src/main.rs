#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
use tokio::sync::Notify;

mod admin;
mod alerts;
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
pub(crate) mod resources;
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
mod git_providers;
mod billing;
mod nodes;
mod plans;
mod provisioning;
mod compute;
mod edge_functions;
mod artifactory;
mod artifact_source;

use shipyard_registry::{
    router::registry_router,
    storage::{local::LocalStorage, StorageBackend},
    RegistryState,
};
use axum::extract::FromRef;

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
    /// Shared HTTP client — reuses the connection pool across all outbound requests.
    pub http_client: reqwest::Client,
    /// Tight per-IP rate limiter for /auth/login and /auth/register (10 req/min).
    pub auth_limiter: SharedRateLimiter,
    /// Notified whenever a deployment completes so the Swarm sync loop wakes immediately.
    pub swarm_sync_trigger: Arc<Notify>,
    /// Artifact registry storage backend (local or S3).
    pub registry_storage: Arc<dyn StorageBackend>,
}

/// Allow registry route handlers typed `State<RegistryState>` to be used inside
/// the main `Router<AppState>` — axum calls this to extract the sub-state.
impl FromRef<AppState> for RegistryState {
    fn from_ref(s: &AppState) -> Self {
        RegistryState {
            db:         s.db.clone(),
            storage:    Arc::clone(&s.registry_storage),
            hostname:   s.config.registry.hostname.clone(),
            jwt_secret: s.config.auth.jwt_secret.clone(),
        }
    }
}

fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(512 * 1024) // 512 KB — async tasks don't deep-recurse
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(async_main());
}

async fn async_main() {
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

    // Write Traefik dynamic config (shipyard.yml) on every startup so Traefik
    // always has the global routes and middleware definitions, even after a volume wipe.
    if let Some(ref dir) = config.traefik.dynamic_config_dir {
        // Derive the API domain from config or fall back to "api-<domain>".
        let raw_domain = config.app_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or(&config.app_url)
            .split(':')
            .next()
            .unwrap_or(&config.app_url);
        let default_api_domain = format!("api-{raw_domain}");
        let api_domain = config.api_domain.as_deref().unwrap_or(&default_api_domain);

        write_shipyard_traefik_config(
            &config.app_url,
            api_domain,
            &config.registry.hostname,
            dir,
            &config.traefik.entrypoint_http,
            &config.traefik.entrypoint_https,
            &config.traefik.cert_resolver,
        )
        .await;
    }

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

    // Build the Docker Event Worker once — both the event loop and Swarm sync loop
    // share this instance so they share the same bollard connection pool.
    let event_worker = match shipyard_docker_worker::DockerEventWorker::new(
        Arc::clone(&docker_engine),
        pool.clone(),
        Arc::clone(&mqtt_publisher),
        config.docker.label_prefix.clone(),
    ) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to create Docker event worker: {e}");
            std::process::exit(1);
        }
    };

    // Spawn Docker Event Worker (with exponential backoff restart loop)
    {
        let worker = event_worker.clone();
        tokio::spawn(async move {
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


    // Spawn edge function invocation log retention — purge logs older than 7 days, runs daily.
    {
        let cleanup_db = pool.clone();
        tokio::spawn(async move {
            loop {
                match sqlx::query(
                    "DELETE FROM edge_function_invocation_logs WHERE logged_at < NOW() - INTERVAL '7 days'",
                )
                .execute(&cleanup_db)
                .await
                {
                    Ok(r) => tracing::info!(
                        "edge_fn_logs cleanup: {} rows deleted",
                        r.rows_affected()
                    ),
                    Err(e) => tracing::warn!("edge_fn_logs cleanup failed: {e}"),
                }
                tokio::time::sleep(Duration::from_secs(86400)).await;
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

    // ── Registry storage ──────────────────────────────────────────────────────
    let registry_path = format!("{}/registry", config.data_dir);
    if let Err(e) = tokio::fs::create_dir_all(&registry_path).await {
        tracing::error!("Failed to create registry data dir '{registry_path}': {e}");
        std::process::exit(1);
    }

    let registry_storage: Arc<dyn StorageBackend> = {
        use shipyard_registry::storage::s3::S3Storage;
        if config.registry.storage == "s3" {
            let endpoint   = config.registry.s3_endpoint.as_deref().unwrap_or_default();
            let bucket     = config.registry.s3_bucket.as_deref().unwrap_or("shipyard-registry");
            let access_key = config.registry.s3_access_key.as_deref().unwrap_or_default();
            let secret_key = config.registry.s3_secret_key.as_deref().unwrap_or_default();
            let region     = config.registry.s3_region.as_deref().unwrap_or("us-east-1");
            let path_style = config.registry.s3_path_style;
            match S3Storage::new(endpoint, bucket, access_key, secret_key, region, path_style).await {
                Ok(s3) => {
                    tracing::info!("Registry storage: S3/MinIO at {endpoint} bucket={bucket}");
                    Arc::new(s3) as Arc<dyn StorageBackend>
                }
                Err(e) => {
                    tracing::error!("Failed to init S3 registry storage: {e}. Falling back to local disk.");
                    Arc::new(LocalStorage::new(&registry_path)) as Arc<dyn StorageBackend>
                }
            }
        } else {
            tracing::info!("Registry storage: local disk at {registry_path}");
            Arc::new(LocalStorage::new(&registry_path)) as Arc<dyn StorageBackend>
        }
    };

    let auth_limiter = middleware::rate_limit::make_auth_rate_limiter();

    let swarm_sync_trigger = Arc::new(Notify::new());

    let state = AppState {
        config: Arc::new(config),
        db: pool,
        docker: docker_engine,
        mqtt: mqtt_publisher,
        oauth_states: Arc::new(DashMap::new()),
        redis: redis_conn,
        http_client: reqwest::Client::new(),
        auth_limiter,
        swarm_sync_trigger: Arc::clone(&swarm_sync_trigger),
        registry_storage,
    };

    // ── Deployment scheduler ──────────────────────────────────────────────────
    // Polls every 5 s for queued deployments when running count is below the
    // max_parallel_deployments setting. When max is 0 (default) the gate in
    // trigger_deploy is off and no queued rows are ever created, so the loop
    // is effectively a no-op in that case.
    {
        let sched_db      = state.db.clone();
        let sched_docker  = Arc::clone(&state.docker);
        let sched_mqtt    = Arc::clone(&state.mqtt);
        let sched_config  = Arc::clone(&state.config);
        let sched_storage = Arc::clone(&state.registry_storage);

        tokio::spawn(async move {
            loop {
                // Read current max_parallel setting first — when 0 (disabled) we
                // skip all scheduling work and back off to 60 s to avoid pointless
                // DB churn (was: 1 query every 5 s even when the feature is off).
                let max_parallel: i64 = match sqlx::query_as::<_, (String,)>(
                    "SELECT value::text FROM system_config WHERE key = 'max_parallel_deployments'",
                )
                .fetch_optional(&sched_db)
                .await
                {
                    Ok(Some((v,))) => v.trim_matches('"').parse::<i64>().unwrap_or(0),
                    _ => 0,
                };

                if max_parallel > 0 {
                    let running: i64 = match sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM deployments WHERE status = 'running'::deployment_status",
                    )
                    .fetch_one(&sched_db)
                    .await
                    {
                        Ok((n,)) => n,
                        Err(e) => { tracing::warn!("scheduler: running count query failed: {e}"); 0 }
                    };

                    let slots = (max_parallel - running).max(0) as usize;
                    if slots > 0 {
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
                            Err(e) => { tracing::warn!("scheduler: queued fetch failed: {e}"); vec![] }
                        };

                        for (dep_id, svc_id, triggered_by, source_ref) in queued {
                            if let Err(e) = sqlx::query(
                                "UPDATE deployments SET status = 'running'::deployment_status WHERE id = $1"
                            )
                            .bind(dep_id)
                            .execute(&sched_db)
                            .await
                            {
                                tracing::error!("scheduler: failed to mark deployment {dep_id} as running: {e}");
                                continue;
                            }

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
                            ).with_registry(shipyard_registry::push::ArtifactPusher::new(
                                sched_db.clone(),
                                Arc::clone(&sched_storage),
                            ))
                            .with_registry_hostname(sched_config.registry.hostname.clone());
                            tokio::spawn(async move {
                                if let Err(e) = engine.deploy_queued(dep_id, svc_id, &triggered_by, &source_ref).await {
                                    tracing::error!(deployment_id = %dep_id, "scheduled deployment failed: {e}");
                                }
                            });
                        }
                    }
                }

                // When the feature is disabled back off to 60 s; 5 s when active.
                tokio::time::sleep(if max_parallel <= 0 {
                    Duration::from_secs(60)
                } else {
                    Duration::from_secs(5)
                }).await;
            }
        });
    }

    // Spawn periodic Swarm task sync — keeps container records fresh for all nodes.
    // The event worker only sees events from the local Docker socket, so containers on
    // worker nodes never get events. This loop fills the gap:
    //   • runs once immediately on startup
    //   • wakes immediately when `swarm_sync_trigger` is notified (e.g. after a deploy)
    //   • falls back to a 10 s poll so orphan detection still happens on its own
    {
        let sync_worker = event_worker.clone(); // shared bollard pool — no extra connection
        let sync_notify = Arc::clone(&state.swarm_sync_trigger);

        tokio::spawn(async move {
            loop {
                if let Err(e) = sync_worker.sync_swarm_tasks().await {
                    tracing::warn!("sync_swarm_tasks failed: {e}");
                }
                // Wait for a deploy signal OR 10 s, whichever comes first.
                tokio::select! {
                    _ = sync_notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {}
                }
            }
        });
    }

    // Spawn provisioning worker — monitors compute_nodes for timed-out or advancing states.
    {
        let provision_db = state.db.clone();
        let provision_client = state.http_client.clone();
        let hetzner_key = state.config.hetzner_api_key.clone();
        let do_key = state.config.do_api_key.clone();
        let provision_mqtt = Arc::clone(&state.mqtt);
        let label_prefix = state.config.docker.label_prefix.clone();
        let worker = Arc::new(provisioning::ProvisioningWorker::new(
            provision_db,
            provision_client,
            hetzner_key,
            do_key,
            provision_mqtt,
            label_prefix,
            state.config.default_cloud_provider.clone(),
            state.config.hetzner_server_type.clone(),
            state.config.hetzner_region.clone(),
            state.config.do_size.clone(),
            state.config.do_region.clone(),
        ));

        let worker_clone = Arc::clone(&worker);
        tokio::spawn(async move { worker_clone.run().await });

        // Reconciliation loop: every 5 min, detect paid orgs with no live node.
        tokio::spawn(async move { worker.run_reconciliation().await });
    }

    // Edge runtime worker: ensures Deno containers exist per org with active functions.
    if state.config.edge_functions.enabled {
        let edge_state = Arc::new(state.clone());
        tokio::spawn(async move {
            edge_functions::runtime_worker::run(edge_state).await;
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

    // Spawn a background task to periodically clean up static site access logs older than 7 days
    {
        let cleaner_config = Arc::clone(&state.config);
        tokio::spawn(async move {
            loop {
                let logs_dir = format!("{}/static/logs", cleaner_config.data_dir);
                tracing::info!("log_cleaner: scanning {logs_dir} for logs older than 7 days...");

                if let Ok(mut entries) = tokio::fs::read_dir(&logs_dir).await {
                    while let Ok(Some(log_entry)) = entries.next_entry().await {
                        if log_entry.file_type().await.map(|t| t.is_file()).unwrap_or(false) {
                            let name = log_entry.file_name().to_string_lossy().into_owned();
                            if name.starts_with("access-") && name.ends_with(".log") {
                                let name_without_ext = name.trim_end_matches(".log");
                                if name_without_ext.len() >= 10 {
                                    let date_str = &name_without_ext[name_without_ext.len() - 10..];
                                    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                                        let today = chrono::Utc::now().date_naive();
                                        let days_old = (today - date).num_days();
                                        if days_old > 7 {
                                            if let Err(e) = tokio::fs::remove_file(log_entry.path()).await {
                                                tracing::warn!("log_cleaner: failed to delete old log file {:?}: {}", log_entry.path(), e);
                                            } else {
                                                tracing::info!("log_cleaner: deleted old log file {:?}", log_entry.path());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Run once every 24 hours
                tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
            }
        });
    }

    // Per-IP rate limiter shared via Extension
    let rate_limiter = middleware::rate_limit::make_rate_limiter();

    // Spawn a background task to periodically clean up expired rate limiter entries to prevent memory leak
    {
        let main_limiter = Arc::clone(&rate_limiter);
        let auth_limiter = Arc::clone(&state.auth_limiter);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                main_limiter.retain_recent();
                auth_limiter.retain_recent();
                main_limiter.shrink_to_fit();
                auth_limiter.shrink_to_fit();
            }
        });
    }

    // Build the full application router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(metrics))
        // Internal MQTT auth callback — called by rmqtt-auth-http, not by clients.
        // Lives outside /api so it bypasses the init gate and rate limiter.
        .route("/internal/mqtt/auth", post(mqtt_auth))
        .nest("/api", api)
        // Edge function invocations — public, no auth, routed to per-org runtime.
        // Must be outside /api so it's not behind the init gate.
        .nest("/fn", edge_functions::invoke_routes())
        // OCI artifact registry — nested at /registry so Traefik can route
        // registry-domain.com/* → backend:3001/registry/* with addPrefix middleware.
        // RegistryState is extracted from AppState via FromRef.
        .nest("/registry", registry_router::<AppState>())
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

// ─── Traefik startup config ───────────────────────────────────────────────────

/// Writes `shipyard.yml` to the Traefik dynamic config directory on every startup.
///
/// install.sh also writes this file on first install, but if the volume is wiped
/// or the container is recreated, the file disappears and all Traefik routes break.
/// Writing it here makes startup self-healing — Traefik hot-reloads it immediately.
async fn write_shipyard_traefik_config(
    app_url: &str,
    api_domain: &str,
    registry_hostname: &str,
    dynamic_config_dir: &str,
    entrypoint_http: &str,
    entrypoint_https: &str,
    cert_resolver: &str,
) {
    // Extract bare hostname from app_url ("https://ship.example.com:8080/path" → "ship.example.com")
    let domain = app_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(app_url)
        .split(':')
        .next()
        .unwrap_or(app_url);

    let is_https = app_url.starts_with("https://");
    let static_entrypoints = if is_https { entrypoint_https } else { entrypoint_http };
    let static_tls = if is_https { "      tls: {}" } else { "" };

    let content = format!(
        r#"http:
  routers:
    shipyard-frontend:
      rule: "Host(`{domain}`)"
      entryPoints: [{entrypoint_https}]
      service: shipyard-frontend
      tls:
        certResolver: {cert_resolver}

    shipyard-api:
      rule: "Host(`{api_domain}`)"
      entryPoints: [{entrypoint_https}]
      service: shipyard-backend
      tls:
        certResolver: {cert_resolver}

    shipyard-registry:
      rule: "Host(`{registry_hostname}`)"
      entryPoints: [{entrypoint_https}]
      service: shipyard-backend
      middlewares:
        - shipyard-registry-prefix
      tls:
        certResolver: {cert_resolver}

    shipyard-fn-invoke:
      rule: "Host(`{domain}`) && PathPrefix(`/fn/`)"
      entryPoints: [{entrypoint_https}]
      service: shipyard-backend
      tls:
        certResolver: {cert_resolver}

    shipyard-mqtt:
      rule: "Host(`{domain}`) && PathPrefix(`/mqtt`)"
      entryPoints: [{entrypoint_https}]
      service: shipyard-mqtt
      middlewares:
        - shipyard-mqtt-strip
      tls:
        certResolver: {cert_resolver}

    static-sites:
      rule: "HostRegexp(`.+`)"
      priority: 1
      entryPoints: [{static_entrypoints}]
      service: nginx-static
{static_tls}

  services:
    shipyard-frontend:
      loadBalancer:
        servers:
          - url: "http://shipyard-frontend:3000"

    shipyard-backend:
      loadBalancer:
        servers:
          - url: "http://shipyard-backend:3001"

    shipyard-mqtt:
      loadBalancer:
        servers:
          - url: "http://shipyard-mqtt:8083"

    nginx-static:
      loadBalancer:
        servers:
          - url: "http://shipyard-nginx-static:80"

  middlewares:
    shipyard-registry-prefix:
      addPrefix:
        prefix: "/registry"

    shipyard-mqtt-strip:
      stripPrefix:
        prefixes:
          - "/mqtt"

    shipyard-error-pages:
      errors:
        status:
          - "404"
        service: nginx-static
        query: "/_errors/{{status}}.html"
"#
    );

    if let Err(e) = tokio::fs::create_dir_all(dynamic_config_dir).await {
        tracing::warn!("Could not create traefik dynamic config dir '{dynamic_config_dir}': {e}");
        return;
    }
    let dest = format!("{dynamic_config_dir}/shipyard.yml");
    // Only write if the file is absent — install.sh writes the authoritative version
    // with the real domain. Overwriting it would replace the domain with whatever
    // app_url is set to (which may be localhost in container-internal configs).
    if tokio::fs::try_exists(&dest).await.unwrap_or(false) {
        tracing::info!("Traefik dynamic config already exists, skipping write: {dest}");
        return;
    }
    match tokio::fs::write(&dest, content).await {
        Ok(_) => tracing::info!("Traefik dynamic config written (bootstrap): {dest}"),
        Err(e) => tracing::warn!("Could not write traefik config '{dest}': {e}"),
    }
}
