use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use super::jobs::ForecastJob;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct JobListResponse {
    pub jobs: Vec<ForecastJob>,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
pub struct TriggerRequest {
    pub city: String,
    #[serde(default = "default_units")]
    pub units: String,
}

fn default_units() -> String {
    "metric".to_string()
}

#[derive(Debug, Serialize)]
pub struct TriggerResponse {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// List all scheduled jobs
/// GET /scheduler/jobs
pub async fn list_jobs(State(state): State<AppState>) -> Json<JobListResponse> {
    let jobs = state.scheduler_service.get_jobs().await;
    Json(JobListResponse {
        count: jobs.len(),
        jobs,
    })
}

/// Trigger a manual forecast with notification
/// POST /scheduler/trigger
pub async fn trigger_forecast(
    State(state): State<AppState>,
    Json(request): Json<TriggerRequest>,
) -> impl IntoResponse {
    match state
        .scheduler_service
        .run_now(&request.city, &request.units)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(TriggerResponse {
                status: "success".to_string(),
                message: format!("Forecast triggered for {}", request.city),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Trigger forecast for a specific city via path
/// POST /scheduler/trigger/{city}
pub async fn trigger_forecast_by_city(
    State(state): State<AppState>,
    Path(city): Path<String>,
) -> impl IntoResponse {
    match state.scheduler_service.run_now(&city, "metric").await {
        Ok(()) => (
            StatusCode::OK,
            Json(TriggerResponse {
                status: "success".to_string(),
                message: format!("Forecast triggered for {}", city),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Get scheduler status
/// GET /scheduler/status
pub async fn scheduler_status(State(state): State<AppState>) -> Json<SchedulerStatus> {
    let jobs = state.scheduler_service.get_jobs().await;
    Json(SchedulerStatus {
        running: true,
        job_count: jobs.len(),
        notifications_configured: state.notification_service.is_configured(),
    })
}

#[derive(Debug, Serialize)]
pub struct SchedulerStatus {
    pub running: bool,
    pub job_count: usize,
    pub notifications_configured: bool,
}
