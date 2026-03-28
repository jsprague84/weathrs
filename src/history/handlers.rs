use axum::{
    extract::{Path, Query, State},
    Json,
};

use super::models::{
    DailyHistoryResponse, HistoryQuery, HistoryResponse, TrendResponse, TrendsQuery,
};
use super::service::HistoryError;
use crate::AppState;

/// Delete all history records for a location key
///
/// DELETE /history/location/{location_key}
pub async fn delete_history(
    State(state): State<AppState>,
    Path(location_key): Path<String>,
) -> Result<Json<serde_json::Value>, HistoryError> {
    let deleted = state
        .history_service
        .delete_by_location_key(&location_key)
        .await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "deleted": deleted
    })))
}

/// Normalize duplicate city names across location keys
///
/// POST /history/cleanup
pub async fn cleanup_history(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HistoryError> {
    let updated = state.history_service.cleanup_duplicate_locations().await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "updated": updated
    })))
}

/// Get hourly history data for a city
///
/// GET /history/{city}?start={unix}&end={unix}&units={units}
pub async fn get_history(
    State(state): State<AppState>,
    Path(city): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<HistoryResponse>, HistoryError> {
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let response = state
        .history_service
        .get_history(&city, query.start, query.end, &units)
        .await?;

    Ok(Json(response))
}

/// Get daily aggregated history for a city
///
/// GET /history/{city}/daily?start={unix}&end={unix}&units={units}
pub async fn get_daily_history(
    State(state): State<AppState>,
    Path(city): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<DailyHistoryResponse>, HistoryError> {
    let units = query.units.unwrap_or_else(|| state.config.units.clone());

    let response = state
        .history_service
        .get_daily_history(&city, query.start, query.end, &units)
        .await?;

    Ok(Json(response))
}

/// Get weather trends with summary statistics
///
/// GET /history/{city}/trends?period=7d|30d|90d&units={units}
pub async fn get_trends(
    State(state): State<AppState>,
    Path(city): Path<String>,
    Query(query): Query<TrendsQuery>,
) -> Result<Json<TrendResponse>, HistoryError> {
    let units = query.units.unwrap_or_else(|| state.config.units.clone());
    let period = query.period.unwrap_or_else(|| "7d".to_string());

    let response = state
        .history_service
        .get_trends(&city, &period, &units, query.start, query.end)
        .await?;

    Ok(Json(response))
}
