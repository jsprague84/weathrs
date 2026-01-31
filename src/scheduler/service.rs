use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobBuilder, JobScheduler};
use uuid::Uuid;

use crate::devices::DevicesService;
use crate::forecast::ForecastService;
use crate::notifications::{NotificationMessage, Priority};

use super::jobs::{ForecastJob, JobConfig};
use super::storage::JobStorage;

#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("Job not found: {0}")]
    NotFound(String),

    #[error("Invalid cron expression: {0}")]
    InvalidCron(String),

    #[error("Invalid timezone: {0}")]
    InvalidTimezone(String),

    #[error("Storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("Scheduler error: {0}")]
    Scheduler(String),
}

/// Service for managing scheduled forecast jobs
pub struct SchedulerService {
    scheduler: JobScheduler,
    forecast_service: Arc<ForecastService>,
    devices_service: Arc<DevicesService>,
    /// Maps our job IDs to scheduler's internal UUIDs
    job_uuids: Arc<RwLock<HashMap<String, Uuid>>>,
    /// Persistent storage for jobs
    storage: JobStorage,
}

impl SchedulerService {
    pub async fn new(
        forecast_service: Arc<ForecastService>,
        devices_service: Arc<DevicesService>,
        storage_path: &str,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;
        let storage = JobStorage::new(storage_path);

        Ok(Self {
            scheduler,
            forecast_service,
            devices_service,
            job_uuids: Arc::new(RwLock::new(HashMap::new())),
            storage,
        })
    }

    /// Initialize storage and load persisted jobs
    pub async fn init(&self) -> Result<(), SchedulerError> {
        self.storage.load().await?;

        // Load all enabled jobs from storage
        let jobs = self.storage.get_enabled().await;
        for job in jobs {
            if let Err(e) = self.schedule_job(&job).await {
                tracing::error!(job_id = %job.id, error = %e, "Failed to schedule job from storage");
            }
        }

        tracing::info!(
            count = self.storage.count().await,
            "Scheduler initialized with stored jobs"
        );
        Ok(())
    }

