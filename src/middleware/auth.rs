use axum::{
    body::Body,
    extract::Extension,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

use crate::error::ErrorResponse;

/// Wrapper type for the device API key
#[derive(Clone)]
pub struct DeviceApiKey(pub Option<String>);

/// Middleware that requires a valid API key for device endpoints
///
/// If `device_api_key` is not configured (None), all requests are allowed (development mode).
/// If configured, the `X-API-Key` header must match the configured key.
pub async fn require_api_key(
    Extension(DeviceApiKey(expected_key)): Extension<DeviceApiKey>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // If no API key is configured, allow all requests
    let Some(expected) = expected_key else {
        return next.run(request).await;
    };

    // Check for API key in header
    let provided_key = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok());

    match provided_key {
        Some(key) if key == expected => {
            // Valid API key, proceed
            next.run(request).await
        }
        Some(_) => {
            // Invalid API key
            tracing::warn!("Invalid API key provided for device endpoint");
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::with_code(
                    "Invalid API key",
                    "INVALID_API_KEY",
                )),
            )
                .into_response()
        }
        None => {
            // Missing API key
            tracing::warn!("Missing API key for device endpoint");
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::with_code(
                    "API key required. Provide X-API-Key header.",
                    "MISSING_API_KEY",
                )),
            )
                .into_response()
        }
    }
}
