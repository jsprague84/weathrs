use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    Json,
};

use super::models::{ForecastResponse, WidgetResponse};
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

/// Get minimal widget data for home screen widgets
///
/// Returns a lightweight payload with current temp, daily high/low, icon, and description.
/// Includes Cache-Control header for aggressive caching (5 minutes).
///
/// - GET /widget/{city}?units=metric
pub async fn get_widget(
    State(state): State<AppState>,
    CityParam(city): CityParam,
    UnitsParam(units): UnitsParam,
) -> Result<(HeaderMap, Json<WidgetResponse>), ForecastError> {
    let city = city.unwrap_or_else(|| state.config.default_city.clone());
    let units = units.unwrap_or_else(|| state.config.units.clone());

    let forecast = state.forecast_service.get_forecast(&city, &units).await?;

    let current = forecast.current.as_ref().ok_or_else(|| {
        ForecastError::InvalidResponse("No current weather data available".to_string())
    })?;

    let today = forecast.daily.first().ok_or_else(|| {
        ForecastError::InvalidResponse("No daily forecast data available".to_string())
    })?;

    let widget = WidgetResponse {
        city: forecast.location.city,
        temperature: current.temperature,
        high: today.temp_max,
        low: today.temp_min,
        icon: current.icon.clone(),
        description: current.description.clone(),
        units,
        updated_at: current.timestamp,
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );

    Ok((headers, Json(widget)))
}
