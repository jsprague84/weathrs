use std::collections::HashMap;

use axum::{extract::State, Json};
use serde::Serialize;

use crate::db::history_repo::CityHistoryStats;
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    pub api_budget: ApiBudgetStats,
    pub history: HistoryStatsResponse,
    pub devices: DeviceStats,
    pub scheduler: SchedulerStats,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiBudgetStats {
    pub daily_limit: u32,
    pub used_today: u32,
    pub remaining_today: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStatsResponse {
    pub total_records: i64,
    pub cities: Vec<CityHistoryStats>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStats {
    pub total: usize,
    pub enabled: usize,
    pub by_platform: HashMap<String, usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerStats {
    pub total_jobs: usize,
    pub enabled_jobs: usize,
}

pub async fn get_stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let api_budget = ApiBudgetStats {
        daily_limit: state.api_budget.daily_limit(),
        used_today: state.api_budget.used_today(),
        remaining_today: state.api_budget.remaining(),
    };

    let history = match state.history_service.get_stats().await {
        Ok(stats) => HistoryStatsResponse {
            total_records: stats.total_records,
            cities: stats.cities,
        },
        Err(_) => HistoryStatsResponse {
            total_records: 0,
            cities: Vec::new(),
        },
    };

    let all_devices = state.devices_service.get_all().await;
    let enabled = all_devices.iter().filter(|d| d.enabled).count();
    let mut by_platform: HashMap<String, usize> = HashMap::new();
    for device in &all_devices {
        *by_platform
            .entry(format!("{:?}", device.platform).to_lowercase())
            .or_default() += 1;
    }
    let devices = DeviceStats {
        total: all_devices.len(),
        enabled,
        by_platform,
    };

    let jobs = state.scheduler_service.get_jobs().await;
    let scheduler = SchedulerStats {
        total_jobs: jobs.len(),
        enabled_jobs: jobs.iter().filter(|j| j.enabled).count(),
    };

    Json(StatsResponse {
        api_budget,
        history,
        devices,
        scheduler,
    })
}
