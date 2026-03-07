use axum::{extract::State, Json};

use super::models::AirQualityResponse;
use super::service::AirQualityError;
use crate::extractors::CityParam;
use crate::AppState;

/// Get air quality data for a city
///
/// Uses the OWM Air Pollution API to return current AQI and pollutant concentrations.
///
/// - GET /air-quality/{city}
pub async fn get_air_quality(
    State(state): State<AppState>,
    CityParam(city): CityParam,
) -> Result<Json<AirQualityResponse>, AirQualityError> {
    let city = city.unwrap_or_else(|| state.config.default_city.clone());

    let air_quality = state.air_quality_service.get_air_quality(&city).await?;
    Ok(Json(air_quality))
}
