use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::forecast::ForecastService;
use crate::notifications::{NotificationMessage, NotificationService, Priority};

use super::jobs::{ForecastJob, JobConfig};

/// Service for managing scheduled forecast jobs
pub struct SchedulerService {
    scheduler: JobScheduler,
    forecast_service: Arc<ForecastService>,
    notification_service: Arc<NotificationService>,
    jobs: Arc<RwLock<Vec<ForecastJob>>>,
}

impl SchedulerService {
    pub async fn new(
        forecast_service: Arc<ForecastService>,
        notification_service: Arc<NotificationService>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;

        Ok(Self {
            scheduler,
            forecast_service,
            notification_service,
            jobs: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Start the scheduler
    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting scheduler");
        self.scheduler.start().await?;
        Ok(())
    }

    /// Load jobs from configuration
    pub async fn load_jobs(&self, config: &JobConfig) -> Result<()> {
        for job in &config.jobs {
            if job.enabled {
                self.add_job(job.clone()).await?;
            }
        }
        Ok(())
    }

    /// Add a forecast job to the scheduler
    pub async fn add_job(&self, job_config: ForecastJob) -> Result<()> {
        let job_id = job_config.id.clone();
        let job_name = job_config.name.clone();
        let city = job_config.city.clone();
        let units = job_config.units.clone();
        let notify_config = job_config.notify.clone();
        let include_daily = job_config.include_daily;

        let forecast_service = Arc::clone(&self.forecast_service);
        let notification_service = Arc::clone(&self.notification_service);

        tracing::info!(
            job_id = %job_id,
            job_name = %job_name,
            city = %city,
            cron = %job_config.cron,
            "Adding scheduled forecast job"
        );

        let cron_job = Job::new_async(job_config.cron.as_str(), move |_uuid, _lock| {
            let city = city.clone();
            let units = units.clone();
            let job_name = job_name.clone();
            let notify_config = notify_config.clone();
            let forecast_service = Arc::clone(&forecast_service);
            let notification_service = Arc::clone(&notification_service);

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

                        if should_notify && notification_service.is_configured() {
                            let message = build_notification_message(&forecast, &notify_config);
                            if let Err(e) = notification_service.send(&message).await {
                                tracing::error!(error = %e, "Failed to send notification");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(job = %job_name, error = %e, "Failed to fetch forecast");

                        // Notify on error if configured
                        if notification_service.is_configured() {
                            let message = NotificationMessage {
                                title: format!("⚠️ Weather Alert: {} Failed", job_name),
                                body: format!("Failed to fetch forecast for {}: {}", city, e),
                                priority: Priority::High,
                                tags: vec!["warning".to_string()],
                            };
                            let _ = notification_service.send(&message).await;
                        }
                    }
                }
            })
        })?;

        self.scheduler.add(cron_job).await?;

        // Store job config
        self.jobs.write().await.push(job_config);

        Ok(())
    }

    /// Get all configured jobs
    pub async fn get_jobs(&self) -> Vec<ForecastJob> {
        self.jobs.read().await.clone()
    }

    /// Run a job immediately (manual trigger)
    pub async fn run_now(&self, city: &str, units: &str) -> Result<()> {
        tracing::info!(city = %city, "Running manual forecast job");

        let forecast = self
            .forecast_service
            .get_daily_forecast(city, units)
            .await?;

        if self.notification_service.is_configured() {
            let notify_config = super::jobs::NotifyConfig {
                on_run: true,
                ..Default::default()
            };
            let message = build_notification_message(&forecast, &notify_config);
            self.notification_service.send(&message).await?;
        }

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
    _config: &super::jobs::NotifyConfig,
) -> NotificationMessage {
    let city = &forecast.location.city;
    let country = &forecast.location.country;

    // Build summary from current + today's forecast
    let mut body = String::new();

    if let Some(ref current) = forecast.current {
        body.push_str(&format!(
            "🌡️ Now: {:.1}° (feels {:.1}°)\n",
            current.temperature, current.feels_like
        ));
        body.push_str(&format!("☁️ {}\n", current.description));
    }

    if let Some(today) = forecast.daily.first() {
        body.push_str(&format!(
            "📊 Today: {:.0}° - {:.0}°\n",
            today.temp_min, today.temp_max
        ));
        if today.precipitation_probability > 0.0 {
            body.push_str(&format!(
                "🌧️ Rain: {:.0}% chance\n",
                today.precipitation_probability * 100.0
            ));
        }
        if let Some(ref summary) = today.summary {
            body.push_str(&format!("📝 {}", summary));
        }
    }

    // Check for alerts
    let priority = if !forecast.alerts.is_empty() {
        body.push_str("\n\n⚠️ WEATHER ALERTS:\n");
        for alert in &forecast.alerts {
            body.push_str(&format!("• {}\n", alert.event));
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
        title: format!("🌤️ Weather: {}, {}", city, country),
        body,
        priority,
        tags,
    }
}
