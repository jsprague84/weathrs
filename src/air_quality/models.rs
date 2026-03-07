use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ============================================================================
// OWM Air Pollution API Response (Internal)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AirPollutionResponse {
    pub list: Vec<AirPollutionEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AirPollutionEntry {
    pub dt: i64,
    pub main: AirPollutionMain,
    pub components: AirPollutionComponents,
}

#[derive(Debug, Deserialize)]
pub struct AirPollutionMain {
    pub aqi: u8,
}

#[derive(Debug, Deserialize)]
pub struct AirPollutionComponents {
    pub co: f64,
    pub no: f64,
    pub no2: f64,
    pub o3: f64,
    pub so2: f64,
    pub pm2_5: f64,
    pub pm10: f64,
    pub nh3: f64,
}

// ============================================================================
// API Response Models (External)
// ============================================================================

/// Air quality data for a city
#[derive(Debug, Serialize, ToSchema)]
pub struct AirQualityResponse {
    pub city: String,
    /// Air Quality Index (1-5)
    pub aqi: u8,
    /// Human-readable AQI label
    pub aqi_label: &'static str,
    /// Pollutant concentrations (μg/m³)
    pub components: AirQualityComponents,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AirQualityComponents {
    pub co: f64,
    pub no: f64,
    pub no2: f64,
    pub o3: f64,
    pub so2: f64,
    pub pm2_5: f64,
    pub pm10: f64,
    pub nh3: f64,
}

/// Convert AQI number to human-readable label
pub fn aqi_label(aqi: u8) -> &'static str {
    match aqi {
        1 => "Good",
        2 => "Fair",
        3 => "Moderate",
        4 => "Poor",
        5 => "Very Poor",
        _ => "Unknown",
    }
}
