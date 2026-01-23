use axum::http::StatusCode;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

use crate::error::HttpError;
use crate::impl_into_response;

const OPENWEATHERMAP_API_URL: &str = "https://api.openweathermap.org/data/2.5/weather";

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

impl HttpError for WeatherError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::CityNotFound(_) => StatusCode::NOT_FOUND,
            Self::RequestError(_) => StatusCode::BAD_GATEWAY,
            Self::ApiError(_) => StatusCode::BAD_REQUEST,
            Self::InvalidResponse(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> Option<&'static str> {
        match self {
            Self::CityNotFound(_) => Some("CITY_NOT_FOUND"),
            Self::RequestError(_) => Some("REQUEST_ERROR"),
            Self::ApiError(_) => Some("API_ERROR"),
            Self::InvalidResponse(_) => Some("INVALID_RESPONSE"),
        }
    }
}

impl_into_response!(WeatherError);

#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
    pub fn new(client: Client, api_key: &str) -> Self {
        Self {
            client,
            api_key: api_key.to_string(),
        }
    }

    /// Check if input looks like a zip code (digits only, or digits,country)
    fn is_zip_code(input: &str) -> bool {
        let parts: Vec<&str> = input.split(',').collect();
        match parts.as_slice() {
            [zip] => zip.trim().chars().all(|c| c.is_ascii_digit()),
            [zip, _country] => zip.trim().chars().all(|c| c.is_ascii_digit()),
            _ => false,
        }
    }

    pub async fn get_weather(
        &self,
        location: &str,
        units: &str,
    ) -> Result<WeatherResponse, WeatherError> {
        tracing::debug!(location = %location, units = %units, "Fetching weather data");

        // Build query based on whether input is zip code or city name
        let response = if Self::is_zip_code(location) {
            // For zip codes, default to US if no country specified
            let zip_query = if location.contains(',') {
                location.to_string()
            } else {
                format!("{},US", location)
            };
            tracing::debug!(zip = %zip_query, "Using zip code query");
            self.client
                .get(OPENWEATHERMAP_API_URL)
                .query(&[
                    ("zip", zip_query.as_str()),
                    ("appid", self.api_key.as_str()),
                    ("units", units),
                ])
                .send()
                .await?
        } else {
            self.client
                .get(OPENWEATHERMAP_API_URL)
                .query(&[
                    ("q", location),
                    ("appid", self.api_key.as_str()),
                    ("units", units),
                ])
                .send()
                .await?
        };

        let status = response.status();
        tracing::debug!(status = %status, "Received API response");

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(WeatherError::CityNotFound(location.to_string()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_zip_code_us_numeric() {
        assert!(WeatherService::is_zip_code("60601"));
        assert!(WeatherService::is_zip_code("90210"));
        assert!(WeatherService::is_zip_code("10001"));
    }

    #[test]
    fn test_is_zip_code_with_country() {
        assert!(WeatherService::is_zip_code("60601,US"));
        assert!(WeatherService::is_zip_code("90210,US"));
        assert!(WeatherService::is_zip_code("10001,DE"));
    }

    #[test]
    fn test_is_zip_code_trims_whitespace() {
        assert!(WeatherService::is_zip_code(" 60601 "));
        assert!(WeatherService::is_zip_code("60601 ,US"));
    }

    #[test]
    fn test_is_not_zip_code_city_names() {
        assert!(!WeatherService::is_zip_code("Chicago"));
        assert!(!WeatherService::is_zip_code("London"));
        assert!(!WeatherService::is_zip_code("New York"));
    }

    #[test]
    fn test_is_not_zip_code_city_with_country() {
        assert!(!WeatherService::is_zip_code("London,GB"));
        assert!(!WeatherService::is_zip_code("Paris,FR"));
    }

    #[test]
    fn test_is_not_zip_code_mixed() {
        assert!(!WeatherService::is_zip_code("E14 5AB")); // UK postal code
        assert!(!WeatherService::is_zip_code("SW1A 1AA,GB"));
    }

    #[test]
    fn test_is_not_zip_code_multiple_commas() {
        assert!(!WeatherService::is_zip_code("60601,US,IL"));
    }
}
