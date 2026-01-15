use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use super::service::{WeatherError, WeatherResponse};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct WeatherQuery {
    /// City name to get weather for
    pub city: Option<String>,
    /// Units: metric, imperial, or standard
    pub units: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// Health check endpoint
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Get weather by query parameter or default city
///
/// Uses Result<T, E> where E: IntoResponse - Axum best practice
/// The `?` operator automatically converts WeatherError to a response
pub async fn get_weather(
    State(state): State<AppState>,
    Query(query): Query<WeatherQuery>,
) -> Result<Json<WeatherResponse>, WeatherError> {
    let city = query
        .city
        .unwrap_or_else(|| state.config.default_city.clone());
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let weather = state.weather_service.get_weather(&city, &units).await?;
    Ok(Json(weather))
}

/// Get weather by city path parameter
///
/// Extractor order follows best practice: Path -> Query -> State
/// (though State can be anywhere since it doesn't consume body)
pub async fn get_weather_by_city(
    State(state): State<AppState>,
    Path(city): Path<String>,
    Query(query): Query<WeatherQuery>,
) -> Result<Json<WeatherResponse>, WeatherError> {
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let weather = state.weather_service.get_weather(&city, &units).await?;
    Ok(Json(weather))
}
