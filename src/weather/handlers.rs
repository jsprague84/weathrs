use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use std::time::Instant;

use super::service::{WeatherError, WeatherResponse};
use crate::extractors::{CityParam, UnitsParam};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// Health check endpoint (lightweight, for load balancers)
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Debug, Serialize)]
pub struct DeepHealthResponse {
    pub status: String,
    pub components: DeepHealthComponents,
    pub version: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DeepHealthComponents {
    pub database: ComponentHealth,
    pub openweathermap: ComponentHealth,
}

#[derive(Debug, Serialize)]
pub struct ComponentHealth {
    pub status: String,
    pub latency_ms: u64,
}

/// Deep health check endpoint - verifies database and OWM connectivity
/// GET /health/deep
pub async fn health_deep(State(state): State<AppState>) -> impl IntoResponse {
    let mut overall_healthy = true;

    // Check database
    let db_start = Instant::now();
    let db_status = match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db_pool)
        .await
    {
        Ok(_) => ComponentHealth {
            status: "healthy".to_string(),
            latency_ms: db_start.elapsed().as_millis() as u64,
        },
        Err(_) => {
            overall_healthy = false;
            ComponentHealth {
                status: "unhealthy".to_string(),
                latency_ms: db_start.elapsed().as_millis() as u64,
            }
        }
    };

    // Check OWM API with a lightweight geocoding request
    let owm_start = Instant::now();
    let owm_status = match state
        .http_client
        .get("https://api.openweathermap.org/geo/1.0/direct")
        .query(&[
            ("q", "London"),
            ("limit", "1"),
            ("appid", &state.config.openweathermap_api_key),
        ])
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => ComponentHealth {
            status: "healthy".to_string(),
            latency_ms: owm_start.elapsed().as_millis() as u64,
        },
        Ok(resp) => {
            overall_healthy = false;
            ComponentHealth {
                status: format!("degraded (HTTP {})", resp.status()),
                latency_ms: owm_start.elapsed().as_millis() as u64,
            }
        }
        Err(_) => {
            overall_healthy = false;
            ComponentHealth {
                status: "unhealthy".to_string(),
                latency_ms: owm_start.elapsed().as_millis() as u64,
            }
        }
    };

    let status = if overall_healthy {
        "healthy"
    } else if db_status.status == "healthy" || owm_status.status.starts_with("healthy") {
        "degraded"
    } else {
        "unhealthy"
    };

    let response = DeepHealthResponse {
        status: status.to_string(),
        components: DeepHealthComponents {
            database: db_status,
            openweathermap: owm_status,
        },
        version: env!("CARGO_PKG_VERSION"),
    };

    let status_code = if overall_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(response))
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
