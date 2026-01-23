use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::jobs::{ForecastJob, NotifyConfig};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct JobListResponse {
    pub jobs: Vec<ForecastJob>,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
pub struct TriggerRequest {
    pub city: String,
    pub units: Option<String>,
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

/// Request to create a new job
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobRequest {
    pub name: String,
    pub city: String,
    #[serde(default = "default_units")]
    pub units: String,
    pub cron: String,
    /// IANA timezone (e.g., "America/Chicago"). Defaults to UTC.
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_true")]
    pub include_daily: bool,
    #[serde(default)]
    pub include_hourly: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub notify: Option<NotifyConfigRequest>,
}

/// Request to update a job
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJobRequest {
    pub name: Option<String>,
    pub city: Option<String>,
    pub units: Option<String>,
    pub cron: Option<String>,
    /// IANA timezone (e.g., "America/Chicago")
    pub timezone: Option<String>,
    pub include_daily: Option<bool>,
    pub include_hourly: Option<bool>,
    pub enabled: Option<bool>,
    pub notify: Option<NotifyConfigRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyConfigRequest {
    pub on_run: Option<bool>,
    pub on_alert: Option<bool>,
    pub on_precipitation: Option<bool>,
    pub cold_threshold: Option<f64>,
    pub heat_threshold: Option<f64>,
}

fn default_units() -> String {
    "metric".to_string()
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_true() -> bool {
    true
}

/// Response for job operations
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ForecastJob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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
    let units = request.units.unwrap_or_else(|| state.config.units.clone());

    if let Err(e) = state.scheduler_service.run_now(&request.city, &units).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(TriggerResponse {
            status: "success".to_string(),
            message: format!("Forecast triggered for {}", request.city),
        }),
    )
        .into_response()
}

/// Trigger forecast for a specific city via path
/// POST /scheduler/trigger/{city}
pub async fn trigger_forecast_by_city(
    State(state): State<AppState>,
    Path(city): Path<String>,
) -> impl IntoResponse {
    let units = &state.config.units;

    if let Err(e) = state.scheduler_service.run_now(&city, units).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(TriggerResponse {
            status: "success".to_string(),
            message: format!("Forecast triggered for {}", city),
        }),
    )
        .into_response()
}

/// Get scheduler status
/// GET /scheduler/status
pub async fn scheduler_status(State(state): State<AppState>) -> Json<SchedulerStatus> {
    let jobs = state.scheduler_service.get_jobs().await;
    let device_count = state.devices_service.count().await;
    Json(SchedulerStatus {
        running: true,
        job_count: jobs.len(),
        device_count,
    })
}

#[derive(Debug, Serialize)]
pub struct SchedulerStatus {
    pub running: bool,
    pub job_count: usize,
    pub device_count: usize,
}

/// Create a new scheduled job
/// POST /scheduler/jobs
pub async fn create_job(
    State(state): State<AppState>,
    Json(request): Json<CreateJobRequest>,
) -> impl IntoResponse {
    let notify_config = request
        .notify
        .map(|n| NotifyConfig {
            on_run: n.on_run.unwrap_or(true),
            on_alert: n.on_alert.unwrap_or(true),
            on_precipitation: n.on_precipitation.unwrap_or(false),
            cold_threshold: n.cold_threshold,
            heat_threshold: n.heat_threshold,
        })
        .unwrap_or_default();

    let job = ForecastJob {
        id: Uuid::new_v4().to_string(),
        name: request.name,
        city: request.city,
        units: request.units,
        cron: request.cron,
        timezone: request.timezone,
        include_daily: request.include_daily,
        include_hourly: request.include_hourly,
        enabled: request.enabled,
        notify: notify_config,
    };

    match state.scheduler_service.create_job(job).await {
        Ok(created) => (
            StatusCode::CREATED,
            Json(JobResponse {
                success: true,
                job: Some(created),
                message: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to create job");
            (
                StatusCode::BAD_REQUEST,
                Json(JobResponse {
                    success: false,
                    job: None,
                    message: Some(e.to_string()),
                }),
            )
                .into_response()
        }
    }
}

/// Get a job by ID
/// GET /scheduler/jobs/{id}
pub async fn get_job(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.scheduler_service.get_job(&id).await {
        Some(job) => (
            StatusCode::OK,
            Json(JobResponse {
                success: true,
                job: Some(job),
                message: None,
            }),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(JobResponse {
                success: false,
                job: None,
                message: Some(format!("Job not found: {}", id)),
            }),
        )
            .into_response(),
    }
}

/// Update a job
/// PUT /scheduler/jobs/{id}
pub async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateJobRequest>,
) -> impl IntoResponse {
    // Get existing job first
    let existing = match state.scheduler_service.get_job(&id).await {
        Some(job) => job,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(JobResponse {
                    success: false,
                    job: None,
                    message: Some(format!("Job not found: {}", id)),
                }),
            )
                .into_response();
        }
    };

    // Merge updates
    let notify_config = if let Some(n) = request.notify {
        NotifyConfig {
            on_run: n.on_run.unwrap_or(existing.notify.on_run),
            on_alert: n.on_alert.unwrap_or(existing.notify.on_alert),
            on_precipitation: n
                .on_precipitation
                .unwrap_or(existing.notify.on_precipitation),
            cold_threshold: n.cold_threshold.or(existing.notify.cold_threshold),
            heat_threshold: n.heat_threshold.or(existing.notify.heat_threshold),
        }
    } else {
        existing.notify.clone()
    };

    let updated_job = ForecastJob {
        id: existing.id,
        name: request.name.unwrap_or(existing.name),
        city: request.city.unwrap_or(existing.city),
        units: request.units.unwrap_or(existing.units),
        cron: request.cron.unwrap_or(existing.cron),
        timezone: request.timezone.unwrap_or(existing.timezone),
        include_daily: request.include_daily.unwrap_or(existing.include_daily),
        include_hourly: request.include_hourly.unwrap_or(existing.include_hourly),
        enabled: request.enabled.unwrap_or(existing.enabled),
        notify: notify_config,
    };

    match state.scheduler_service.update_job(updated_job).await {
        Ok(job) => (
            StatusCode::OK,
            Json(JobResponse {
                success: true,
                job: Some(job),
                message: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to update job");
            (
                StatusCode::BAD_REQUEST,
                Json(JobResponse {
                    success: false,
                    job: None,
                    message: Some(e.to_string()),
                }),
            )
                .into_response()
        }
    }
}

/// Delete a job
/// DELETE /scheduler/jobs/{id}
pub async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.scheduler_service.delete_job(&id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(JobResponse {
                success: true,
                job: None,
                message: Some("Job deleted".to_string()),
            }),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(JobResponse {
                success: false,
                job: None,
                message: Some(format!("Job not found: {}", id)),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to delete job");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(JobResponse {
                    success: false,
                    job: None,
                    message: Some(e.to_string()),
                }),
            )
                .into_response()
        }
    }
}
