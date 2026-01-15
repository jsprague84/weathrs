use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use super::service::WeatherError;

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

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Health check endpoint
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Get weather by query parameter or default city
pub async fn get_weather(
    State(state): State<AppState>,
    Query(query): Query<WeatherQuery>,
) -> impl IntoResponse {
    let city = query.city.unwrap_or_else(|| state.config.default_city.clone());
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    fetch_weather(&state, &city, &units).await
}

/// Get weather by city path parameter
pub async fn get_weather_by_city(
    State(state): State<AppState>,
    Path(city): Path<String>,
    Query(query): Query<WeatherQuery>,
) -> impl IntoResponse {
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    fetch_weather(&state, &city, &units).await
}

async fn fetch_weather(state: &AppState, city: &str, units: &str) -> impl IntoResponse {
    match state.weather_service.get_weather(city, units).await {
        Ok(weather) => (StatusCode::OK, Json(serde_json::to_value(weather).unwrap())).into_response(),
        Err(e) => {
            let (status, message) = match &e {
                WeatherError::CityNotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
                WeatherError::RequestError(_) => (StatusCode::BAD_GATEWAY, e.to_string()),
                WeatherError::ApiError(_) => (StatusCode::BAD_REQUEST, e.to_string()),
            };
            (status, Json(ErrorResponse { error: message })).into_response()
        }
    }
}
