use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use super::models::ForecastResponse;
use super::service::ForecastError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ForecastQuery {
    /// City name
    pub city: Option<String>,
    /// Units: metric, imperial, or standard
    pub units: Option<String>,
}

/// Get full forecast (current + 48h hourly + 8 day daily)
///
/// GET /forecast?city=London&units=metric
pub async fn get_forecast(
    State(state): State<AppState>,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let city = query
        .city
        .unwrap_or_else(|| state.config.default_city.clone());
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state.forecast_service.get_forecast(&city, &units).await?;
    Ok(Json(forecast))
}

/// Get full forecast by city path parameter
///
/// GET /forecast/{city}?units=metric
pub async fn get_forecast_by_city(
    State(state): State<AppState>,
    Path(city): Path<String>,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state.forecast_service.get_forecast(&city, &units).await?;
    Ok(Json(forecast))
}

/// Get daily forecast only (8 days)
///
/// GET /forecast/daily?city=London&units=metric
pub async fn get_daily_forecast(
    State(state): State<AppState>,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let city = query
        .city
        .unwrap_or_else(|| state.config.default_city.clone());
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state
        .forecast_service
        .get_daily_forecast(&city, &units)
        .await?;
    Ok(Json(forecast))
}

/// Get daily forecast by city path parameter
///
/// GET /forecast/daily/{city}?units=metric
pub async fn get_daily_forecast_by_city(
    State(state): State<AppState>,
    Path(city): Path<String>,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state
        .forecast_service
        .get_daily_forecast(&city, &units)
        .await?;
    Ok(Json(forecast))
}

/// Get hourly forecast only (48 hours)
///
/// GET /forecast/hourly?city=London&units=metric
pub async fn get_hourly_forecast(
    State(state): State<AppState>,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let city = query
        .city
        .unwrap_or_else(|| state.config.default_city.clone());
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state
        .forecast_service
        .get_hourly_forecast(&city, &units)
        .await?;
    Ok(Json(forecast))
}

/// Get hourly forecast by city path parameter
///
/// GET /forecast/hourly/{city}?units=metric
pub async fn get_hourly_forecast_by_city(
    State(state): State<AppState>,
    Path(city): Path<String>,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<ForecastResponse>, ForecastError> {
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state
        .forecast_service
        .get_hourly_forecast(&city, &units)
        .await?;
    Ok(Json(forecast))
}
