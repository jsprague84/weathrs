use axum::http::StatusCode;
use reqwest::Client;
use sqlx::SqlitePool;
use thiserror::Error;

use super::models::*;
use crate::cache::{normalize_cache_key, CachedGeoLocation, GeoCache};
use crate::db::history_repo::{HistoryRecord, HistoryRepository, SqliteHistoryRepository};
use crate::error::HttpError;
use crate::forecast::models::GeoLocation;
use crate::impl_into_response;

const GEOCODING_API_URL: &str = "https://api.openweathermap.org/geo/1.0/direct";
const ZIP_GEOCODING_API_URL: &str = "https://api.openweathermap.org/geo/1.0/zip";
const TIMEMACHINE_API_URL: &str = "https://api.openweathermap.org/data/3.0/onecall/timemachine";

/// Maximum number of API calls (days) per request to avoid OWM throttling.
/// Set to 90 to cover the maximum supported period (90d) in a single request.
const MAX_DAYS_PER_REQUEST: usize = 90;

/// Default history range: 7 days
const DEFAULT_RANGE_DAYS: i64 = 7;

#[derive(Error, Debug)]
pub enum HistoryError {
    #[error("Failed to fetch data: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("City not found: {0}")]
    CityNotFound(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Invalid date range: {0}")]
    InvalidDateRange(String),

    #[error("One Call API subscription required")]
    SubscriptionRequired,
}

impl HttpError for HistoryError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::CityNotFound(_) => StatusCode::NOT_FOUND,
            Self::RequestError(_) => StatusCode::BAD_GATEWAY,
            Self::ApiError(_) => StatusCode::BAD_REQUEST,
            Self::DatabaseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::InvalidDateRange(_) => StatusCode::BAD_REQUEST,
            Self::SubscriptionRequired => StatusCode::PAYMENT_REQUIRED,
        }
    }

    fn error_code(&self) -> Option<&'static str> {
        match self {
            Self::CityNotFound(_) => Some("CITY_NOT_FOUND"),
            Self::RequestError(_) => Some("REQUEST_ERROR"),
            Self::ApiError(_) => Some("API_ERROR"),
            Self::DatabaseError(_) => Some("DATABASE_ERROR"),
            Self::InvalidDateRange(_) => Some("INVALID_DATE_RANGE"),
            Self::SubscriptionRequired => Some("SUBSCRIPTION_REQUIRED"),
        }
    }
}

impl_into_response!(HistoryError);

pub struct HistoryService {
    client: Client,
    api_key: String,
    geo_cache: GeoCache,
    repo: SqliteHistoryRepository,
}

impl HistoryService {
    pub fn new(client: Client, api_key: &str, geo_cache: GeoCache, pool: SqlitePool) -> Self {
        Self {
            client,
            api_key: api_key.to_string(),
            geo_cache,
            repo: SqliteHistoryRepository::new(pool),
        }
    }

    /// Geocode a location string to coordinates (reuses ForecastService pattern)
    async fn geocode(&self, location: &str) -> Result<GeoLocation, HistoryError> {
        let cache_key = normalize_cache_key(location);

        if let Some(cached) = self.geo_cache.get(&cache_key) {
            return Ok(GeoLocation {
                name: cached.name,
                lat: cached.lat,
                lon: cached.lon,
                country: cached.country,
                state: cached.state,
            });
        }

        let result = if Self::is_zip_code(location) {
            self.geocode_zip(location).await
        } else {
            self.geocode_city(location).await
        }?;

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

    fn is_zip_code(input: &str) -> bool {
        let parts: Vec<&str> = input.split(',').collect();
        match parts.as_slice() {
            [zip] => zip.trim().chars().all(|c| c.is_ascii_digit()),
            [zip, _country] => zip.trim().chars().all(|c| c.is_ascii_digit()),
            _ => false,
        }
    }

    async fn geocode_city(&self, city: &str) -> Result<GeoLocation, HistoryError> {
        let response = self
            .client
            .get(GEOCODING_API_URL)
            .query(&[("q", city), ("limit", "1"), ("appid", &self.api_key)])
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(HistoryError::ApiError(format!(
                "Geocoding failed: {}",
                text
            )));
        }

        let locations: Vec<GeoLocation> = response.json().await?;
        locations
            .into_iter()
            .next()
            .ok_or_else(|| HistoryError::CityNotFound(city.to_string()))
    }

