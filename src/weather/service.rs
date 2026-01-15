use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

const OPENWEATHERMAP_API_URL: &str = "https://api.openweathermap.org/data/2.5/weather";
const DEFAULT_TIMEOUT_SECS: u64 = 10;

#[derive(Error, Debug)]
pub enum WeatherError {
    #[error("Failed to fetch weather data: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("City not found: {0}")]
    CityNotFound(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Invalid API response: {0}")]
    InvalidResponse(String),
}

// Implement IntoResponse for WeatherError - Axum best practice
// This allows handlers to return Result<T, WeatherError> directly
impl IntoResponse for WeatherError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            WeatherError::CityNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            WeatherError::RequestError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            WeatherError::ApiError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            WeatherError::InvalidResponse(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
        };

        tracing::error!(error = %self, status = %status, "Weather API error");

        (status, Json(ErrorBody { error: message })).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WeatherResponse {
    pub city: String,
    pub country: String,
    pub temperature: f64,
    pub feels_like: f64,
    pub humidity: u32,
    pub pressure: u32,
    pub wind_speed: f64,
    pub description: String,
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenWeatherMapResponse {
    name: String,
    sys: SysInfo,
    main: MainInfo,
    weather: Vec<WeatherInfo>,
    wind: WindInfo,
    visibility: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SysInfo {
    country: String,
}

#[derive(Debug, Deserialize)]
struct MainInfo {
    temp: f64,
    feels_like: f64,
    humidity: u32,
    pressure: u32,
}

#[derive(Debug, Deserialize)]
struct WeatherInfo {
    description: String,
    icon: String,
}

#[derive(Debug, Deserialize)]
struct WindInfo {
    speed: f64,
}

#[derive(Debug, Deserialize)]
struct OpenWeatherMapError {
    message: String,
}

pub struct WeatherService {
    client: Client,
    api_key: String,
}

impl WeatherService {
    pub fn new(api_key: &str) -> Self {
        // Configure client with timeout - reqwest best practice
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key: api_key.to_string(),
        }
    }

    pub async fn get_weather(
        &self,
        city: &str,
        units: &str,
    ) -> Result<WeatherResponse, WeatherError> {
        tracing::debug!(city = %city, units = %units, "Fetching weather data");

        // Use query builder for proper URL encoding - handles spaces and special chars
        let response = self
            .client
            .get(OPENWEATHERMAP_API_URL)
            .query(&[("q", city), ("appid", &self.api_key), ("units", units)])
            .send()
            .await?;

        let status = response.status();
        tracing::debug!(status = %status, "Received API response");

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(WeatherError::CityNotFound(city.to_string()));
        }

        if !status.is_success() {
            let error: OpenWeatherMapError = response.json().await.unwrap_or(OpenWeatherMapError {
                message: format!("HTTP {}", status),
            });
            return Err(WeatherError::ApiError(error.message));
        }

        let data: OpenWeatherMapResponse = response.json().await?;

        let weather_info = data.weather.first().ok_or_else(|| {
            WeatherError::InvalidResponse("No weather information available".to_string())
        })?;

        let weather = WeatherResponse {
            city: data.name,
            country: data.sys.country,
            temperature: data.main.temp,
            feels_like: data.main.feels_like,
            humidity: data.main.humidity,
            pressure: data.main.pressure,
            wind_speed: data.wind.speed,
            description: weather_info.description.clone(),
            icon: weather_info.icon.clone(),
            visibility: data.visibility,
        };

        tracing::info!(city = %weather.city, temp = %weather.temperature, "Weather data fetched successfully");

        Ok(weather)
    }
}
