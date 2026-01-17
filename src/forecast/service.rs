use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;
use thiserror::Error;

use super::models::*;

const GEOCODING_API_URL: &str = "https://api.openweathermap.org/geo/1.0/direct";
const ONE_CALL_API_URL: &str = "https://api.openweathermap.org/data/3.0/onecall";
const DEFAULT_TIMEOUT_SECS: u64 = 15;

#[derive(Error, Debug)]
pub enum ForecastError {
    #[error("Failed to fetch data: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("City not found: {0}")]
    CityNotFound(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Invalid API response: {0}")]
    InvalidResponse(String),

    #[error("One Call API subscription required. Subscribe at https://openweathermap.org/api/one-call-3")]
    SubscriptionRequired,
}

impl IntoResponse for ForecastError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ForecastError::CityNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            ForecastError::RequestError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            ForecastError::ApiError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ForecastError::InvalidResponse(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
            ForecastError::SubscriptionRequired => (StatusCode::PAYMENT_REQUIRED, self.to_string()),
        };

        tracing::error!(error = %self, status = %status, "Forecast API error");

        (status, Json(ErrorBody { error: message })).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

pub struct ForecastService {
    client: Client,
    api_key: String,
}

impl ForecastService {
    pub fn new(api_key: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key: api_key.to_string(),
        }
    }

