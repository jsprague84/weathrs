use axum::http::StatusCode;
use reqwest::Client;
use thiserror::Error;

use super::models::*;
use crate::cache::{normalize_cache_key, CachedGeoLocation, GeoCache};
use crate::error::HttpError;
use crate::impl_into_response;

const GEOCODING_API_URL: &str = "https://api.openweathermap.org/geo/1.0/direct";
const ZIP_GEOCODING_API_URL: &str = "https://api.openweathermap.org/geo/1.0/zip";
const ONE_CALL_API_URL: &str = "https://api.openweathermap.org/data/3.0/onecall";

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

impl HttpError for ForecastError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::CityNotFound(_) => StatusCode::NOT_FOUND,
            Self::RequestError(_) => StatusCode::BAD_GATEWAY,
            Self::ApiError(_) => StatusCode::BAD_REQUEST,
            Self::InvalidResponse(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::SubscriptionRequired => StatusCode::PAYMENT_REQUIRED,
        }
    }

    fn error_code(&self) -> Option<&'static str> {
        match self {
            Self::CityNotFound(_) => Some("CITY_NOT_FOUND"),
            Self::RequestError(_) => Some("REQUEST_ERROR"),
            Self::ApiError(_) => Some("API_ERROR"),
            Self::InvalidResponse(_) => Some("INVALID_RESPONSE"),
            Self::SubscriptionRequired => Some("SUBSCRIPTION_REQUIRED"),
        }
    }
}

impl_into_response!(ForecastError);

pub struct ForecastService {
    client: Client,
    api_key: String,
    geo_cache: GeoCache,
}

