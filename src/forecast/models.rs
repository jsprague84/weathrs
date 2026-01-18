use serde::{Deserialize, Serialize};

// ============================================================================
// Geocoding API Response
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GeoLocation {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub country: String,
    pub state: Option<String>,
}

/// Response from ZIP code geocoding API (different format than city)
#[derive(Debug, Deserialize)]
pub struct ZipGeoLocation {
    pub zip: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub country: String,
}

impl From<ZipGeoLocation> for GeoLocation {
    fn from(z: ZipGeoLocation) -> Self {
        GeoLocation {
            name: z.name,
            lat: z.lat,
            lon: z.lon,
            country: z.country,
            state: None,
        }
    }
}

// ============================================================================
// One Call API 3.0 Response (Internal)
// These structs deserialize the raw API response; not all fields are used
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct OneCallResponse {
    pub lat: f64,
    pub lon: f64,
    pub timezone: String,
    pub timezone_offset: i32,
    pub current: Option<CurrentWeather>,
    pub minutely: Option<Vec<MinutelyForecast>>,
    pub hourly: Option<Vec<HourlyForecast>>,
    pub daily: Option<Vec<DailyForecast>>,
    pub alerts: Option<Vec<WeatherAlert>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct CurrentWeather {
    pub dt: i64,
    pub sunrise: Option<i64>,
    pub sunset: Option<i64>,
    pub temp: f64,
    pub feels_like: f64,
    pub pressure: u32,
    pub humidity: u32,
    pub dew_point: f64,
    pub uvi: f64,
    pub clouds: u32,
    pub visibility: Option<u32>,
    pub wind_speed: f64,
    pub wind_deg: u32,
    pub wind_gust: Option<f64>,
    pub weather: Vec<WeatherCondition>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct MinutelyForecast {
    pub dt: i64,
    pub precipitation: f64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct HourlyForecast {
    pub dt: i64,
    pub temp: f64,
    pub feels_like: f64,
    pub pressure: u32,
    pub humidity: u32,
    pub dew_point: f64,
    pub uvi: f64,
    pub clouds: u32,
    pub visibility: Option<u32>,
    pub wind_speed: f64,
    pub wind_deg: u32,
    pub wind_gust: Option<f64>,
    pub pop: f64, // Probability of precipitation
    pub rain: Option<PrecipitationVolume>,
    pub snow: Option<PrecipitationVolume>,
    pub weather: Vec<WeatherCondition>,
}

#[derive(Debug, Deserialize)]
pub struct PrecipitationVolume {
    #[serde(rename = "1h")]
    pub one_hour: Option<f64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct DailyForecast {
    pub dt: i64,
    pub sunrise: i64,
    pub sunset: i64,
    pub moonrise: i64,
    pub moonset: i64,
    pub moon_phase: f64,
    pub summary: Option<String>,
    pub temp: DailyTemperature,
    pub feels_like: DailyFeelsLike,
    pub pressure: u32,
    pub humidity: u32,
    pub dew_point: f64,
    pub wind_speed: f64,
    pub wind_deg: u32,
    pub wind_gust: Option<f64>,
    pub clouds: u32,
    pub pop: f64,
    pub rain: Option<f64>,
    pub snow: Option<f64>,
    pub uvi: f64,
    pub weather: Vec<WeatherCondition>,
}

#[derive(Debug, Deserialize)]
pub struct DailyTemperature {
    pub day: f64,
    pub min: f64,
    pub max: f64,
    pub night: f64,
    pub eve: f64,
    pub morn: f64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct DailyFeelsLike {
    pub day: f64,
    pub night: f64,
    pub eve: f64,
    pub morn: f64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct WeatherCondition {
    pub id: u32,
    pub main: String,
    pub description: String,
    pub icon: String,
}

#[derive(Debug, Deserialize)]
pub struct WeatherAlert {
    pub sender_name: String,
    pub event: String,
    pub start: i64,
    pub end: i64,
    pub description: String,
    pub tags: Option<Vec<String>>,
}

// ============================================================================
// API Response Models (External - what we return to clients)
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ForecastResponse {
    pub location: LocationInfo,
    pub timezone: String,
    pub current: Option<CurrentWeatherResponse>,
    pub hourly: Vec<HourlyForecastResponse>,
    pub daily: Vec<DailyForecastResponse>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<AlertResponse>,
}

#[derive(Debug, Serialize)]
pub struct LocationInfo {
    pub city: String,
    pub country: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Serialize)]
pub struct CurrentWeatherResponse {
    pub timestamp: i64,
    pub temperature: f64,
    pub feels_like: f64,
    pub humidity: u32,
    pub pressure: u32,
    pub uv_index: f64,
    pub clouds: u32,
    pub visibility: Option<u32>,
    pub wind_speed: f64,
    pub wind_direction: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_gust: Option<f64>,
    pub description: String,
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunrise: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct HourlyForecastResponse {
    pub timestamp: i64,
    pub temperature: f64,
    pub feels_like: f64,
    pub humidity: u32,
    pub pressure: u32,
    pub uv_index: f64,
    pub clouds: u32,
    pub wind_speed: f64,
    pub wind_direction: u32,
    pub precipitation_probability: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rain_volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow_volume: Option<f64>,
    pub description: String,
    pub icon: String,
}

#[derive(Debug, Serialize)]
pub struct DailyForecastResponse {
    pub timestamp: i64,
    pub sunrise: i64,
    pub sunset: i64,
    pub moon_phase: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub temp_min: f64,
    pub temp_max: f64,
    pub temp_day: f64,
    pub temp_night: f64,
    pub temp_morning: f64,
    pub temp_evening: f64,
    pub feels_like_day: f64,
    pub feels_like_night: f64,
    pub humidity: u32,
    pub pressure: u32,
    pub uv_index: f64,
    pub clouds: u32,
    pub wind_speed: f64,
    pub wind_direction: u32,
    pub precipitation_probability: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rain_volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow_volume: Option<f64>,
    pub description: String,
    pub icon: String,
}

#[derive(Debug, Serialize)]
pub struct AlertResponse {
    pub sender: String,
    pub event: String,
    pub start: i64,
    pub end: i64,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}
