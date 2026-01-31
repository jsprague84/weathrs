use std::sync::Arc;

use indexmap::IndexSet;
use tokio_cron_scheduler::Job;

use crate::api_budget::ApiCallBudget;
use crate::config::HistoryBackfillConfig;
use crate::devices::DevicesService;
use crate::history::HistoryService;
use crate::scheduler::{SchedulerError, SchedulerService};

/// Build a deduplicated, priority-ordered list of cities to backfill.
///
/// Priority order:
/// 1. First city from each enabled device (= "my location")
/// 2. Remaining device cities
/// 3. Cities from enabled scheduler jobs
/// 4. Fallback cities from config
fn build_city_list(
    devices: &[crate::devices::models::Device],
    jobs: &[crate::scheduler::ForecastJob],
    fallback: &[String],
) -> Vec<String> {
    let mut set = IndexSet::new();

    // 1) First city from each enabled device
    for device in devices.iter().filter(|d| d.enabled) {
        if let Some(first) = device.cities.first() {
            set.insert(first.clone());
        }
    }

    // 2) Remaining cities from enabled devices
    for device in devices.iter().filter(|d| d.enabled) {
        for city in &device.cities {
            set.insert(city.clone());
        }
    }

    // 3) Cities from enabled scheduler jobs
    for job in jobs.iter().filter(|j| j.enabled) {
        set.insert(job.city.clone());
    }

    // 4) Fallback cities
    for city in fallback {
        set.insert(city.clone());
    }

    set.into_iter().collect()
}

/// Run the backfill: iterate cities, fetch missing days until budget is exhausted.
async fn run_backfill(
    history_service: &HistoryService,
    devices_service: &DevicesService,
    scheduler_service: &SchedulerService,
    config: &HistoryBackfillConfig,
    budget: &ApiCallBudget,
) {
    let devices = devices_service.get_all().await;
    let jobs = scheduler_service.get_jobs().await;
    let cities = build_city_list(&devices, &jobs, &config.fallback_cities);

    if cities.is_empty() {
        tracing::info!("Backfill: no cities configured, skipping");
        return;
    }

    let now = chrono::Utc::now().timestamp();
    let years_secs = config.max_years as i64 * 365 * 86400;
    let start_ts = now - years_secs;
    let end_ts = now;

    tracing::info!(
        cities = cities.len(),
        budget_remaining = budget.remaining(),
        max_years = config.max_years,
        "Starting history backfill"
    );

    let mut total_inserted: usize = 0;
    let units = "metric";

    for city in &cities {
        if budget.remaining() == 0 {
            tracing::info!("Backfill: daily budget exhausted");
            break;
        }

        // Geocode the city
        let location = match history_service.geocode(city).await {
            Ok(loc) => loc,
            Err(e) => {
                tracing::warn!(city = %city, error = %e, "Backfill: failed to geocode, skipping");
                continue;
            }
        };
        let city_name = location.name.clone();

        // Get missing days
        let missing_days = match history_service
            .get_missing_days(&city_name, start_ts, end_ts, units)
            .await
        {
            Ok(days) => days,
            Err(e) => {
                tracing::warn!(city = %city_name, error = %e, "Backfill: failed to get missing days");
                continue;
            }
        };

        if missing_days.is_empty() {
            tracing::debug!(city = %city_name, "Backfill: city fully cached");
            continue;
        }

        tracing::info!(
            city = %city_name,
            missing = missing_days.len(),
            budget_remaining = budget.remaining(),
            "Backfill: fetching missing days"
        );

        let mut city_inserted: usize = 0;

        for day_ts in &missing_days {
            match history_service
                .fetch_day_if_budget(&city_name, &location, *day_ts, units)
                .await
            {
                Ok(Some(count)) => {
                    city_inserted += count;
                }
                Ok(None) => {
                    tracing::info!(city = %city_name, "Backfill: budget exhausted mid-city");
                    break;
                }
                Err(e) => {
                    tracing::warn!(
                        city = %city_name,
                        day_ts = day_ts,
                        error = %e,
                        "Backfill: failed to fetch day, skipping"
                    );
                }
            }

            // 100ms delay between API calls to avoid throttling
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        total_inserted += city_inserted;
        tracing::info!(
            city = %city_name,
            inserted = city_inserted,
            "Backfill: city complete"
        );
    }

    tracing::info!(
        total_inserted = total_inserted,
        budget_used = budget.used_today(),
        "Backfill run finished"
    );
}

/// Register the backfill cron job on the scheduler.
pub async fn schedule_backfill_job(
    scheduler_service: Arc<SchedulerService>,
    history_service: Arc<HistoryService>,
    devices_service: Arc<DevicesService>,
    config: HistoryBackfillConfig,
    budget: Arc<ApiCallBudget>,
) -> Result<(), SchedulerError> {
    let cron = config.cron.clone();

    tracing::info!(cron = %cron, "Scheduling history backfill job");

    // Clone the Arc before moving into the closure so we can still call add_system_job
    let scheduler_for_closure = Arc::clone(&scheduler_service);

    let job = Job::new_async(cron.as_str(), move |_uuid, _lock| {
        let history_service = Arc::clone(&history_service);
        let devices_service = Arc::clone(&devices_service);
        let scheduler_service = Arc::clone(&scheduler_for_closure);
        let config = config.clone();
        let budget = Arc::clone(&budget);

        Box::pin(async move {
            tracing::info!("Backfill job triggered");
            run_backfill(
                &history_service,
                &devices_service,
                &scheduler_service,
                &config,
                &budget,
            )
            .await;
        })
    })
    .map_err(|e| SchedulerError::Scheduler(e.to_string()))?;

    scheduler_service.add_system_job(job).await?;

    tracing::info!("History backfill job scheduled");
    Ok(())
}
