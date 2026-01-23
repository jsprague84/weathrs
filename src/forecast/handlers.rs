use axum::{extract::State, Json};

use super::models::ForecastResponse;
use super::service::ForecastError;
use crate::extractors::{CityParam, UnitsParam};
use crate::AppState;

/// Get full forecast (current + 48h hourly + 8 day daily)
///
/// Accepts city from either path parameter or query parameter:
/// - GET /forecast?city=London&units=metric
/// - GET /forecast/{city}?units=metric
pub async fn get_forecast(
    State(state): State<AppState>,
    CityParam(city): CityParam,
    UnitsParam(units): UnitsParam,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let city = city.unwrap_or_else(|| state.config.default_city.clone());
    let units = units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state.forecast_service.get_forecast(&city, &units).await?;
    Ok(Json(forecast))
}

/// Get daily forecast only (8 days)
///
/// Accepts city from either path parameter or query parameter:
/// - GET /forecast/daily?city=London&units=metric
/// - GET /forecast/daily/{city}?units=metric
pub async fn get_daily_forecast(
    State(state): State<AppState>,
    CityParam(city): CityParam,
    UnitsParam(units): UnitsParam,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let city = city.unwrap_or_else(|| state.config.default_city.clone());
    let units = units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state
        .forecast_service
        .get_daily_forecast(&city, &units)
        .await?;
    Ok(Json(forecast))
}

/// Get hourly forecast only (48 hours)
///
/// Accepts city from either path parameter or query parameter:
/// - GET /forecast/hourly?city=London&units=metric
/// - GET /forecast/hourly/{city}?units=metric
pub async fn get_hourly_forecast(
    State(state): State<AppState>,
    CityParam(city): CityParam,
    UnitsParam(units): UnitsParam,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let city = city.unwrap_or_else(|| state.config.default_city.clone());
    let units = units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state
        .forecast_service
        .get_hourly_forecast(&city, &units)
        .await?;
    Ok(Json(forecast))
}
