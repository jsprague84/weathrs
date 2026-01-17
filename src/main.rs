mod config;
mod forecast;
mod notifications;
mod scheduler;
mod weather;

use axum::{routing::get, routing::post, Router};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::AppConfig;
use crate::forecast::{handlers as forecast_handlers, ForecastService};
use crate::notifications::NotificationService;
use crate::scheduler::{handlers as scheduler_handlers, JobConfig, SchedulerService};
use crate::weather::{handlers as weather_handlers, WeatherService};

#[derive(Clone)]
pub struct AppState {
    pub weather_service: Arc<WeatherService>,
    pub forecast_service: Arc<ForecastService>,
    pub notification_service: Arc<NotificationService>,
    pub scheduler_service: Arc<SchedulerService>,
    pub config: Arc<AppConfig>,
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

    // Initialize services
    let weather_service = Arc::new(WeatherService::new(&config.openweathermap_api_key));
    let forecast_service = Arc::new(ForecastService::new(&config.openweathermap_api_key));

    // Initialize notification service
    let notification_service = Arc::new(NotificationService::from_config(
        config.notifications.ntfy.as_ref().map(|n| n.url.as_str()),
        config.notifications.ntfy.as_ref().map(|n| n.topic.as_str()),
        config
            .notifications
            .ntfy
            .as_ref()
            .and_then(|n| n.token.as_deref()),
        config.notifications.gotify.as_ref().map(|g| g.url.as_str()),
        config
            .notifications
            .gotify
            .as_ref()
            .map(|g| g.token.as_str()),
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
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
