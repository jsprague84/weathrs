mod config;
mod forecast;
mod notifications;
mod scheduler;
mod weather;

use axum::{
    error_handling::HandleErrorLayer, http::StatusCode, routing::get, routing::post, BoxError,
    Router,
};
use reqwest::Client;
use std::{sync::Arc, time::Duration};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::AppConfig;
use crate::forecast::{handlers as forecast_handlers, ForecastService};
use crate::notifications::{NotificationService, NotificationServiceConfig};
use crate::scheduler::{handlers as scheduler_handlers, JobConfig, SchedulerService};
use crate::weather::{handlers as weather_handlers, WeatherService};

/// Shared HTTP client configuration
const HTTP_TIMEOUT_SECS: u64 = 30;
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 5;
const HTTP_POOL_IDLE_TIMEOUT_SECS: u64 = 90;

#[derive(Clone)]
pub struct AppState {
    pub http_client: Client,
    pub weather_service: Arc<WeatherService>,
    pub forecast_service: Arc<ForecastService>,
    pub notification_service: Arc<NotificationService>,
    pub scheduler_service: Arc<SchedulerService>,
    pub config: Arc<AppConfig>,
}

/// Create shared HTTP client with connection pooling
fn create_http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
        .pool_idle_timeout(Duration::from_secs(HTTP_POOL_IDLE_TIMEOUT_SECS))
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to create HTTP client")
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
    let http_client = create_http_client();
    tracing::debug!("Shared HTTP client created");

    // Initialize services with shared client
    let weather_service = Arc::new(WeatherService::new(
        http_client.clone(),
        &config.openweathermap_api_key,
    ));
    let forecast_service = Arc::new(ForecastService::new(
        http_client.clone(),
        &config.openweathermap_api_key,
    ));

    // Initialize notification service with shared client
    let notification_service = Arc::new(NotificationService::from_config(
        NotificationServiceConfig {
            client: http_client.clone(),
            ntfy_url: config.notifications.ntfy.as_ref().map(|n| n.url.as_str()),
            ntfy_topic: config.notifications.ntfy.as_ref().map(|n| n.topic.as_str()),
            ntfy_token: config
                .notifications
                .ntfy
                .as_ref()
                .and_then(|n| n.token.as_deref()),
            ntfy_username: config
                .notifications
                .ntfy
                .as_ref()
                .and_then(|n| n.username.as_deref()),
            ntfy_password: config
                .notifications
                .ntfy
                .as_ref()
                .and_then(|n| n.password.as_deref()),
            gotify_url: config.notifications.gotify.as_ref().map(|g| g.url.as_str()),
            gotify_token: config
                .notifications
                .gotify
                .as_ref()
                .map(|g| g.token.as_str()),
        },
    ));

    if notification_service.is_configured() {
        tracing::info!("Notification service configured");
    } else {
        tracing::info!("No notification services configured");
    }

    // Initialize scheduler
    let scheduler_service = Arc::new(
        SchedulerService::new(
            Arc::clone(&forecast_service),
            Arc::clone(&notification_service),
        )
        .await?,
    );

    // Load scheduled jobs from config
    if config.scheduler.enabled && !config.scheduler.jobs.is_empty() {
        let job_config = JobConfig {
            jobs: config.scheduler.jobs.clone(),
        };
        scheduler_service.load_jobs(&job_config).await?;
        scheduler_service.start().await?;
        tracing::info!(
            job_count = config.scheduler.jobs.len(),
            "Scheduler started with jobs"
        );
    } else {
        scheduler_service.start().await?;
        tracing::info!("Scheduler started (no jobs configured)");
    }

    // Create shared application state
    let state = AppState {
        http_client,
        weather_service,
        forecast_service,
        notification_service,
        scheduler_service,
        config: Arc::new(config.clone()),
    };

    // Build router
    let app = Router::new()
        // Health check
        .route("/", get(weather_handlers::health))
        .route("/health", get(weather_handlers::health))
        // Current weather (basic API)
        .route("/weather", get(weather_handlers::get_weather))
        .route(
            "/weather/{city}",
            get(weather_handlers::get_weather_by_city),
        )
        // Forecast (One Call API 3.0)
        .route("/forecast", get(forecast_handlers::get_forecast))
        .route(
            "/forecast/{city}",
            get(forecast_handlers::get_forecast_by_city),
        )
        .route(
            "/forecast/daily",
            get(forecast_handlers::get_daily_forecast),
        )
        .route(
            "/forecast/daily/{city}",
            get(forecast_handlers::get_daily_forecast_by_city),
        )
        .route(
            "/forecast/hourly",
            get(forecast_handlers::get_hourly_forecast),
        )
        .route(
            "/forecast/hourly/{city}",
            get(forecast_handlers::get_hourly_forecast_by_city),
        )
        // Scheduler endpoints
        .route(
            "/scheduler/status",
            get(scheduler_handlers::scheduler_status),
        )
        .route("/scheduler/jobs", get(scheduler_handlers::list_jobs))
        .route(
            "/scheduler/trigger",
            post(scheduler_handlers::trigger_forecast),
        )
        .route(
            "/scheduler/trigger/{city}",
            post(scheduler_handlers::trigger_forecast_by_city),
        )
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
