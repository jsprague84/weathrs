use async_trait::async_trait;
use sqlx::SqlitePool;

use super::DbError;

/// A single weather history record stored in SQLite
#[derive(Debug, Clone)]
pub struct HistoryRecord {
    pub city: String,
    pub lat: f64,
    pub lon: f64,
    pub timestamp: i64,
    pub temperature: f64,
    pub feels_like: f64,
    pub humidity: i32,
    pub pressure: i32,
    pub wind_speed: f64,
    pub wind_direction: Option<i32>,
    pub clouds: Option<i32>,
    pub visibility: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub rain_1h: Option<f64>,
    pub snow_1h: Option<f64>,
    pub units: String,
    pub fetched_at: i64,
}

/// Aggregated daily summary from history data
#[derive(Debug, Clone)]
pub struct DailySummaryRow {
    pub date: String,
    pub temp_min: f64,
    pub temp_max: f64,
    pub temp_avg: f64,
    pub humidity_avg: f64,
    pub wind_speed_avg: f64,
    pub precipitation_total: f64,
    pub dominant_condition: Option<String>,
}

/// Repository trait for weather history operations
#[async_trait]
pub trait HistoryRepository: Send + Sync {
    /// Get history records for a city within a time range
    async fn get_range(
        &self,
        city: &str,
        start_ts: i64,
        end_ts: i64,
        units: &str,
    ) -> Result<Vec<HistoryRecord>, DbError>;

    /// Get daily summaries (aggregated) for a city within a time range
    async fn get_daily_summary(
        &self,
        city: &str,
        start_ts: i64,
        end_ts: i64,
        units: &str,
    ) -> Result<Vec<DailySummaryRow>, DbError>;

    /// Insert a batch of records, ignoring duplicates
    async fn insert_batch(&self, records: &[HistoryRecord]) -> Result<usize, DbError>;

    /// Check if data exists for a specific city, timestamp, and units
    async fn has_data(&self, city: &str, timestamp: i64, units: &str) -> Result<bool, DbError>;

    /// Find timestamps in a range that are missing from the database
    async fn get_missing_timestamps(
        &self,
        city: &str,
        start_ts: i64,
        end_ts: i64,
        interval_secs: i64,
        units: &str,
    ) -> Result<Vec<i64>, DbError>;

    /// Delete records older than the given timestamp
    async fn cleanup_old(&self, before_ts: i64) -> Result<usize, DbError>;
}

/// SQLite implementation of HistoryRepository
pub struct SqliteHistoryRepository {
    pool: SqlitePool,
}

impl SqliteHistoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Internal row structure for SQLite queries
#[derive(sqlx::FromRow)]
struct HistoryRow {
    city: String,
    lat: f64,
    lon: f64,
    timestamp: i64,
    temperature: f64,
    feels_like: f64,
    humidity: i32,
    pressure: i32,
    wind_speed: f64,
    wind_direction: Option<i32>,
    clouds: Option<i32>,
    visibility: Option<i32>,
    description: Option<String>,
    icon: Option<String>,
    rain_1h: Option<f64>,
    snow_1h: Option<f64>,
    units: String,
    fetched_at: i64,
}

