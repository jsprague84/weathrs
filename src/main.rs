mod cache;
mod config;
mod db;
mod devices;
mod error;
mod extractors;
mod forecast;
mod middleware;
mod notifications;
mod openapi;
mod routes;
mod scheduler;
mod weather;

use axum::{error_handling::HandleErrorLayer, http::StatusCode, BoxError};
use reqwest::Client;
use std::{sync::Arc, time::Duration};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::cache::{create_geo_cache, start_cache_cleanup_task};
use crate::config::AppConfig;
use crate::devices::DevicesService;
use crate::forecast::ForecastService;
use crate::scheduler::{JobConfig, SchedulerService};
use crate::weather::WeatherService;

/// Shared HTTP client configuration
const HTTP_TIMEOUT_SECS: u64 = 30;
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 5;
const HTTP_POOL_IDLE_TIMEOUT_SECS: u64 = 90;

#[derive(Clone)]
pub struct AppState {
    pub http_client: Client,
    pub weather_service: Arc<WeatherService>,
    pub forecast_service: Arc<ForecastService>,
    pub scheduler_service: Arc<SchedulerService>,
    pub devices_service: Arc<DevicesService>,
    pub config: Arc<AppConfig>,
}

/// Create shared HTTP client with connection pooling
fn create_http_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
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
    let http_client = create_http_client()?;
    tracing::debug!("Shared HTTP client created");

    // Create geocoding cache with 24-hour TTL
    let geo_cache = create_geo_cache();
    start_cache_cleanup_task(geo_cache.clone());
    tracing::debug!("Geocoding cache initialized");

    // Initialize services with shared client
    let weather_service = Arc::new(WeatherService::new(
        http_client.clone(),
        &config.openweathermap_api_key,
    ));
    let forecast_service = Arc::new(ForecastService::new(
        http_client.clone(),
        &config.openweathermap_api_key,
        geo_cache,
    ));

    // Initialize devices service for Expo push notifications
    let devices_service = Arc::new(DevicesService::new(
        http_client.clone(),
        "data/devices.json",
    ));
    devices_service.init().await?;
    tracing::info!("Devices service initialized");

    // Initialize scheduler with persistent storage
    let scheduler_service = Arc::new(
        SchedulerService::new(
            Arc::clone(&forecast_service),
            Arc::clone(&devices_service),
            "data/scheduler_jobs.json",
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

    // Create shared application state
    let state = AppState {
        http_client,
        weather_service,
        forecast_service,
        scheduler_service,
        devices_service,
        config: Arc::new(config.clone()),
    };

    // Build router using the routes module
    let app = routes::build_router(state.clone())
        .layer(
            ServiceBuilder::new()
                // Handle timeout errors
                .layer(HandleErrorLayer::new(handle_timeout_error))
                // Request timeout (60 seconds for slow API calls)
                .timeout(Duration::from_secs(60)),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server with graceful shutdown
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown complete");

    Ok(())
}