    async fn geocode_zip(&self, zip: &str) -> Result<GeoLocation, HistoryError> {
        let zip_query = if zip.contains(',') {
            zip.to_string()
        } else {
            format!("{},US", zip)
        };

        let response = self
            .client
            .get(ZIP_GEOCODING_API_URL)
            .query(&[("zip", &zip_query), ("appid", &self.api_key)])
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(HistoryError::ApiError(format!(
                "Zip geocoding failed: {}",
                text
            )));
        }

        let location: crate::forecast::models::ZipGeoLocation = response.json().await?;
        Ok(location.into())
    }

    /// Get hourly history data for a city within a time range
    pub async fn get_history(
        &self,
        city: &str,
        start: Option<i64>,
        end: Option<i64>,
        units: &str,
    ) -> Result<HistoryResponse, HistoryError> {
        let now = chrono::Utc::now().timestamp();
        let end_ts = end.unwrap_or(now);
        let start_ts = start.unwrap_or(end_ts - DEFAULT_RANGE_DAYS * 86400);

        if start_ts >= end_ts {
            return Err(HistoryError::InvalidDateRange(
                "start must be before end".to_string(),
            ));
        }

        let location = self.geocode(city).await?;
        let city_name = location.name.clone();

        // Fetch missing data from OWM and store in DB
        self.backfill_data(&city_name, &location, start_ts, end_ts, units)
            .await?;

        // Query all data from DB
        let records = self
            .repo
            .get_range(&city_name, start_ts, end_ts, units)
            .await
            .map_err(|e| HistoryError::DatabaseError(sqlx::Error::Protocol(e.to_string())))?;

        let data_points = records
            .into_iter()
            .map(|r| HistoryDataPoint {
                timestamp: r.timestamp,
                temperature: r.temperature,
                feels_like: r.feels_like,
                humidity: r.humidity,
                pressure: r.pressure,
                wind_speed: r.wind_speed,
                wind_direction: r.wind_direction,
                clouds: r.clouds,
                visibility: r.visibility,
                description: r.description,
                icon: r.icon,
                rain_1h: r.rain_1h,
                snow_1h: r.snow_1h,
            })
            .collect();

        let period = format_period(start_ts, end_ts);

        Ok(HistoryResponse {
            city: city_name,
            units: units.to_string(),
            period,
            data_points,
        })
    }

    /// Get daily aggregated history for a city
    pub async fn get_daily_history(
        &self,
        city: &str,
        start: Option<i64>,
        end: Option<i64>,
        units: &str,
    ) -> Result<DailyHistoryResponse, HistoryError> {
        let now = chrono::Utc::now().timestamp();
        let end_ts = end.unwrap_or(now);
        let start_ts = start.unwrap_or(end_ts - DEFAULT_RANGE_DAYS * 86400);

        if start_ts >= end_ts {
            return Err(HistoryError::InvalidDateRange(
                "start must be before end".to_string(),
            ));
        }

        let location = self.geocode(city).await?;
        let city_name = location.name.clone();

        self.backfill_data(&city_name, &location, start_ts, end_ts, units)
            .await?;

        let summaries = self
            .repo
            .get_daily_summary(&city_name, start_ts, end_ts, units)
            .await
            .map_err(|e| HistoryError::DatabaseError(sqlx::Error::Protocol(e.to_string())))?;

        let days = summaries
            .into_iter()
            .map(|s| DailyHistorySummary {
                date: s.date,
                temp_min: s.temp_min,
                temp_max: s.temp_max,
                temp_avg: round_2(s.temp_avg),
                humidity_avg: round_2(s.humidity_avg),
                wind_speed_avg: round_2(s.wind_speed_avg),
                precipitation_total: round_2(s.precipitation_total),
                dominant_condition: s.dominant_condition,
            })
            .collect();

        let period = format_period(start_ts, end_ts);

        Ok(DailyHistoryResponse {
            city: city_name,
            units: units.to_string(),
            period,
            days,
        })
    }

    /// Get weather trends with summary statistics
    pub async fn get_trends(
        &self,
        city: &str,
        period: &str,
        units: &str,
        custom_start: Option<i64>,
        custom_end: Option<i64>,
    ) -> Result<TrendResponse, HistoryError> {
        let now = chrono::Utc::now().timestamp();

        let (start_ts, end_ts) = if let (Some(s), Some(e)) = (custom_start, custom_end) {
            if s >= e {
                return Err(HistoryError::InvalidDateRange(
                    "start must be before end".to_string(),
                ));
            }
            let range_days = (e - s) / 86400;
            if range_days > 365 {
                return Err(HistoryError::InvalidDateRange(
                    "custom range cannot exceed 365 days".to_string(),
                ));
            }
            (s, e)
        } else {
            let days = match period {
                "7d" => 7,
                "30d" => 30,
                "90d" => 90,
                _ => {
                    return Err(HistoryError::InvalidDateRange(
                        "period must be 7d, 30d, or 90d".to_string(),
                    ))
                }
            };
            (now - days * 86400, now)
        };

        let location = self.geocode(city).await?;
        let city_name = location.name.clone();

        self.backfill_data(&city_name, &location, start_ts, end_ts, units)
            .await?;

        let summaries = self
            .repo
            .get_daily_summary(&city_name, start_ts, end_ts, units)
            .await
            .map_err(|e| HistoryError::DatabaseError(sqlx::Error::Protocol(e.to_string())))?;

        let daily_summaries: Vec<DailyHistorySummary> = summaries
            .into_iter()
            .map(|s| DailyHistorySummary {
                date: s.date,
                temp_min: s.temp_min,
                temp_max: s.temp_max,
                temp_avg: round_2(s.temp_avg),
                humidity_avg: round_2(s.humidity_avg),
                wind_speed_avg: round_2(s.wind_speed_avg),
                precipitation_total: round_2(s.precipitation_total),
                dominant_condition: s.dominant_condition,
            })
            .collect();

        let summary = compute_trend_summary(&daily_summaries);

        Ok(TrendResponse {
            city: city_name,
            units: units.to_string(),
            period: period.to_string(),
            days: daily_summaries,
            summary,
        })
    }

    /// Fetch missing data from OWM Timemachine API and store in DB.
    /// OWM Timemachine returns all hourly data for a given UTC day, so we
    /// identify which days are missing and fetch one API call per day.
    async fn backfill_data(
        &self,
        city: &str,
        location: &GeoLocation,
        start_ts: i64,
        end_ts: i64,
        units: &str,
    ) -> Result<(), HistoryError> {
        let missing_days = self
            .repo
            .get_missing_days(city, start_ts, end_ts, units)
            .await
            .map_err(|e| HistoryError::DatabaseError(sqlx::Error::Protocol(e.to_string())))?;

        if missing_days.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            city = %city,
            missing_days = missing_days.len(),
            "Backfilling history data"
        );

        // Limit API calls per request (one call per day)
        let days_to_fetch: Vec<i64> = missing_days
            .into_iter()
            .take(MAX_DAYS_PER_REQUEST)
            .collect();

        let mut records = Vec::new();
        let now = chrono::Utc::now().timestamp();

        for day_ts in &days_to_fetch {
            // Use noon UTC for the API call to ensure we get the right day
            let fetch_ts = day_ts + 12 * 3600;
            match self
                .fetch_timemachine(location.lat, location.lon, fetch_ts, units)
                .await
            {
                Ok(data_points) => {
                    for dp in data_points {
                        records.push(HistoryRecord {
                            city: city.to_string(),
                            lat: location.lat,
                            lon: location.lon,
                            timestamp: dp.dt,
                            temperature: dp.temp,
                            feels_like: dp.feels_like,
                            humidity: dp.humidity as i32,
                            pressure: dp.pressure as i32,
                            wind_speed: dp.wind_speed,
                            wind_direction: dp.wind_deg.map(|d| d as i32),
                            clouds: dp.clouds.map(|c| c as i32),
                            visibility: dp.visibility.map(|v| v as i32),
                            description: dp.weather.first().map(|w| w.description.clone()),
                            icon: dp.weather.first().map(|w| w.icon.clone()),
                            rain_1h: dp.rain.and_then(|r| r.one_hour),
                            snow_1h: dp.snow.and_then(|s| s.one_hour),
                            units: units.to_string(),
                            fetched_at: now,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        city = %city,
                        day_ts = day_ts,
                        error = %e,
                        "Failed to fetch timemachine data for day, skipping"
                    );
                }
            }
        }

        if !records.is_empty() {
            let inserted =
                self.repo.insert_batch(&records).await.map_err(|e| {
                    HistoryError::DatabaseError(sqlx::Error::Protocol(e.to_string()))
                })?;

            tracing::debug!(
                city = %city,
                fetched = records.len(),
                inserted = inserted,
                "Backfill complete"
            );
        }

        Ok(())
    }

    /// Fetch a single timemachine data point from OWM
    async fn fetch_timemachine(
        &self,
        lat: f64,
        lon: f64,
        timestamp: i64,
        units: &str,
    ) -> Result<Vec<TimemachineData>, HistoryError> {
        let response = self
            .client
            .get(TIMEMACHINE_API_URL)
            .query(&[
                ("lat", lat.to_string()),
                ("lon", lon.to_string()),
                ("dt", timestamp.to_string()),
                ("units", units.to_string()),
                ("appid", self.api_key.clone()),
            ])
            .send()
            .await?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(HistoryError::SubscriptionRequired);
        }

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(HistoryError::ApiError(text));
        }

        let data: TimemachineResponse = response.json().await?;
        Ok(data.data)
    }
}

