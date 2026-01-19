use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::AppState;

use super::models::{
    DeviceRegistrationRequest, DeviceResponse, DeviceSettingsRequest, DeviceUnregisterRequest,
    TestNotificationRequest,
};

/// POST /devices/register - Register a device for push notifications
pub async fn register_device(
    State(state): State<AppState>,
    Json(request): Json<DeviceRegistrationRequest>,
) -> impl IntoResponse {
    match state.devices_service.register(request).await {
        Ok(device) => (
            StatusCode::OK,
            Json(DeviceResponse::success(Some(device.id))),
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to register device");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeviceResponse::error(e.to_string())),
            )
        }
    }
}

/// POST /devices/unregister - Unregister a device
pub async fn unregister_device(
    State(state): State<AppState>,
    Json(request): Json<DeviceUnregisterRequest>,
) -> impl IntoResponse {
    match state.devices_service.unregister(&request.token).await {
        Ok(removed) => {
            if removed {
                (StatusCode::OK, Json(DeviceResponse::success(None)))
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(DeviceResponse::error("Device not found")),
                )
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to unregister device");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeviceResponse::error(e.to_string())),
            )
        }
    }
}

/// PUT /devices/settings - Update device settings
pub async fn update_device_settings(
    State(state): State<AppState>,
    Json(request): Json<DeviceSettingsRequest>,
) -> impl IntoResponse {
    match state.devices_service.update_settings(request).await {
        Ok(device) => (
            StatusCode::OK,
            Json(DeviceResponse::success(Some(device.id))),
        ),
        Err(super::service::DevicesError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(DeviceResponse::error("Device not found")),
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to update device settings");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeviceResponse::error(e.to_string())),
            )
        }
    }
}

/// POST /devices/test - Send a test notification
pub async fn send_test_notification(
    State(state): State<AppState>,
    Json(request): Json<TestNotificationRequest>,
) -> impl IntoResponse {
    match state.devices_service.send_test(&request.token).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({
                "status": "success",
                "message": "Test notification sent"
            })),
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to send test notification");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": e.to_string()
                })),
            )
        }
    }
}

/// GET /devices/count - Get registered device count
pub async fn get_device_count(State(state): State<AppState>) -> impl IntoResponse {
    let count = state.devices_service.count().await;
    Json(json!({
        "count": count
    }))
}
