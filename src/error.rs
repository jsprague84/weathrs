use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

/// Standard error response format for all API errors
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: None,
        }
    }

    pub fn with_code(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: Some(code.into()),
        }
    }
}

/// Trait for errors that can be converted to HTTP responses
pub trait HttpError: std::error::Error {
    /// HTTP status code for this error
    fn status_code(&self) -> StatusCode;

    /// Optional error code for programmatic handling (e.g., "CITY_NOT_FOUND")
    fn error_code(&self) -> Option<&'static str> {
        None
    }
}

/// Convert any HttpError into an Axum response
pub fn into_response<E: HttpError>(err: E) -> Response {
    let status = err.status_code();
    let code = err.error_code();
    let message = err.to_string();

    tracing::error!(
        error = %message,
        status = %status,
        code = ?code,
        "API error"
    );

    let body = if let Some(code) = code {
        ErrorResponse::with_code(message, code)
    } else {
        ErrorResponse::new(message)
    };

    (status, Json(body)).into_response()
}

/// Macro to implement IntoResponse for HttpError types
#[macro_export]
macro_rules! impl_into_response {
    ($error_type:ty) => {
        impl axum::response::IntoResponse for $error_type {
            fn into_response(self) -> axum::response::Response {
                $crate::error::into_response(self)
            }
        }
    };
}
