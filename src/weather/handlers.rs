use axum::{extract::State, Json};
use serde::Serialize;

use super::service::{WeatherError, WeatherResponse};
use crate::extractors::{CityParam, UnitsParam};
use crate::AppState;

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

/// Get weather for a city
///
/// Accepts city from either path parameter or query parameter:
/// - GET /weather?city=London&units=metric
/// - GET /weather/{city}?units=metric
pub async fn get_weather(
    State(state): State<AppState>,
    CityParam(city): CityParam,
    UnitsParam(units): UnitsParam,
) -> Result<Json<WeatherResponse>, WeatherError> {
    let city = city.unwrap_or_else(|| state.config.default_city.clone());
    let units = units.unwrap_or_else(|| state.config.units.clone());

    let weather = state.weather_service.get_weather(&city, &units).await?;
    Ok(Json(weather))
}
