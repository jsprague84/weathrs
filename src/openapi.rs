use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::error::ErrorResponse;
use crate::history::models::{
    DailyHistoryResponse, DailyHistorySummary, HistoryDataPoint, HistoryResponse, TrendExtreme,
    TrendResponse, TrendSummary,
};
use crate::air_quality::models::{AirQualityComponents, AirQualityResponse};
use crate::forecast::models::WidgetResponse;
use crate::weather::service::WeatherResponse;

/// OpenAPI documentation for the Weathrs API
///
/// This provides basic schema documentation. Full path annotations
/// can be added incrementally to handlers as needed.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Weathrs API",
        version = "1.0.0",
        description = "A streamlined Rust weather API using OpenWeatherMap. Provides current weather, forecasts, history, trends, and scheduled notifications.",
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
        ),
        contact(
            name = "Weathrs",
            url = "https://github.com/jsprague84/weathrs"
        )
    ),
    tags(
        (name = "weather", description = "Current weather data"),
        (name = "forecast", description = "Weather forecasts (daily, hourly)"),
        (name = "widget", description = "Lightweight widget data"),
        (name = "air-quality", description = "Air quality and pollution data"),
        (name = "history", description = "Historical weather data and trends"),
        (name = "scheduler", description = "Scheduled forecast jobs"),
        (name = "devices", description = "Device registration for push notifications")
    ),
    components(
        schemas(
            ErrorResponse,
            WeatherResponse,
            HistoryResponse,
            HistoryDataPoint,
            DailyHistoryResponse,
            DailyHistorySummary,
            TrendResponse,
            TrendSummary,
            TrendExtreme,
            WidgetResponse,
            AirQualityResponse,
            AirQualityComponents,
        )
    )
)]
pub struct ApiDoc;

/// Create the Swagger UI router
pub fn swagger_ui() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())
}
