use axum::{
    extract::{Path, Query, State},
    Json,
};

use super::models::{
    DailyHistoryResponse, HistoryQuery, HistoryResponse, TrendResponse, TrendsQuery,
};
use super::service::HistoryError;
use crate::AppState;

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
        .get_trends(&city, &period, &units)
        .await?;

    Ok(Json(response))
}
