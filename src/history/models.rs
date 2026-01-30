use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ============================================================================
// API Response Models (External - what we return to clients)
// ============================================================================

/// A single hourly history data point
#[derive(Debug, Serialize, ToSchema)]
pub struct HistoryDataPoint {
    pub timestamp: i64,
    pub temperature: f64,
    pub feels_like: f64,
    pub humidity: i32,
    pub pressure: i32,
    pub wind_speed: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_direction: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clouds: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rain_1h: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow_1h: Option<f64>,
}

/// Aggregated daily history summary
#[derive(Debug, Serialize, ToSchema)]
pub struct DailyHistorySummary {
    pub date: String,
    pub temp_min: f64,
    pub temp_max: f64,
    pub temp_avg: f64,
    pub humidity_avg: f64,
    pub wind_speed_avg: f64,
    pub precipitation_total: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dominant_condition: Option<String>,
}

/// Response wrapper for hourly history data
#[derive(Debug, Serialize, ToSchema)]
pub struct HistoryResponse {
    pub city: String,
    pub units: String,
    pub period: String,
    pub data_points: Vec<HistoryDataPoint>,
}

/// Response wrapper for daily history summaries
#[derive(Debug, Serialize, ToSchema)]
pub struct DailyHistoryResponse {
    pub city: String,
    pub units: String,
    pub period: String,
    pub days: Vec<DailyHistorySummary>,
}

/// Response wrapper for trend analysis
#[derive(Debug, Serialize, ToSchema)]
pub struct TrendResponse {
    pub city: String,
    pub units: String,
    pub period: String,
    pub days: Vec<DailyHistorySummary>,
    pub summary: TrendSummary,
}

/// Summary of weather trends over a period
#[derive(Debug, Serialize, ToSchema)]
pub struct TrendSummary {
    pub avg_temp: f64,
    pub temp_trend: String,
    pub max_temp: TrendExtreme,
    pub min_temp: TrendExtreme,
    pub total_precipitation: f64,
    pub avg_humidity: f64,
}

/// A temperature extreme (max or min) with its date
#[derive(Debug, Serialize, ToSchema)]
pub struct TrendExtreme {
    pub value: f64,
    pub date: String,
}

// ============================================================================
// OWM Timemachine API Response (Internal deserialization)
// ============================================================================

/// Raw response from OWM One Call 3.0 Timemachine endpoint
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct TimemachineResponse {
    pub lat: f64,
    pub lon: f64,
    pub timezone: String,
    pub timezone_offset: i32,
    pub data: Vec<TimemachineData>,
}

/// A single data point from the Timemachine response
#[derive(Debug, Deserialize)]
pub struct TimemachineData {
    pub dt: i64,
    pub temp: f64,
    pub feels_like: f64,
    pub pressure: u32,
    pub humidity: u32,
    #[serde(default)]
    pub clouds: Option<u32>,
    #[serde(default)]
    pub visibility: Option<u32>,
    pub wind_speed: f64,
    #[serde(default)]
    pub wind_deg: Option<u32>,
    #[serde(default)]
    pub weather: Vec<TimemachineWeather>,
    #[serde(default)]
    pub rain: Option<TimemachinePrecip>,
    #[serde(default)]
    pub snow: Option<TimemachinePrecip>,
}

#[derive(Debug, Deserialize)]
pub struct TimemachineWeather {
    pub description: String,
    pub icon: String,
}

#[derive(Debug, Deserialize)]
pub struct TimemachinePrecip {
    #[serde(rename = "1h")]
    pub one_hour: Option<f64>,
}

// ============================================================================
// Query Parameters
// ============================================================================

/// Query parameters for history endpoints
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub units: Option<String>,
}

/// Query parameters for trends endpoint
#[derive(Debug, Deserialize)]
pub struct TrendsQuery {
    pub period: Option<String>,
    pub units: Option<String>,
    pub start: Option<i64>,
    pub end: Option<i64>,
}
