use config::{Case, Config, ConfigError, Environment, File};
use serde::Deserialize;

use crate::scheduler::ForecastJob;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// Server host address
    #[serde(default = "default_host")]
    pub host: String,

    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,

    /// OpenWeatherMap API key
    pub openweathermap_api_key: String,

    /// Default city for weather queries
    #[serde(default = "default_city")]
    pub default_city: String,

    /// Temperature units: metric, imperial, or standard
    #[serde(default = "default_units")]
    pub units: String,

    /// API key for device endpoints (optional - if not set, no auth required)
    #[serde(default)]
    pub device_api_key: Option<String>,

    /// Database URL (SQLite connection string)
    #[serde(default = "default_database_url")]
    pub database_url: String,

    /// Display configuration
    #[serde(default)]
    pub display: DisplayConfig,

    /// Scheduled jobs configuration
    #[serde(default)]
    pub scheduler: SchedulerConfig,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SchedulerConfig {
    /// Whether scheduler is enabled
    #[serde(default)]
    pub enabled: bool,

    /// List of scheduled forecast jobs
    #[serde(default)]
    pub jobs: Vec<ForecastJob>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DisplayConfig {
    /// Show temperature
    #[serde(default = "default_true")]
    pub temperature: bool,

    /// Show humidity
    #[serde(default = "default_true")]
    pub humidity: bool,

    /// Show wind speed
    #[serde(default = "default_true")]
    pub wind_speed: bool,

    /// Show weather description
    #[serde(default = "default_true")]
    pub description: bool,

    /// Show feels-like temperature
    #[serde(default = "default_true")]
    pub feels_like: bool,

    /// Show pressure
    #[serde(default = "default_false")]
    pub pressure: bool,

    /// Show visibility
    #[serde(default = "default_false")]
    pub visibility: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            temperature: true,
            humidity: true,
            wind_speed: true,
            description: true,
            feels_like: true,
            pressure: false,
            visibility: false,
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_city() -> String {
    "London".to_string()
}

fn default_units() -> String {
    "metric".to_string()
}

fn default_database_url() -> String {
    "sqlite:data/weathrs.db".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        // Load .env file if present
        let _ = dotenvy::dotenv();

        let config = Config::builder()
            // Start with default values
            .set_default("host", default_host())?
            .set_default("port", default_port())?
            .set_default("default_city", default_city())?
            .set_default("units", default_units())?
            // Load from config file if present
            .add_source(File::with_name("config").required(false))
            .add_source(File::with_name("config.local").required(false))
            // Override with environment variables (prefixed with WEATHRS_)
            // Convert SCREAMING_SNAKE_CASE env vars to snake_case config keys
            .add_source(
                Environment::with_prefix("WEATHRS")
                    .prefix_separator("_")
                    .separator("__")
                    .convert_case(Case::Snake)
                    .try_parsing(true),
            )
            .build()?;

        config.try_deserialize()
    }
}
