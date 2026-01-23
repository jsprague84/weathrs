use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::scheduler::{ForecastJob, NotifyConfig};

use super::DbError;

/// Repository trait for scheduler job operations
#[async_trait]
pub trait JobRepository: Send + Sync {
    /// Get a job by its ID
    async fn get(&self, id: &str) -> Result<Option<ForecastJob>, DbError>;

    /// Get all jobs
    async fn get_all(&self) -> Result<Vec<ForecastJob>, DbError>;

    /// Get all enabled jobs
    async fn get_enabled(&self) -> Result<Vec<ForecastJob>, DbError>;

    /// Check if a job exists
    async fn exists(&self, id: &str) -> Result<bool, DbError>;

    /// Insert or update a job
    async fn upsert(&self, job: &ForecastJob) -> Result<(), DbError>;

    /// Remove a job by ID
    async fn remove(&self, id: &str) -> Result<bool, DbError>;

    /// Get the total count of jobs
    async fn count(&self) -> Result<usize, DbError>;
}

/// SQLite implementation of JobRepository
pub struct SqliteJobRepository {
    pool: SqlitePool,
}

impl SqliteJobRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_job(row: JobRow) -> Result<ForecastJob, DbError> {
        let notify: NotifyConfig = serde_json::from_str(&row.notify_config)?;

        Ok(ForecastJob {
            id: row.id,
            name: row.name,
            city: row.city,
            units: row.units,
            cron: row.cron,
            timezone: row.timezone,
            include_daily: row.include_daily != 0,
            include_hourly: row.include_hourly != 0,
            enabled: row.enabled != 0,
            notify,
        })
    }
}

/// Internal row structure for SQLite queries
#[derive(sqlx::FromRow)]
struct JobRow {
    id: String,
    name: String,
    city: String,
    units: String,
    cron: String,
    timezone: String,
    include_daily: i32,
    include_hourly: i32,
    enabled: i32,
    notify_config: String,
}

#[async_trait]
impl JobRepository for SqliteJobRepository {
    async fn get(&self, id: &str) -> Result<Option<ForecastJob>, DbError> {
        let row: Option<JobRow> = sqlx::query_as(
            "SELECT id, name, city, units, cron, timezone, include_daily, include_hourly, enabled, notify_config
             FROM scheduler_jobs WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::row_to_job).transpose()
    }

    async fn get_all(&self) -> Result<Vec<ForecastJob>, DbError> {
        let rows: Vec<JobRow> = sqlx::query_as(
            "SELECT id, name, city, units, cron, timezone, include_daily, include_hourly, enabled, notify_config
             FROM scheduler_jobs ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_job).collect()
    }

    async fn get_enabled(&self) -> Result<Vec<ForecastJob>, DbError> {
        let rows: Vec<JobRow> = sqlx::query_as(
            "SELECT id, name, city, units, cron, timezone, include_daily, include_hourly, enabled, notify_config
             FROM scheduler_jobs WHERE enabled = 1 ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_job).collect()
    }

    async fn exists(&self, id: &str) -> Result<bool, DbError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scheduler_jobs WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0 > 0)
    }

    async fn upsert(&self, job: &ForecastJob) -> Result<(), DbError> {
        let notify_json = serde_json::to_string(&job.notify)?;

        sqlx::query(
            "INSERT INTO scheduler_jobs (id, name, city, units, cron, timezone, include_daily, include_hourly, enabled, notify_config)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                city = excluded.city,
                units = excluded.units,
                cron = excluded.cron,
                timezone = excluded.timezone,
                include_daily = excluded.include_daily,
                include_hourly = excluded.include_hourly,
                enabled = excluded.enabled,
                notify_config = excluded.notify_config"
        )
        .bind(&job.id)
        .bind(&job.name)
        .bind(&job.city)
        .bind(&job.units)
        .bind(&job.cron)
        .bind(&job.timezone)
        .bind(if job.include_daily { 1 } else { 0 })
        .bind(if job.include_hourly { 1 } else { 0 })
        .bind(if job.enabled { 1 } else { 0 })
        .bind(&notify_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove(&self, id: &str) -> Result<bool, DbError> {
        let result = sqlx::query("DELETE FROM scheduler_jobs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn count(&self) -> Result<usize, DbError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scheduler_jobs")
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0 as usize)
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

    fn create_test_job(id: &str) -> ForecastJob {
        ForecastJob {
            id: id.to_string(),
            name: format!("Test Job {}", id),
            city: "Chicago".to_string(),
            units: "metric".to_string(),
            cron: "0 0 7 * * *".to_string(),
            timezone: "America/Chicago".to_string(),
            include_daily: true,
            include_hourly: false,
            enabled: true,
            notify: NotifyConfig {
                on_run: true,
                on_alert: true,
                on_precipitation: false,
                cold_threshold: Some(0.0),
                heat_threshold: Some(35.0),
            },
        }
    }

    #[tokio::test]
    async fn test_upsert_and_get() {
        let pool = setup_test_db().await;
        let repo = SqliteJobRepository::new(pool);

        let job = create_test_job("test-job-1");
        repo.upsert(&job).await.unwrap();

        let retrieved = repo.get("test-job-1").await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, job.id);
        assert_eq!(retrieved.name, job.name);
        assert_eq!(retrieved.city, job.city);
        assert_eq!(retrieved.notify.cold_threshold, Some(0.0));
    }

    #[tokio::test]
    async fn test_get_enabled() {
        let pool = setup_test_db().await;
        let repo = SqliteJobRepository::new(pool);

        let job1 = create_test_job("job1");
        let mut job2 = create_test_job("job2");
        job2.enabled = false;

        repo.upsert(&job1).await.unwrap();
        repo.upsert(&job2).await.unwrap();

        let enabled = repo.get_enabled().await.unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "job1");
    }

    #[tokio::test]
    async fn test_exists() {
        let pool = setup_test_db().await;
        let repo = SqliteJobRepository::new(pool);

        assert!(!repo.exists("nonexistent").await.unwrap());

        repo.upsert(&create_test_job("exists-job")).await.unwrap();
        assert!(repo.exists("exists-job").await.unwrap());
    }

    #[tokio::test]
    async fn test_remove() {
        let pool = setup_test_db().await;
        let repo = SqliteJobRepository::new(pool);

        let job = create_test_job("job-to-remove");
        repo.upsert(&job).await.unwrap();

        let removed = repo.remove("job-to-remove").await.unwrap();
        assert!(removed);

        let retrieved = repo.get("job-to-remove").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_count() {
        let pool = setup_test_db().await;
        let repo = SqliteJobRepository::new(pool);

        assert_eq!(repo.count().await.unwrap(), 0);

        repo.upsert(&create_test_job("job1")).await.unwrap();
        repo.upsert(&create_test_job("job2")).await.unwrap();

        assert_eq!(repo.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_update_existing() {
        let pool = setup_test_db().await;
        let repo = SqliteJobRepository::new(pool);

        let mut job = create_test_job("update-test");
        repo.upsert(&job).await.unwrap();

        job.city = "London".to_string();
        job.enabled = false;
        repo.upsert(&job).await.unwrap();

        let retrieved = repo.get("update-test").await.unwrap().unwrap();
        assert_eq!(retrieved.city, "London");
        assert!(!retrieved.enabled);
    }
}