    /// Get coordinates for a city using the Geocoding API
    pub async fn geocode(&self, city: &str) -> Result<GeoLocation, ForecastError> {
        tracing::debug!(city = %city, "Geocoding city");

        let response = self
            .client
            .get(GEOCODING_API_URL)
            .query(&[("q", city), ("limit", "1"), ("appid", &self.api_key)])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ForecastError::ApiError(format!(
                "Geocoding failed: {}",
                text
            )));
        }

        let locations: Vec<GeoLocation> = response.json().await?;

        locations
            .into_iter()
            .next()
            .ok_or_else(|| ForecastError::CityNotFound(city.to_string()))
    }

    /// Get full forecast using One Call API 3.0
    pub async fn get_forecast(
        &self,
        city: &str,
        units: &str,
    ) -> Result<ForecastResponse, ForecastError> {
        // First, geocode the city to get coordinates
        let location = self.geocode(city).await?;

        tracing::debug!(
            city = %location.name,
            lat = %location.lat,
            lon = %location.lon,
            "Fetching forecast"
        );

        // Then fetch the forecast using One Call API
        let response = self
            .client
            .get(ONE_CALL_API_URL)
            .query(&[
                ("lat", location.lat.to_string()),
                ("lon", location.lon.to_string()),
                ("units", units.to_string()),
                ("appid", self.api_key.clone()),
                ("exclude", "minutely".to_string()), // Skip minute-by-minute data
            ])
            .send()
            .await?;

        let status = response.status();
        tracing::debug!(status = %status, "Received One Call API response");

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ForecastError::SubscriptionRequired);
        }

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ForecastError::ApiError(text));
        }

        let data: OneCallResponse = response.json().await?;

        // Transform to our response format
        Ok(self.transform_response(data, location))
    }

    /// Get only daily forecast (8 days)
    pub async fn get_daily_forecast(
        &self,
        city: &str,
        units: &str,
    ) -> Result<ForecastResponse, ForecastError> {
        let location = self.geocode(city).await?;

        let response = self
            .client
            .get(ONE_CALL_API_URL)
            .query(&[
                ("lat", location.lat.to_string()),
                ("lon", location.lon.to_string()),
                ("units", units.to_string()),
                ("appid", self.api_key.clone()),
                ("exclude", "minutely,hourly".to_string()),
            ])
            .send()
            .await?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ForecastError::SubscriptionRequired);
        }

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ForecastError::ApiError(text));
        }

        let data: OneCallResponse = response.json().await?;
        Ok(self.transform_response(data, location))
    }

    /// Get only hourly forecast (48 hours)
    pub async fn get_hourly_forecast(
        &self,
        city: &str,
        units: &str,
    ) -> Result<ForecastResponse, ForecastError> {
        let location = self.geocode(city).await?;

        let response = self
            .client
            .get(ONE_CALL_API_URL)
            .query(&[
                ("lat", location.lat.to_string()),
                ("lon", location.lon.to_string()),
                ("units", units.to_string()),
                ("appid", self.api_key.clone()),
                ("exclude", "minutely,daily".to_string()),
            ])
            .send()
            .await?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ForecastError::SubscriptionRequired);
        }

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ForecastError::ApiError(text));
        }

        let data: OneCallResponse = response.json().await?;
        Ok(self.transform_response(data, location))
    }

    fn transform_response(&self, data: OneCallResponse, location: GeoLocation) -> ForecastResponse {
        ForecastResponse {
            location: LocationInfo {
                city: location.name,
                country: location.country,
                state: location.state,
                lat: location.lat,
                lon: location.lon,
            },
            timezone: data.timezone,
            current: data.current.map(|c| {
                let weather = c.weather.first();
                CurrentWeatherResponse {
                    timestamp: c.dt,
                    temperature: c.temp,
                    feels_like: c.feels_like,
                    humidity: c.humidity,
                    pressure: c.pressure,
                    uv_index: c.uvi,
                    clouds: c.clouds,
                    visibility: c.visibility,
                    wind_speed: c.wind_speed,
                    wind_direction: c.wind_deg,
                    wind_gust: c.wind_gust,
                    description: weather.map(|w| w.description.clone()).unwrap_or_default(),
                    icon: weather.map(|w| w.icon.clone()).unwrap_or_default(),
                    sunrise: c.sunrise,
                    sunset: c.sunset,
                }
            }),
            hourly: data
                .hourly
                .unwrap_or_default()
                .into_iter()
                .map(|h| {
                    let weather = h.weather.first();
                    HourlyForecastResponse {
                        timestamp: h.dt,
                        temperature: h.temp,
                        feels_like: h.feels_like,
                        humidity: h.humidity,
                        pressure: h.pressure,
                        uv_index: h.uvi,
                        clouds: h.clouds,
                        wind_speed: h.wind_speed,
                        wind_direction: h.wind_deg,
                        precipitation_probability: h.pop,
                        rain_volume: h.rain.and_then(|r| r.one_hour),
                        snow_volume: h.snow.and_then(|s| s.one_hour),
                        description: weather.map(|w| w.description.clone()).unwrap_or_default(),
                        icon: weather.map(|w| w.icon.clone()).unwrap_or_default(),
                    }
                })
                .collect(),
            daily: data
                .daily
                .unwrap_or_default()
                .into_iter()
                .map(|d| {
                    let weather = d.weather.first();
                    DailyForecastResponse {
                        timestamp: d.dt,
                        sunrise: d.sunrise,
                        sunset: d.sunset,
                        moon_phase: d.moon_phase,
                        summary: d.summary,
                        temp_min: d.temp.min,
                        temp_max: d.temp.max,
                        temp_day: d.temp.day,
                        temp_night: d.temp.night,
                        temp_morning: d.temp.morn,
                        temp_evening: d.temp.eve,
                        feels_like_day: d.feels_like.day,
                        feels_like_night: d.feels_like.night,
                        humidity: d.humidity,
                        pressure: d.pressure,
                        uv_index: d.uvi,
                        clouds: d.clouds,
                        wind_speed: d.wind_speed,
                        wind_direction: d.wind_deg,
                        precipitation_probability: d.pop,
                        rain_volume: d.rain,
                        snow_volume: d.snow,
                        description: weather.map(|w| w.description.clone()).unwrap_or_default(),
                        icon: weather.map(|w| w.icon.clone()).unwrap_or_default(),
                    }
                })
                .collect(),
            alerts: data
                .alerts
                .unwrap_or_default()
                .into_iter()
                .map(|a| AlertResponse {
                    sender: a.sender_name,
                    event: a.event,
                    start: a.start,
                    end: a.end,
                    description: a.description,
                    tags: a.tags,
                })
                .collect(),
        }
    }
}