/// Compute trend summary from daily summaries
fn compute_trend_summary(days: &[DailyHistorySummary]) -> TrendSummary {
    if days.is_empty() {
        return TrendSummary {
            avg_temp: 0.0,
            temp_trend: "stable".to_string(),
            max_temp: TrendExtreme {
                value: 0.0,
                date: String::new(),
            },
            min_temp: TrendExtreme {
                value: 0.0,
                date: String::new(),
            },
            total_precipitation: 0.0,
            avg_humidity: 0.0,
        };
    }

    let n = days.len() as f64;

    // Average temperature
    let avg_temp: f64 = days.iter().map(|d| d.temp_avg).sum::<f64>() / n;

    // Max and min temperatures with dates
    let max_day = days
        .iter()
        .max_by(|a, b| a.temp_max.partial_cmp(&b.temp_max).unwrap())
        .unwrap();
    let min_day = days
        .iter()
        .min_by(|a, b| a.temp_min.partial_cmp(&b.temp_min).unwrap())
        .unwrap();

    // Total precipitation
    let total_precipitation: f64 = days.iter().map(|d| d.precipitation_total).sum();

    // Average humidity
    let avg_humidity: f64 = days.iter().map(|d| d.humidity_avg).sum::<f64>() / n;

    // Simple linear regression for trend direction
    let temp_trend = compute_trend_direction(days);

    TrendSummary {
        avg_temp: round_2(avg_temp),
        temp_trend,
        max_temp: TrendExtreme {
            value: max_day.temp_max,
            date: max_day.date.clone(),
        },
        min_temp: TrendExtreme {
            value: min_day.temp_min,
            date: min_day.date.clone(),
        },
        total_precipitation: round_2(total_precipitation),
        avg_humidity: round_2(avg_humidity),
    }
}