    /// Start the scheduler
    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting scheduler");
        self.scheduler.start().await?;
        Ok(())
    }

    /// Load jobs from configuration file (merges with stored jobs)
    pub async fn load_jobs(&self, config: &JobConfig) -> Result<()> {
        for job in &config.jobs {
            // Only add if not already in storage
            if !self.storage.exists(&job.id).await {
                if let Err(e) = self.create_job(job.clone()).await {
                    tracing::error!(job_id = %job.id, error = %e, "Failed to load job from config");
                }
            }
        }
        Ok(())
    }

    /// Schedule a job in the cron scheduler (internal)
    async fn schedule_job(&self, job_config: &ForecastJob) -> Result<Uuid, SchedulerError> {
        let job_id = job_config.id.clone();
        let job_name = job_config.name.clone();
        let city = job_config.city.clone();
        let units = job_config.units.clone();
        let notify_config = job_config.notify.clone();
        let include_daily = job_config.include_daily;

        let forecast_service = Arc::clone(&self.forecast_service);
        let devices_service = Arc::clone(&self.devices_service);

        // Parse the timezone string to chrono_tz::Tz
        let timezone: chrono_tz::Tz = job_config
            .timezone
            .parse()
            .map_err(|_| SchedulerError::InvalidTimezone(job_config.timezone.clone()))?;

        tracing::info!(
            job_id = %job_id,
            job_name = %job_name,
            city = %city,
            cron = %job_config.cron,
            timezone = %job_config.timezone,
            "Scheduling forecast job"
        );

        let cron_job = JobBuilder::new()
            .with_timezone(timezone)
            .with_cron_job_type()
            .with_schedule(&job_config.cron)
            .map_err(|e| SchedulerError::InvalidCron(e.to_string()))?
            .with_run_async(Box::new(move |_uuid, _lock| {
                let city = city.clone();
                let units = units.clone();
                let job_name = job_name.clone();
                let notify_config = notify_config.clone();
                let forecast_service = Arc::clone(&forecast_service);
                let devices_service = Arc::clone(&devices_service);

                Box::pin(async move {
                    tracing::info!(job = %job_name, city = %city, "Running scheduled forecast job");

                    // Fetch forecast
                    let forecast_result = if include_daily {
                        forecast_service.get_daily_forecast(&city, &units).await
                    } else {
                        forecast_service.get_forecast(&city, &units).await
                    };

                    match forecast_result {
                        Ok(forecast) => {
                            tracing::info!(
                                job = %job_name,
                                city = %forecast.location.city,
                                "Forecast fetched successfully"
                            );

                            // Check if we should send notification
                            let should_notify = should_notify_for_forecast(&forecast, &notify_config);

                            if should_notify {
                                let message = build_notification_message(&forecast);

                                // Send to Expo push (devices subscribed to this city)
                                match devices_service.send_to_city(&city, &message).await {
                                    Ok(count) => {
                                        tracing::info!(city = %city, count = count, "Sent push notifications");
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "Failed to send push notifications");
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(job = %job_name, error = %e, "Failed to fetch forecast");

                            // Send error notification to devices subscribed to this city
                            let message = NotificationMessage {
                                title: format!("Weather Alert: {} Failed", job_name),
                                body: format!("Failed to fetch forecast for {}: {}", city, e),
                                priority: Priority::High,
                                tags: vec!["warning".to_string()],
                                city: Some(city.clone()),
                            };

                            let _ = devices_service.send_to_city(&city, &message).await;
                        }
                    }
                })
            }))
            .build()
            .map_err(|e| SchedulerError::Scheduler(e.to_string()))?;

        let uuid = self
            .scheduler
            .add(cron_job)
            .await
            .map_err(|e| SchedulerError::Scheduler(e.to_string()))?;

        // Store mapping
        self.job_uuids.write().await.insert(job_id, uuid);

        Ok(uuid)
    }

    /// Unschedule a job from the cron scheduler (internal)
    async fn unschedule_job(&self, job_id: &str) -> Result<(), SchedulerError> {
        let uuid = {
            let uuids = self.job_uuids.read().await;
            uuids.get(job_id).copied()
        };

        if let Some(uuid) = uuid {
            self.scheduler
                .remove(&uuid)
                .await
                .map_err(|e| SchedulerError::Scheduler(e.to_string()))?;
            self.job_uuids.write().await.remove(job_id);
            tracing::info!(job_id = %job_id, "Unscheduled job");
        }

        Ok(())
    }

    /// Create a new job
    pub async fn create_job(&self, job: ForecastJob) -> Result<ForecastJob, SchedulerError> {
        // Validate cron expression by trying to parse it
        if Job::new_async(job.cron.as_str(), |_, _| Box::pin(async {})).is_err() {
            return Err(SchedulerError::InvalidCron(job.cron.clone()));
        }

        // Validate timezone
        if job.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(SchedulerError::InvalidTimezone(job.timezone.clone()));
        }

        // Save to storage
        self.storage.upsert(job.clone()).await?;

        // Schedule if enabled
        if job.enabled {
            self.schedule_job(&job).await?;
        }

        tracing::info!(job_id = %job.id, name = %job.name, "Created new job");
        Ok(job)
    }

    /// Update an existing job
    pub async fn update_job(&self, job: ForecastJob) -> Result<ForecastJob, SchedulerError> {
        // Check if job exists
        if !self.storage.exists(&job.id).await {
            return Err(SchedulerError::NotFound(job.id.clone()));
        }

        // Validate cron expression
        if Job::new_async(job.cron.as_str(), |_, _| Box::pin(async {})).is_err() {
            return Err(SchedulerError::InvalidCron(job.cron.clone()));
        }

        // Validate timezone
        if job.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(SchedulerError::InvalidTimezone(job.timezone.clone()));
        }

        // Unschedule old job
        self.unschedule_job(&job.id).await?;

        // Save updated job
        self.storage.upsert(job.clone()).await?;

        // Reschedule if enabled
        if job.enabled {
            self.schedule_job(&job).await?;
        }

        tracing::info!(job_id = %job.id, name = %job.name, "Updated job");
        Ok(job)
    }

    /// Delete a job
    pub async fn delete_job(&self, job_id: &str) -> Result<bool, SchedulerError> {
        // Unschedule first
        self.unschedule_job(job_id).await?;

        // Remove from storage
        let removed = self.storage.remove(job_id).await?;

        if removed {
            tracing::info!(job_id = %job_id, "Deleted job");
        }

        Ok(removed)
    }

    /// Get a job by ID
    pub async fn get_job(&self, job_id: &str) -> Option<ForecastJob> {
        self.storage.get(job_id).await
    }

    /// Get all configured jobs
    pub async fn get_jobs(&self) -> Vec<ForecastJob> {
        self.storage.get_all().await
    }

    /// Add a raw cron `Job` to the scheduler (used by system jobs like backfill).
    pub async fn add_system_job(&self, job: Job) -> Result<Uuid, SchedulerError> {
        let uuid = self
            .scheduler
            .add(job)
            .await
            .map_err(|e| SchedulerError::Scheduler(e.to_string()))?;
        Ok(uuid)
    }

    /// Run a job immediately (manual trigger) - sends to all devices subscribed to the city
    pub async fn run_now(&self, city: &str, units: &str) -> Result<()> {
        tracing::info!(city = %city, "Running manual forecast job");

        let forecast = self
            .forecast_service
            .get_daily_forecast(city, units)
            .await?;

        let message = build_notification_message(&forecast);
        self.devices_service.send_to_city(city, &message).await?;

        Ok(())
    }
}

