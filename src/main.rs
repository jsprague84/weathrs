mod air_quality;
mod api_budget;
mod backfill;
mod cache;
mod config;
mod db;
mod devices;
mod error;
mod extractors;
mod forecast;
mod history;
mod middleware;
mod notifications;
mod openapi;
mod routes;
mod scheduler;
mod weather;

use axum::{
    error_handling::HandleErrorLayer,
    http::{self, Method, StatusCode},
    BoxError,
};
use reqwest::Client;
use std::net::SocketAddr;
use std::{sync::Arc, time::Duration};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::air_quality::AirQualityService;
use crate::cache::{create_geo_cache, start_cache_cleanup_task};
use crate::config::AppConfig;
use crate::devices::DevicesService;
use crate::forecast::ForecastService;
use crate::history::HistoryService;
use crate::scheduler::{JobConfig, SchedulerService};
use crate::weather::WeatherService;

const HTTP_POOL_IDLE_TIMEOUT_SECS: u64 = 90;

#[derive(Clone)]
pub struct AppState {
    pub http_client: Client,
    pub db_pool: sqlx::SqlitePool,
    pub weather_service: Arc<WeatherService>,
    pub forecast_service: Arc<ForecastService>,
    pub history_service: Arc<HistoryService>,
    pub scheduler_service: Arc<SchedulerService>,
    pub devices_service: Arc<DevicesService>,
    pub air_quality_service: Arc<AirQualityService>,
    pub config: Arc<AppConfig>,
}

/// Create shared HTTP client with connection pooling
fn create_http_client(config: &AppConfig) -> Result<Client, reqwest::Error> {
    Client::builder()
        .timeout(Duration::from_secs(config.request_timeout_secs))
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .pool_idle_timeout(Duration::from_secs(HTTP_POOL_IDLE_TIMEOUT_SECS))
        .pool_max_idle_per_host(10)
        .build()
}

/// Handle request timeout errors
async fn handle_timeout_error(err: BoxError) -> (StatusCode, String) {
    if err.is::<tower::timeout::error::Elapsed>() {
        (StatusCode::REQUEST_TIMEOUT, "Request timed out".to_string())
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Internal error: {}", err),
        )
    }
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl+c");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, starting graceful shutdown");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weathrs=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = AppConfig::load()?;
    tracing::info!("Configuration loaded successfully");

    // Create shared HTTP client with connection pooling
    let http_client = create_http_client(&config)?;
    tracing::debug!("Shared HTTP client created");

    // Initialize database
    let db_config = db::DbConfig {
        url: config.database_url.clone(),
        ..Default::default()
    };
    let db_pool = db::create_pool(&db_config).await?;
    db::run_migrations(&db_pool).await?;
    tracing::info!("Database initialized");

    // Create geocoding cache with in-memory TTL + SQLite persistence
    let geo_cache = create_geo_cache(db_pool.clone());
    start_cache_cleanup_task(geo_cache.clone());
    tracing::debug!("Geocoding cache initialized");

    // Create shared API call budget
    let api_budget = Arc::new(api_budget::ApiCallBudget::new(
        config.history_backfill.daily_budget,
    ));

    // Initialize services with shared client
    let weather_service = Arc::new(WeatherService::new(
        http_client.clone(),
        &config.openweathermap_api_key,
    ));
    let forecast_service = Arc::new(ForecastService::new(
        http_client.clone(),
        &config.openweathermap_api_key,
        geo_cache.clone(),
    ));
    let history_service = Arc::new(HistoryService::new(
        http_client.clone(),
        &config.openweathermap_api_key,
        geo_cache,
        db_pool.clone(),
        Arc::clone(&api_budget),
    ));

    // Initialize air quality service
    let air_quality_service = Arc::new(AirQualityService::new(
        http_client.clone(),
        &config.openweathermap_api_key,
        Arc::clone(&forecast_service),
    ));

    // Initialize devices service backed by SQLite
    let devices_service = Arc::new(DevicesService::new(http_client.clone(), db_pool.clone()));
    tracing::info!("Devices service initialized");

    // Initialize scheduler backed by SQLite
    let scheduler_service = Arc::new(
        SchedulerService::new(
            Arc::clone(&forecast_service),
            Arc::clone(&devices_service),
            db_pool.clone(),
        )
        .await?,
    );

    // Initialize scheduler storage and load persisted jobs
    scheduler_service.init().await?;

    // Load jobs from config file (if any, and if not already in storage)
    if config.scheduler.enabled && !config.scheduler.jobs.is_empty() {
        let job_config = JobConfig {
            jobs: config.scheduler.jobs.clone(),
        };
        scheduler_service.load_jobs(&job_config).await?;
    }

    // Start the scheduler
    scheduler_service.start().await?;
    tracing::info!(
        job_count = scheduler_service.get_jobs().await.len(),
        "Scheduler started"
    );

    // Schedule history backfill job if enabled
    if config.history_backfill.enabled {
        backfill::schedule_backfill_job(
            Arc::clone(&scheduler_service),
            Arc::clone(&history_service),
            Arc::clone(&devices_service),
            config.history_backfill.clone(),
            Arc::clone(&api_budget),
        )
        .await?;
    }

    // Create shared application state
    let state = AppState {
        http_client,
        db_pool,
        weather_service,
        forecast_service,
        history_service,
        scheduler_service,
        devices_service,
        air_quality_service,
        config: Arc::new(config.clone()),
    };

    // Build CORS layer
    let cors = if config.cors_allowed_origins.is_empty() {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(
                config
                    .cors_allowed_origins
                    .iter()
                    .filter_map(|o| o.parse().ok()),
            ))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                http::header::CONTENT_TYPE,
                http::header::HeaderName::from_static("x-api-key"),
                http::header::AUTHORIZATION,
            ])
    };

    // Build router using the routes module
    let app = routes::build_router(state.clone())
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(
            ServiceBuilder::new()
                // Handle timeout errors
                .layer(HandleErrorLayer::new(handle_timeout_error))
                // Request timeout (configurable)
                .timeout(Duration::from_secs(config.request_timeout_secs)),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server with graceful shutdown
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("Server shutdown complete");

    Ok(())
}