/// Compute trend direction using simple linear regression on daily avg temperatures
fn compute_trend_direction(days: &[DailyHistorySummary]) -> String {
    let n = days.len() as f64;
    if n < 2.0 {
        return "stable".to_string();
    }

    // x = day index (0, 1, 2, ...), y = temp_avg
    let sum_x: f64 = (0..days.len()).map(|i| i as f64).sum();
    let sum_y: f64 = days.iter().map(|d| d.temp_avg).sum();
    let sum_xy: f64 = days
        .iter()
        .enumerate()
        .map(|(i, d)| i as f64 * d.temp_avg)
        .sum();
    let sum_x2: f64 = (0..days.len()).map(|i| (i as f64) * (i as f64)).sum();

    let denominator = n * sum_x2 - sum_x * sum_x;
    if denominator.abs() < f64::EPSILON {
        return "stable".to_string();
    }

    let slope = (n * sum_xy - sum_x * sum_y) / denominator;

    // Use a threshold relative to the temperature range
    let threshold = 0.1; // degrees per day
    if slope > threshold {
        "rising".to_string()
    } else if slope < -threshold {
        "falling".to_string()
    } else {
        "stable".to_string()
    }
}

fn round_2(val: f64) -> f64 {
    (val * 100.0).round() / 100.0
}

