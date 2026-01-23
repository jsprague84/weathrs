use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::error::ErrorResponse;
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
        description = "A streamlined Rust weather API using OpenWeatherMap. Provides current weather, forecasts, and scheduled notifications.",
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
        (name = "scheduler", description = "Scheduled forecast jobs"),
        (name = "devices", description = "Device registration for push notifications")
    ),
    components(
        schemas(
            ErrorResponse,
            WeatherResponse,
        )
    )
)]
pub struct ApiDoc;

/// Create the Swagger UI router
pub fn swagger_ui() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())
}
