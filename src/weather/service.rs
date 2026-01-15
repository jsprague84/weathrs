use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const OPENWEATHERMAP_API_URL: &str = "https://api.openweathermap.org/data/2.5/weather";

#[derive(Error, Debug)]
pub enum WeatherError {
    #[error("Failed to fetch weather data: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("City not found: {0}")]
    CityNotFound(String),

    #[error("API error: {0}")]
    ApiError(String),
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
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        }
    }

    pub async fn get_weather(&self, city: &str, units: &str) -> Result<WeatherResponse, WeatherError> {
        let url = format!(
            "{}?q={}&appid={}&units={}",
            OPENWEATHERMAP_API_URL, city, self.api_key, units
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == 404 {
            return Err(WeatherError::CityNotFound(city.to_string()));
        }

        if !response.status().is_success() {
            let error: OpenWeatherMapError = response.json().await?;
            return Err(WeatherError::ApiError(error.message));
        }

        let data: OpenWeatherMapResponse = response.json().await?;

        let weather_info = data.weather.first().ok_or_else(|| {
            WeatherError::ApiError("No weather information available".to_string())
        })?;

        Ok(WeatherResponse {
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
        })
    }
}
