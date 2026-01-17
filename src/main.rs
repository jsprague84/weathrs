mod config;
mod forecast;
mod weather;

use axum::{routing::get, Router};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::AppConfig;
use crate::forecast::{handlers as forecast_handlers, ForecastService};
use crate::weather::{handlers as weather_handlers, WeatherService};

#[derive(Clone)]
pub struct AppState {
    pub weather_service: Arc<WeatherService>,
    pub forecast_service: Arc<ForecastService>,
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
    let weather_service = WeatherService::new(&config.openweathermap_api_key);
    let forecast_service = ForecastService::new(&config.openweathermap_api_key);

    // Create shared application state
    let state = AppState {
        weather_service: Arc::new(weather_service),
        forecast_service: Arc::new(forecast_service),
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
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
