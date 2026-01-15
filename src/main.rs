mod config;
mod weather;

use axum::{routing::get, Router};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::AppConfig;
use crate::weather::{handlers, WeatherService};

#[derive(Clone)]
pub struct AppState {
    pub weather_service: Arc<WeatherService>,
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

    // Initialize weather service
    let weather_service = WeatherService::new(&config.openweathermap_api_key);

    // Create shared application state
    let state = AppState {
        weather_service: Arc::new(weather_service),
        config: Arc::new(config.clone()),
    };

    // Build router
    let app = Router::new()
        .route("/", get(handlers::health))
        .route("/health", get(handlers::health))
        .route("/weather", get(handlers::get_weather))
        .route("/weather/{city}", get(handlers::get_weather_by_city))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