fn format_period(start_ts: i64, end_ts: i64) -> String {
    let days = (end_ts - start_ts) / 86400;
    format!("{}d", days)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_daily(date: &str, temp_avg: f64, temp_min: f64, temp_max: f64) -> DailyHistorySummary {
        DailyHistorySummary {
            date: date.to_string(),
            temp_min,
            temp_max,
            temp_avg,
            humidity_avg: 60.0,
            wind_speed_avg: 5.0,
            precipitation_total: 0.0,
            dominant_condition: Some("clear sky".to_string()),
        }
    }

    #[test]
    fn test_trend_calculation_rising() {
        let days = vec![
            make_daily("2024-01-01", 10.0, 5.0, 15.0),
            make_daily("2024-01-02", 12.0, 7.0, 17.0),
            make_daily("2024-01-03", 14.0, 9.0, 19.0),
            make_daily("2024-01-04", 16.0, 11.0, 21.0),
            make_daily("2024-01-05", 18.0, 13.0, 23.0),
        ];

        let summary = compute_trend_summary(&days);
        assert_eq!(summary.temp_trend, "rising");
        assert_eq!(summary.max_temp.value, 23.0);
        assert_eq!(summary.min_temp.value, 5.0);
    }

    #[test]
    fn test_trend_calculation_falling() {
        let days = vec![
            make_daily("2024-01-01", 20.0, 15.0, 25.0),
            make_daily("2024-01-02", 18.0, 13.0, 23.0),
            make_daily("2024-01-03", 16.0, 11.0, 21.0),
            make_daily("2024-01-04", 14.0, 9.0, 19.0),
            make_daily("2024-01-05", 12.0, 7.0, 17.0),
        ];

        let summary = compute_trend_summary(&days);
        assert_eq!(summary.temp_trend, "falling");
    }

    #[test]
    fn test_trend_calculation_stable() {
        let days = vec![
            make_daily("2024-01-01", 15.0, 10.0, 20.0),
            make_daily("2024-01-02", 15.05, 10.0, 20.0),
            make_daily("2024-01-03", 14.95, 10.0, 20.0),
            make_daily("2024-01-04", 15.0, 10.0, 20.0),
            make_daily("2024-01-05", 15.02, 10.0, 20.0),
        ];

        let summary = compute_trend_summary(&days);
        assert_eq!(summary.temp_trend, "stable");
    }

    #[test]
    fn test_trend_empty_days() {
        let summary = compute_trend_summary(&[]);
        assert_eq!(summary.temp_trend, "stable");
        assert_eq!(summary.avg_temp, 0.0);
    }

    #[test]
    fn test_default_date_range() {
        // When start is None, it defaults to end - 7 days
        let now = chrono::Utc::now().timestamp();
        let end = now;
        let start = end - DEFAULT_RANGE_DAYS * 86400;
        assert_eq!(end - start, 7 * 86400);
    }

    #[test]
    fn test_format_period() {
        assert_eq!(format_period(0, 7 * 86400), "7d");
        assert_eq!(format_period(0, 30 * 86400), "30d");
    }

    #[test]
    fn test_round_2() {
        assert_eq!(round_2(15.456), 15.46);
        assert_eq!(round_2(15.0), 15.0);
        assert_eq!(round_2(15.005), 15.01);
    }
}