impl From<HistoryRow> for HistoryRecord {
    fn from(row: HistoryRow) -> Self {
        HistoryRecord {
            city: row.city,
            lat: row.lat,
            lon: row.lon,
            timestamp: row.timestamp,
            temperature: row.temperature,
            feels_like: row.feels_like,
            humidity: row.humidity,
            pressure: row.pressure,
            wind_speed: row.wind_speed,
            wind_direction: row.wind_direction,
            clouds: row.clouds,
            visibility: row.visibility,
            description: row.description,
            icon: row.icon,
            rain_1h: row.rain_1h,
            snow_1h: row.snow_1h,
            units: row.units,
            fetched_at: row.fetched_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DailySummaryDbRow {
    date: String,
    temp_min: f64,
    temp_max: f64,
    temp_avg: f64,
    humidity_avg: f64,
    wind_speed_avg: f64,
    precipitation_total: f64,
    dominant_condition: Option<String>,
}

#[derive(sqlx::FromRow)]
struct TimestampRow {
    timestamp: i64,
}

#[async_trait]
impl HistoryRepository for SqliteHistoryRepository {
    async fn get_range(
        &self,
        city: &str,
        start_ts: i64,
        end_ts: i64,
        units: &str,
    ) -> Result<Vec<HistoryRecord>, DbError> {
        let rows: Vec<HistoryRow> = sqlx::query_as(
            "SELECT city, lat, lon, timestamp, temperature, feels_like, humidity, pressure,
                    wind_speed, wind_direction, clouds, visibility, description, icon,
                    rain_1h, snow_1h, units, fetched_at
             FROM weather_history
             WHERE city = ? AND timestamp >= ? AND timestamp <= ? AND units = ?
             ORDER BY timestamp ASC",
        )
        .bind(city)
        .bind(start_ts)
        .bind(end_ts)
        .bind(units)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_daily_summary(
        &self,
        city: &str,
        start_ts: i64,
        end_ts: i64,
        units: &str,
    ) -> Result<Vec<DailySummaryRow>, DbError> {
        let rows: Vec<DailySummaryDbRow> = sqlx::query_as(
            "SELECT
                date(timestamp, 'unixepoch') as date,
                MIN(temperature) as temp_min,
                MAX(temperature) as temp_max,
                AVG(temperature) as temp_avg,
                AVG(humidity) as humidity_avg,
                AVG(wind_speed) as wind_speed_avg,
                COALESCE(SUM(COALESCE(rain_1h, 0.0) + COALESCE(snow_1h, 0.0)), 0.0) as precipitation_total,
                (SELECT h2.description FROM weather_history h2
                 WHERE h2.city = weather_history.city
                   AND date(h2.timestamp, 'unixepoch') = date(weather_history.timestamp, 'unixepoch')
                   AND h2.units = weather_history.units
                 GROUP BY h2.description
                 ORDER BY COUNT(*) DESC
                 LIMIT 1) as dominant_condition
             FROM weather_history
             WHERE city = ? AND timestamp >= ? AND timestamp <= ? AND units = ?
             GROUP BY date(timestamp, 'unixepoch')
             ORDER BY date ASC",
        )
        .bind(city)
        .bind(start_ts)
        .bind(end_ts)
        .bind(units)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| DailySummaryRow {
                date: r.date,
                temp_min: r.temp_min,
                temp_max: r.temp_max,
                temp_avg: r.temp_avg,
                humidity_avg: r.humidity_avg,
                wind_speed_avg: r.wind_speed_avg,
                precipitation_total: r.precipitation_total,
                dominant_condition: r.dominant_condition,
            })
            .collect())
    }

    async fn insert_batch(&self, records: &[HistoryRecord]) -> Result<usize, DbError> {
        let mut inserted = 0;
        for record in records {
            let result = sqlx::query(
                "INSERT OR IGNORE INTO weather_history
                 (city, lat, lon, timestamp, temperature, feels_like, humidity, pressure,
                  wind_speed, wind_direction, clouds, visibility, description, icon,
                  rain_1h, snow_1h, units, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&record.city)
            .bind(record.lat)
            .bind(record.lon)
            .bind(record.timestamp)
            .bind(record.temperature)
            .bind(record.feels_like)
            .bind(record.humidity)
            .bind(record.pressure)
            .bind(record.wind_speed)
            .bind(record.wind_direction)
            .bind(record.clouds)
            .bind(record.visibility)
            .bind(&record.description)
            .bind(&record.icon)
            .bind(record.rain_1h)
            .bind(record.snow_1h)
            .bind(&record.units)
            .bind(record.fetched_at)
            .execute(&self.pool)
            .await?;

            if result.rows_affected() > 0 {
                inserted += 1;
            }
        }
        Ok(inserted)
    }

    async fn has_data(&self, city: &str, timestamp: i64, units: &str) -> Result<bool, DbError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM weather_history WHERE city = ? AND timestamp = ? AND units = ?",
        )
        .bind(city)
        .bind(timestamp)
        .bind(units)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0 > 0)
    }

    async fn get_missing_timestamps(
        &self,
        city: &str,
        start_ts: i64,
        end_ts: i64,
        interval_secs: i64,
        units: &str,
    ) -> Result<Vec<i64>, DbError> {
        // Get existing timestamps in the range
        let existing: Vec<TimestampRow> = sqlx::query_as(
            "SELECT timestamp FROM weather_history
             WHERE city = ? AND timestamp >= ? AND timestamp <= ? AND units = ?",
        )
        .bind(city)
        .bind(start_ts)
        .bind(end_ts)
        .bind(units)
        .fetch_all(&self.pool)
        .await?;

        let existing_set: std::collections::HashSet<i64> =
            existing.into_iter().map(|r| r.timestamp).collect();

        // Generate expected timestamps and find missing ones
        let mut missing = Vec::new();
        let mut ts = start_ts;
        while ts <= end_ts {
            if !existing_set.contains(&ts) {
                missing.push(ts);
            }
            ts += interval_secs;
        }

        Ok(missing)
    }

