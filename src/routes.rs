use std::sync::Arc;

use axum::{
    middleware,
    routing::{get, post, put},
    Extension, Router,
};
use tower_governor::{governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor, GovernorLayer};

use crate::config::RateLimitConfig;
use crate::devices::handlers as devices_handlers;
use crate::forecast::handlers as forecast_handlers;
use crate::history::handlers as history_handlers;
use crate::middleware::{require_api_key, DeviceApiKey};
use crate::openapi::swagger_ui;
use crate::scheduler::handlers as scheduler_handlers;
use crate::weather::handlers as weather_handlers;
use crate::AppState;

/// Build the weather API routes
fn weather_routes() -> Router<AppState> {
    Router::new()
        .route("/weather", get(weather_handlers::get_weather))
        .route("/weather/{city}", get(weather_handlers::get_weather))
}

/// Build the forecast API routes
fn forecast_routes() -> Router<AppState> {
    Router::new()
        .route("/forecast", get(forecast_handlers::get_forecast))
        .route("/forecast/{city}", get(forecast_handlers::get_forecast))
        .route(
            "/forecast/daily",
            get(forecast_handlers::get_daily_forecast),
        )
        .route(
            "/forecast/daily/{city}",
            get(forecast_handlers::get_daily_forecast),
        )
        .route(
            "/forecast/hourly",
            get(forecast_handlers::get_hourly_forecast),
        )
        .route(
            "/forecast/hourly/{city}",
            get(forecast_handlers::get_hourly_forecast),
        )
        .route("/widget/{city}", get(forecast_handlers::get_widget))
}

/// Build the scheduler API routes with separate rate limits for reads vs mutations
fn scheduler_routes(rate_limit: &RateLimitConfig) -> Router<AppState> {
    let mutation_config = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(rate_limit.mutation_rpm as u64 / 60 + 1)
            .burst_size(rate_limit.mutation_rpm)
            .key_extractor(SmartIpKeyExtractor)
            .use_headers()
            .finish()
            .unwrap(),
    );

    // Read endpoints (use general rate limit applied at router level)
    let read_routes = Router::new()
        .route(
            "/scheduler/status",
            get(scheduler_handlers::scheduler_status),
        )
        .route(
            "/scheduler/jobs",
            get(scheduler_handlers::list_jobs),
        )
        .route(
            "/scheduler/jobs/{id}",
            get(scheduler_handlers::get_job),
        );

    // Mutation endpoints (stricter rate limit)
    let mutation_routes = Router::new()
        .route(
            "/scheduler/jobs",
            post(scheduler_handlers::create_job),
        )
        .route(
            "/scheduler/jobs/{id}",
            put(scheduler_handlers::update_job)
                .delete(scheduler_handlers::delete_job),
        )
        .route(
            "/scheduler/trigger",
            post(scheduler_handlers::trigger_forecast),
        )
        .route(
            "/scheduler/trigger/{city}",
            post(scheduler_handlers::trigger_forecast_by_city),
        )
        .layer(GovernorLayer::new(mutation_config));

    read_routes.merge(mutation_routes)
}

/// Build the devices API routes (protected by API key auth)
fn devices_routes(api_key: Option<String>) -> Router<AppState> {
    Router::new()
        .route("/devices/register", post(devices_handlers::register_device))
        .route(
            "/devices/unregister",
            post(devices_handlers::unregister_device),
        )
        .route(
            "/devices/settings",
            put(devices_handlers::update_device_settings),
        )
        .route(
            "/devices/test",
            post(devices_handlers::send_test_notification),
        )
        .route("/devices/count", get(devices_handlers::get_device_count))
        .route("/devices/debug", get(devices_handlers::list_devices))
        .layer(Extension(DeviceApiKey(api_key)))
        .layer(middleware::from_fn(require_api_key))
}

/// Build the history API routes
fn history_routes() -> Router<AppState> {
    Router::new()
        .route("/history/{city}", get(history_handlers::get_history))
        .route(
            "/history/{city}/daily",
            get(history_handlers::get_daily_history),
        )
        .route("/history/{city}/trends", get(history_handlers::get_trends))
}

/// Build all API v1 routes
pub fn api_v1_routes(device_api_key: Option<String>, rate_limit: &RateLimitConfig) -> Router<AppState> {
    Router::new()
        .merge(weather_routes())
        .merge(forecast_routes())
        .merge(history_routes())
        .merge(scheduler_routes(rate_limit))
        .merge(devices_routes(device_api_key))
}

/// Build the complete application router
pub fn build_router(state: AppState) -> Router<AppState> {
    let device_api_key = state.config.device_api_key.clone();
    let rate_limit = state.config.rate_limit.clone();

    // General rate limit for all API routes
    let general_config = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(rate_limit.general_rpm as u64 / 60 + 1)
            .burst_size(rate_limit.general_rpm)
            .key_extractor(SmartIpKeyExtractor)
            .use_headers()
            .finish()
            .unwrap(),
    );

    Router::new()
        // Health check at root level (no rate limit)
        .route("/", get(weather_handlers::health))
        .route("/health", get(weather_handlers::health))
        .route("/health/deep", get(weather_handlers::health_deep))
        // API v1 routes with general rate limiting
        .nest(
            "/api/v1",
            api_v1_routes(device_api_key, &rate_limit)
                .layer(GovernorLayer::new(general_config)),
        )
        // Swagger UI for API documentation
        .merge(swagger_ui())
}
