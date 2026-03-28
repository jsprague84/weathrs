use std::collections::HashMap;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::db::history_repo::CityHistoryStats;
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    pub api_budget: ApiBudgetStats,
    pub tile_usage: TileUsageStats,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TileUsageStats {
    pub owm_tiles: TileBudget,
    pub google_maps_tiles: TileBudget,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TileBudget {
    pub used_today: i64,
    pub daily_limit: u32,
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

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let tile_row: Option<(i64, i64)> =
        sqlx::query_as("SELECT owm_tiles, google_maps_tiles FROM tile_usage WHERE date = ?")
            .bind(&today)
            .fetch_optional(&state.db_pool)
            .await
            .unwrap_or(None);

    let tile_usage = TileUsageStats {
        owm_tiles: TileBudget {
            used_today: tile_row.map(|r| r.0).unwrap_or(0),
            daily_limit: state.config.owm_tile_daily_limit,
        },
        google_maps_tiles: TileBudget {
            used_today: tile_row.map(|r| r.1).unwrap_or(0),
            daily_limit: state.config.google_maps_tile_daily_limit,
        },
    };

    Json(StatsResponse {
        api_budget,
        tile_usage,
        history,
        devices,
        scheduler,
        backfill,
        database,
    })
}

#[derive(Deserialize)]
pub struct TileReport {
    pub owm_tiles: i64,
    pub google_maps_tiles: i64,
}

pub async fn report_tiles(
    State(state): State<AppState>,
    Json(report): Json<TileReport>,
) -> Json<serde_json::Value> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let _ = sqlx::query(
        "INSERT INTO tile_usage (date, owm_tiles, google_maps_tiles) VALUES (?, ?, ?)
         ON CONFLICT(date) DO UPDATE SET
           owm_tiles = owm_tiles + excluded.owm_tiles,
           google_maps_tiles = google_maps_tiles + excluded.google_maps_tiles",
    )
    .bind(&today)
    .bind(report.owm_tiles)
    .bind(report.google_maps_tiles)
    .execute(&state.db_pool)
    .await;

    Json(serde_json::json!({ "success": true }))
}