    async fn cleanup_old(&self, before_ts: i64) -> Result<usize, DbError> {
        let result = sqlx::query("DELETE FROM weather_history WHERE timestamp < ?")
            .bind(before_ts)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_pool, run_migrations, DbConfig};

    async fn setup_test_db() -> SqlitePool {
        let config = DbConfig {
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
        };
        let pool = create_pool(&config).await.unwrap();
        run_migrations(&pool).await.unwrap();
        pool
    }

    fn create_test_record(city: &str, timestamp: i64) -> HistoryRecord {
        HistoryRecord {
            city: city.to_string(),
            lat: 41.8781,
            lon: -87.6298,
            timestamp,
            temperature: 20.5,
            feels_like: 19.0,
            humidity: 65,
            pressure: 1013,
            wind_speed: 5.5,
            wind_direction: Some(180),
            clouds: Some(40),
            visibility: Some(10000),
            description: Some("clear sky".to_string()),
            icon: Some("01d".to_string()),
            rain_1h: None,
            snow_1h: None,
            units: "metric".to_string(),
            fetched_at: 1700000000,
        }
    }

    #[tokio::test]
    async fn test_insert_and_get_range() {
        let pool = setup_test_db().await;
        let repo = SqliteHistoryRepository::new(pool);

        let records = vec![
            create_test_record("Chicago", 1700000000),
            create_test_record("Chicago", 1700003600),
            create_test_record("Chicago", 1700007200),
        ];

        let inserted = repo.insert_batch(&records).await.unwrap();
        assert_eq!(inserted, 3);

        let result = repo
            .get_range("Chicago", 1700000000, 1700007200, "metric")
            .await
            .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].timestamp, 1700000000);
        assert_eq!(result[2].timestamp, 1700007200);
    }

    #[tokio::test]
    async fn test_dedup_on_insert() {
        let pool = setup_test_db().await;
        let repo = SqliteHistoryRepository::new(pool);

        let records = vec![
            create_test_record("Chicago", 1700000000),
            create_test_record("Chicago", 1700000000), // duplicate
        ];

        let inserted = repo.insert_batch(&records).await.unwrap();
        assert_eq!(inserted, 1); // Second should be ignored

        // Insert same record again
        let inserted2 = repo.insert_batch(&records[..1]).await.unwrap();
        assert_eq!(inserted2, 0); // Already exists
    }

    #[tokio::test]
    async fn test_daily_summary_aggregation() {
        let pool = setup_test_db().await;
        let repo = SqliteHistoryRepository::new(pool);

        // Insert records across two days
        let mut records = Vec::new();
        // Day 1: 2023-11-14 (timestamps around 1699920000)
        let day1_base = 1699920000_i64;
        for i in 0..8 {
            let mut r = create_test_record("Chicago", day1_base + i * 3600);
            r.temperature = 15.0 + i as f64; // 15-22
            records.push(r);
        }
        // Day 2: 2023-11-15
        let day2_base = day1_base + 86400;
        for i in 0..8 {
            let mut r = create_test_record("Chicago", day2_base + i * 3600);
            r.temperature = 10.0 + i as f64; // 10-17
            records.push(r);
        }

        repo.insert_batch(&records).await.unwrap();

        let summaries = repo
            .get_daily_summary("Chicago", day1_base, day2_base + 86400, "metric")
            .await
            .unwrap();

        assert_eq!(summaries.len(), 2);
        assert!(summaries[0].temp_min <= summaries[0].temp_max);
        assert!(summaries[1].temp_min <= summaries[1].temp_max);
    }

    #[tokio::test]
    async fn test_missing_timestamps() {
        let pool = setup_test_db().await;
        let repo = SqliteHistoryRepository::new(pool);

        // Insert data at hours 0 and 2, but not hour 1
        let base = 1700000000_i64;
        let records = vec![
            create_test_record("Chicago", base),
            create_test_record("Chicago", base + 7200), // skip 3600
        ];
        repo.insert_batch(&records).await.unwrap();

        let missing = repo
            .get_missing_timestamps("Chicago", base, base + 7200, 3600, "metric")
            .await
            .unwrap();

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], base + 3600);
    }

    #[tokio::test]
    async fn test_cleanup_old() {
        let pool = setup_test_db().await;
        let repo = SqliteHistoryRepository::new(pool);

        let records = vec![
            create_test_record("Chicago", 1000),
            create_test_record("Chicago", 2000),
            create_test_record("Chicago", 3000),
        ];
        repo.insert_batch(&records).await.unwrap();

        let deleted = repo.cleanup_old(2500).await.unwrap();
        assert_eq!(deleted, 2);

        let remaining = repo.get_range("Chicago", 0, 5000, "metric").await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].timestamp, 3000);
    }
}
