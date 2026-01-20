use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Configuration for a scheduled forecast job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastJob {
    /// Unique job identifier
    pub id: String,
    /// Job name for display
    pub name: String,
    /// City to fetch forecast for
    pub city: String,
    /// Units (metric, imperial, standard)
    #[serde(default = "default_units")]
    pub units: String,
    /// Cron expression (e.g., "0 30 5 * * *" for 5:30am daily)
    pub cron: String,
    /// IANA timezone (e.g., "America/Chicago"). Defaults to UTC if not specified.
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// Whether to include daily forecast
    #[serde(default = "default_true")]
    pub include_daily: bool,
    /// Whether to include hourly forecast
    #[serde(default)]
    pub include_hourly: bool,
    /// Whether this job is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Notification settings
    #[serde(default)]
    pub notify: NotifyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotifyConfig {
    /// Send notification on every run
    #[serde(default = "default_true")]
    pub on_run: bool,
    /// Send notification only on weather alerts
    #[serde(default = "default_true")]
    pub on_alert: bool,
    /// Send notification on rain/snow in forecast
    #[serde(default)]
    pub on_precipitation: bool,
    /// Temperature threshold for cold alerts (send if below)
    pub cold_threshold: Option<f64>,
    /// Temperature threshold for heat alerts (send if above)
    pub heat_threshold: Option<f64>,
}

fn default_units() -> String {
    "metric".to_string()
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_true() -> bool {
    true
}

impl ForecastJob {
    pub fn new(name: &str, city: &str, cron: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            city: city.to_string(),
            units: default_units(),
            cron: cron.to_string(),
            timezone: default_timezone(),
            include_daily: true,
            include_hourly: false,
            enabled: true,
            notify: NotifyConfig::default(),
        }
    }

    pub fn with_timezone(mut self, timezone: &str) -> Self {
        self.timezone = timezone.to_string();
        self
    }
}

/// Full job configuration from config file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobConfig {
    /// List of scheduled forecast jobs
    #[serde(default)]
    pub jobs: Vec<ForecastJob>,
}
