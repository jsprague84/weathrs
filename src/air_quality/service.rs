use axum::http::StatusCode;
use reqwest::Client;
use thiserror::Error;

use std::sync::Arc;

use super::models::*;
use crate::api_budget::ApiCallBudget;
use crate::error::HttpError;
use crate::forecast::ForecastService;
use crate::impl_into_response;

const AIR_POLLUTION_API_URL: &str = "https://api.openweathermap.org/data/2.5/air_pollution";

#[derive(Error, Debug)]
pub enum AirQualityError {
    #[error("Failed to fetch data: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("City not found: {0}")]
    CityNotFound(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("No air quality data available")]
    NoData,
}

impl HttpError for AirQualityError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::CityNotFound(_) => StatusCode::NOT_FOUND,
            Self::RequestError(_) => StatusCode::BAD_GATEWAY,
            Self::ApiError(_) => StatusCode::BAD_REQUEST,
            Self::NoData => StatusCode::NOT_FOUND,
        }
    }

    fn error_code(&self) -> Option<&'static str> {
        match self {
            Self::CityNotFound(_) => Some("CITY_NOT_FOUND"),
            Self::RequestError(_) => Some("REQUEST_ERROR"),
            Self::ApiError(_) => Some("API_ERROR"),
            Self::NoData => Some("NO_DATA"),
        }
    }
}

impl_into_response!(AirQualityError);

pub struct AirQualityService {
    client: Client,
    api_key: String,
    forecast_service: Arc<ForecastService>,
    api_budget: Arc<ApiCallBudget>,
}

impl AirQualityService {
    pub fn new(
        client: Client,
        api_key: &str,
        forecast_service: Arc<ForecastService>,
        api_budget: Arc<ApiCallBudget>,
    ) -> Self {
        Self {
            client,
            api_key: api_key.to_string(),
            forecast_service,
            api_budget,
        }
    }

    /// Get air quality data for a city
    pub async fn get_air_quality(&self, city: &str) -> Result<AirQualityResponse, AirQualityError> {
        // Reuse the forecast service's geocoding
        let location = self
            .forecast_service
            .geocode(city)
            .await
            .map_err(|_| AirQualityError::CityNotFound(city.to_string()))?;

        tracing::debug!(
            city = %location.name,
            lat = %location.lat,
            lon = %location.lon,
            "Fetching air quality"
        );

        self.api_budget.record_call();

        let response = self
            .client
            .get(AIR_POLLUTION_API_URL)
            .query(&[
                ("lat", location.lat.to_string()),
                ("lon", location.lon.to_string()),
                ("appid", self.api_key.clone()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AirQualityError::ApiError(text));
        }

        let data: AirPollutionResponse = response.json().await?;

        let entry = data.list.first().ok_or(AirQualityError::NoData)?;

        Ok(AirQualityResponse {
            city: location.name,
            aqi: entry.main.aqi,
            aqi_label: aqi_label(entry.main.aqi),
            components: AirQualityComponents {
                co: entry.components.co,
                no: entry.components.no,
                no2: entry.components.no2,
                o3: entry.components.o3,
                so2: entry.components.so2,
                pm2_5: entry.components.pm2_5,
                pm10: entry.components.pm10,
                nh3: entry.components.nh3,
            },
            updated_at: entry.dt,
        })
    }
}
