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
    pub backfill: BackfillConfigStats,
    pub database: DatabaseStats,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackfillConfigStats {
    pub enabled: bool,
    pub max_years: u32,
    pub daily_budget: u32,
    pub cron: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStats {
    pub size_bytes: u64,
    pub size_human: String,
}

fn get_db_size(url: &str) -> (u64, String) {
    let path = url.strip_prefix("sqlite:").unwrap_or(url);
    match std::fs::metadata(path) {
        Ok(meta) => {
            let bytes = meta.len();
            let human = if bytes < 1024 {
                format!("{} B", bytes)
            } else if bytes < 1024 * 1024 {
                format!("{:.1} KB", bytes as f64 / 1024.0)
            } else if bytes < 1024 * 1024 * 1024 {
                format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
            } else {
                format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
            };
            (bytes, human)
        }
        Err(_) => (0, "unknown".to_string()),
    }
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

    let backfill = BackfillConfigStats {
        enabled: state.config.history_backfill.enabled,
        max_years: state.config.history_backfill.max_years,
        daily_budget: state.config.history_backfill.daily_budget,
        cron: state.config.history_backfill.cron.clone(),
    };

    let (size_bytes, size_human) = get_db_size(&state.config.database_url);
    let database = DatabaseStats {
        size_bytes,
        size_human,
    };

    Json(StatsResponse {
        api_budget,
        history,
        devices,
        scheduler,
        backfill,
        database,
    })
}
