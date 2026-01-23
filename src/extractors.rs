use axum::{
    extract::{FromRequestParts, Path, Query},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use crate::error::ErrorResponse;

/// Query parameters for weather/forecast requests
#[derive(Debug, Deserialize)]
pub struct WeatherQuery {
    /// City name from query string
    pub city: Option<String>,
    /// Units: metric, imperial, or standard
    pub units: Option<String>,
}

/// Extracts city from either path parameter or query parameter
///
/// Checks path first, then falls back to query parameter.
/// Returns None if city is not provided in either location.
#[derive(Debug)]
pub struct CityParam(pub Option<String>);

impl CityParam {
    /// Get the city value or use a default
    pub fn or_default(self, default: impl Into<String>) -> String {
        self.0.unwrap_or_else(|| default.into())
    }

    /// Get the city value
    pub fn into_inner(self) -> Option<String> {
        self.0
    }
}

impl<S> FromRequestParts<S> for CityParam
where
    S: Send + Sync,
{
    type Rejection = CityParamRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Try to extract city from path first
        if let Ok(Path(city)) = Path::<String>::from_request_parts(parts, state).await {
            if !city.is_empty() {
                return Ok(CityParam(Some(city)));
            }
        }

        // Fall back to query parameter
        if let Ok(Query(query)) = Query::<WeatherQuery>::from_request_parts(parts, state).await {
            return Ok(CityParam(query.city));
        }

        // No city provided - that's okay, handler can use default
        Ok(CityParam(None))
    }
}

/// Extracts units from query parameter
#[derive(Debug)]
pub struct UnitsParam(pub Option<String>);

impl UnitsParam {
    /// Get the units value or use a default
    pub fn or_default(self, default: impl Into<String>) -> String {
        self.0.unwrap_or_else(|| default.into())
    }
}

impl<S> FromRequestParts<S> for UnitsParam
where
    S: Send + Sync,
{
    type Rejection = CityParamRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Ok(Query(query)) = Query::<WeatherQuery>::from_request_parts(parts, state).await {
            return Ok(UnitsParam(query.units));
        }

        Ok(UnitsParam(None))
    }
}

/// Rejection type for city parameter extraction failures
#[derive(Debug)]
pub struct CityParamRejection(pub String);

impl IntoResponse for CityParamRejection {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(self.0))).into_response()
    }
}