fn should_notify_for_forecast(
    forecast: &crate::forecast::models::ForecastResponse,
    config: &super::jobs::NotifyConfig,
) -> bool {
    // Always notify if on_run is true
    if config.on_run {
        return true;
    }

    // Check for weather alerts
    if config.on_alert && !forecast.alerts.is_empty() {
        return true;
    }

    // Check for precipitation in forecast
    if config.on_precipitation {
        for daily in &forecast.daily {
            if daily.precipitation_probability > 0.5 {
                return true;
            }
        }
    }

    // Check temperature thresholds
    if let Some(ref current) = forecast.current {
        if let Some(cold) = config.cold_threshold {
            if current.temperature < cold {
                return true;
            }
        }
        if let Some(heat) = config.heat_threshold {
            if current.temperature > heat {
                return true;
            }
        }
    }

    false
}

fn build_notification_message(
    forecast: &crate::forecast::models::ForecastResponse,
) -> NotificationMessage {
    let city = &forecast.location.city;
    let country = &forecast.location.country;

    // Build summary from current + today's forecast
    let mut body = String::new();

    if let Some(ref current) = forecast.current {
        body.push_str(&format!(
            "Now: {:.1} (feels {:.1})\n",
            current.temperature, current.feels_like
        ));
        body.push_str(&format!("{}\n", current.description));
    }

    if let Some(today) = forecast.daily.first() {
        body.push_str(&format!(
            "Today: {:.0} - {:.0}\n",
            today.temp_min, today.temp_max
        ));
        if today.precipitation_probability > 0.0 {
            body.push_str(&format!(
                "Rain: {:.0}% chance\n",
                today.precipitation_probability * 100.0
            ));
        }
        if let Some(ref summary) = today.summary {
            body.push_str(summary);
        }
    }

    // Check for alerts
    let priority = if !forecast.alerts.is_empty() {
        body.push_str("\n\nWEATHER ALERTS:\n");
        for alert in &forecast.alerts {
            body.push_str(&format!("- {}\n", alert.event));
        }
        Priority::Urgent
    } else {
        Priority::Default
    };

    let tags = if !forecast.alerts.is_empty() {
        vec!["warning".to_string(), "weather".to_string()]
    } else {
        vec!["sunny".to_string(), "weather".to_string()]
    };

    NotificationMessage {
        title: format!("Weather: {}, {}", city, country),
        body,
        priority,
        tags,
        city: Some(city.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forecast::models::*;

    fn create_test_forecast(
        current_temp: Option<f64>,
        alerts: Vec<AlertResponse>,
        daily_precip_prob: f64,
    ) -> ForecastResponse {
        ForecastResponse {
            location: LocationInfo {
                city: "Chicago".to_string(),
                country: "US".to_string(),
                state: Some("Illinois".to_string()),
                lat: 41.8781,
                lon: -87.6298,
            },
            timezone: "America/Chicago".to_string(),
            current: current_temp.map(|temp| CurrentWeatherResponse {
                timestamp: 1700000000,
                temperature: temp,
                feels_like: temp - 1.0,
                humidity: 65,
                pressure: 1013,
                uv_index: 3.5,
                clouds: 40,
                visibility: Some(10000),
                wind_speed: 5.5,
                wind_direction: 180,
                wind_gust: None,
                description: "clear sky".to_string(),
                icon: "01d".to_string(),
                sunrise: Some(1699980000),
                sunset: Some(1700020000),
            }),
            hourly: vec![],
            daily: vec![DailyForecastResponse {
                timestamp: 1700000000,
                sunrise: 1699980000,
                sunset: 1700020000,
                moon_phase: 0.5,
                summary: Some("Clear skies".to_string()),
                temp_min: 15.0,
                temp_max: 25.0,
                temp_day: 22.0,
                temp_night: 16.0,
                temp_morning: 18.0,
                temp_evening: 20.0,
                feels_like_day: 21.0,
                feels_like_night: 15.0,
                humidity: 60,
                pressure: 1013,
                uv_index: 5.0,
                clouds: 20,
                wind_speed: 4.0,
                wind_direction: 180,
                precipitation_probability: daily_precip_prob,
                rain_volume: None,
                snow_volume: None,
                description: "clear sky".to_string(),
                icon: "01d".to_string(),
            }],
            alerts,
        }
    }

    fn create_default_notify_config() -> super::super::jobs::NotifyConfig {
        super::super::jobs::NotifyConfig {
            on_run: false,
            on_alert: false,
            on_precipitation: false,
            cold_threshold: None,
            heat_threshold: None,
        }
    }

    #[test]
    fn test_should_notify_on_run_true() {
        let forecast = create_test_forecast(Some(20.0), vec![], 0.0);
        let mut config = create_default_notify_config();
        config.on_run = true;

        assert!(should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_on_run_false() {
        let forecast = create_test_forecast(Some(20.0), vec![], 0.0);
        let config = create_default_notify_config();

        assert!(!should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_on_alert_with_alerts() {
        let alerts = vec![AlertResponse {
            sender: "NWS".to_string(),
            event: "Heat Advisory".to_string(),
            start: 1700000000,
            end: 1700100000,
            description: "Heat warning".to_string(),
            tags: None,
        }];
        let forecast = create_test_forecast(Some(35.0), alerts, 0.0);
        let mut config = create_default_notify_config();
        config.on_alert = true;

        assert!(should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_on_alert_without_alerts() {
        let forecast = create_test_forecast(Some(20.0), vec![], 0.0);
        let mut config = create_default_notify_config();
        config.on_alert = true;

        assert!(!should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_on_precipitation_high_chance() {
        let forecast = create_test_forecast(Some(20.0), vec![], 0.75);
        let mut config = create_default_notify_config();
        config.on_precipitation = true;

        assert!(should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_on_precipitation_low_chance() {
        let forecast = create_test_forecast(Some(20.0), vec![], 0.3);
        let mut config = create_default_notify_config();
        config.on_precipitation = true;

        assert!(!should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_cold_threshold_triggered() {
        let forecast = create_test_forecast(Some(-5.0), vec![], 0.0);
        let mut config = create_default_notify_config();
        config.cold_threshold = Some(0.0);

        assert!(should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_cold_threshold_not_triggered() {
        let forecast = create_test_forecast(Some(10.0), vec![], 0.0);
        let mut config = create_default_notify_config();
        config.cold_threshold = Some(0.0);

        assert!(!should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_heat_threshold_triggered() {
        let forecast = create_test_forecast(Some(38.0), vec![], 0.0);
        let mut config = create_default_notify_config();
        config.heat_threshold = Some(35.0);

        assert!(should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_heat_threshold_not_triggered() {
        let forecast = create_test_forecast(Some(25.0), vec![], 0.0);
        let mut config = create_default_notify_config();
        config.heat_threshold = Some(35.0);

        assert!(!should_notify_for_forecast(&forecast, &config));
    }

    #[test]
    fn test_should_notify_no_current_weather() {
        let forecast = create_test_forecast(None, vec![], 0.0);
        let mut config = create_default_notify_config();
        config.cold_threshold = Some(0.0);

        // Should not notify since there's no current weather to check
        assert!(!should_notify_for_forecast(&forecast, &config));
    }
}