impl ForecastService {
    pub fn new(client: Client, api_key: &str, geo_cache: GeoCache) -> Self {
        Self {
            client,
            api_key: api_key.to_string(),
            geo_cache,
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

    /// Get coordinates for a location using the Geocoding API
    /// Supports both city names ("Chicago") and zip codes ("60601" or "60601,US")
    /// Results are cached for 24 hours
    pub async fn geocode(&self, location: &str) -> Result<GeoLocation, ForecastError> {
        let cache_key = normalize_cache_key(location);

        // Check cache first
        if let Some(cached) = self.geo_cache.get(&cache_key) {
            tracing::debug!(location = %location, "Geocoding cache hit");
            return Ok(GeoLocation {
                name: cached.name,
                lat: cached.lat,
                lon: cached.lon,
                country: cached.country,
                state: cached.state,
            });
        }

        tracing::debug!(location = %location, "Geocoding cache miss");

        // Fetch from API
        let result = if Self::is_zip_code(location) {
            self.geocode_zip(location).await
        } else {
            self.geocode_city(location).await
        }?;

        // Cache the result
        self.geo_cache.insert(
            cache_key,
            CachedGeoLocation {
                name: result.name.clone(),
                lat: result.lat,
                lon: result.lon,
                country: result.country.clone(),
                state: result.state.clone(),
            },
        );

        Ok(result)
    }

    /// Geocode by city name
    async fn geocode_city(&self, city: &str) -> Result<GeoLocation, ForecastError> {
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

    /// Geocode by zip code (e.g., "60601" or "60601,US")
    async fn geocode_zip(&self, zip: &str) -> Result<GeoLocation, ForecastError> {
        // Default to US if no country specified
        let zip_query = if zip.contains(',') {
            zip.to_string()
        } else {
            format!("{},US", zip)
        };

        tracing::debug!(zip = %zip_query, "Geocoding zip code");

        let response = self
            .client
            .get(ZIP_GEOCODING_API_URL)
            .query(&[("zip", &zip_query), ("appid", &self.api_key)])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ForecastError::ApiError(format!(
                "Zip geocoding failed: {}",
                text
            )));
        }

        let location: ZipGeoLocation = response.json().await?;
        Ok(location.into())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_zip_code_us_numeric() {
        assert!(ForecastService::is_zip_code("60601"));
        assert!(ForecastService::is_zip_code("90210"));
        assert!(ForecastService::is_zip_code("10001"));
    }

    #[test]
    fn test_is_zip_code_with_country() {
        assert!(ForecastService::is_zip_code("60601,US"));
        assert!(ForecastService::is_zip_code("90210,US"));
        assert!(ForecastService::is_zip_code("10001,DE"));
    }

    #[test]
    fn test_is_zip_code_trims_whitespace() {
        assert!(ForecastService::is_zip_code(" 60601 "));
        assert!(ForecastService::is_zip_code("60601 ,US"));
    }

    #[test]
    fn test_is_not_zip_code_city_names() {
        assert!(!ForecastService::is_zip_code("Chicago"));
        assert!(!ForecastService::is_zip_code("London"));
        assert!(!ForecastService::is_zip_code("New York"));
    }

    #[test]
    fn test_is_not_zip_code_city_with_country() {
        assert!(!ForecastService::is_zip_code("London,GB"));
        assert!(!ForecastService::is_zip_code("Paris,FR"));
    }

    #[test]
    fn test_is_not_zip_code_mixed() {
        assert!(!ForecastService::is_zip_code("E14 5AB")); // UK postal code
        assert!(!ForecastService::is_zip_code("SW1A 1AA,GB"));
    }

    #[test]
    fn test_is_not_zip_code_multiple_commas() {
        assert!(!ForecastService::is_zip_code("60601,US,IL"));
    }

    fn create_test_location() -> GeoLocation {
        GeoLocation {
            name: "Chicago".to_string(),
            lat: 41.8781,
            lon: -87.6298,
            country: "US".to_string(),
            state: Some("Illinois".to_string()),
        }
    }

    fn create_minimal_one_call_response() -> OneCallResponse {
        OneCallResponse {
            lat: 41.8781,
            lon: -87.6298,
            timezone: "America/Chicago".to_string(),
            timezone_offset: -18000,
            current: None,
            minutely: None,
            hourly: None,
            daily: None,
            alerts: None,
        }
    }

    #[test]
    fn test_transform_response_minimal() {
        let geo_cache = crate::cache::create_geo_cache();
        let service = ForecastService::new(reqwest::Client::new(), "test_api_key", geo_cache);

        let data = create_minimal_one_call_response();
        let location = create_test_location();
        let result = service.transform_response(data, location);

        assert_eq!(result.location.city, "Chicago");
        assert_eq!(result.location.country, "US");
        assert_eq!(result.timezone, "America/Chicago");
        assert!(result.current.is_none());
        assert!(result.hourly.is_empty());
        assert!(result.daily.is_empty());
        assert!(result.alerts.is_empty());
    }

    #[test]
    fn test_transform_response_with_current_weather() {
        let geo_cache = crate::cache::create_geo_cache();
        let service = ForecastService::new(reqwest::Client::new(), "test_api_key", geo_cache);

        let mut data = create_minimal_one_call_response();
        data.current = Some(CurrentWeather {
            dt: 1700000000,
            sunrise: Some(1699980000),
            sunset: Some(1700020000),
            temp: 20.5,
            feels_like: 19.0,
            pressure: 1013,
            humidity: 65,
            dew_point: 14.0,
            uvi: 3.5,
            clouds: 40,
            visibility: Some(10000),
            wind_speed: 5.5,
            wind_deg: 180,
            wind_gust: Some(8.0),
            weather: vec![WeatherCondition {
                id: 800,
                main: "Clear".to_string(),
                description: "clear sky".to_string(),
                icon: "01d".to_string(),
            }],
        });

        let location = create_test_location();
        let result = service.transform_response(data, location);

        let current = result.current.expect("Current weather should be present");
        assert_eq!(current.temperature, 20.5);
        assert_eq!(current.feels_like, 19.0);
        assert_eq!(current.humidity, 65);
        assert_eq!(current.description, "clear sky");
        assert_eq!(current.icon, "01d");
    }

    #[test]
    fn test_transform_response_with_alerts() {
        let geo_cache = crate::cache::create_geo_cache();
        let service = ForecastService::new(reqwest::Client::new(), "test_api_key", geo_cache);

        let mut data = create_minimal_one_call_response();
        data.alerts = Some(vec![WeatherAlert {
            sender_name: "NWS Chicago".to_string(),
            event: "Heat Advisory".to_string(),
            start: 1700000000,
            end: 1700100000,
            description: "Excessive heat expected".to_string(),
            tags: Some(vec!["Extreme temperature".to_string()]),
        }]);

        let location = create_test_location();
        let result = service.transform_response(data, location);

        assert_eq!(result.alerts.len(), 1);
        assert_eq!(result.alerts[0].event, "Heat Advisory");
        assert_eq!(result.alerts[0].sender, "NWS Chicago");
    }
}
