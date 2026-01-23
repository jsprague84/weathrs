use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::devices::{Device, Platform};

use super::DbError;

/// Repository trait for device operations
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Get a device by its token
    async fn get_by_token(&self, token: &str) -> Result<Option<Device>, DbError>;

    /// Get a device by its ID
    async fn get_by_id(&self, id: &str) -> Result<Option<Device>, DbError>;

    /// Get all devices
    async fn get_all(&self) -> Result<Vec<Device>, DbError>;

    /// Get all enabled devices
    async fn get_enabled(&self) -> Result<Vec<Device>, DbError>;

    /// Get devices subscribed to a specific city
    async fn get_by_city(&self, city: &str) -> Result<Vec<Device>, DbError>;

    /// Insert or update a device
    async fn upsert(&self, device: &Device) -> Result<(), DbError>;

    /// Remove a device by token
    async fn remove(&self, token: &str) -> Result<bool, DbError>;

    /// Get the total count of devices
    async fn count(&self) -> Result<usize, DbError>;
}

/// SQLite implementation of DeviceRepository
pub struct SqliteDeviceRepository {
    pool: SqlitePool,
}

impl SqliteDeviceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn platform_to_str(platform: &Platform) -> &'static str {
        match platform {
            Platform::Ios => "ios",
            Platform::Android => "android",
            Platform::Web => "web",
        }
    }

    fn str_to_platform(s: &str) -> Platform {
        match s {
            "ios" => Platform::Ios,
            "android" => Platform::Android,
            "web" => Platform::Web,
            _ => Platform::Android, // Default fallback
        }
    }

    fn row_to_device(row: DeviceRow) -> Result<Device, DbError> {
        let cities: Vec<String> = serde_json::from_str(&row.cities)?;

        Ok(Device {
            id: row.id,
            token: row.token,
            platform: Self::str_to_platform(&row.platform),
            device_name: row.device_name,
            app_version: row.app_version,
            cities,
            units: row.units,
            enabled: row.enabled != 0,
            registered_at: row.registered_at,
            updated_at: row.updated_at,
        })
    }
}

/// Internal row structure for SQLite queries
#[derive(sqlx::FromRow)]
struct DeviceRow {
    id: String,
    token: String,
    platform: String,
    device_name: Option<String>,
    app_version: Option<String>,
    cities: String,
    units: String,
    enabled: i32,
    registered_at: i64,
    updated_at: i64,
}

#[async_trait]
impl DeviceRepository for SqliteDeviceRepository {
    async fn get_by_token(&self, token: &str) -> Result<Option<Device>, DbError> {
        let row: Option<DeviceRow> = sqlx::query_as(
            "SELECT id, token, platform, device_name, app_version, cities, units, enabled, registered_at, updated_at
             FROM devices WHERE token = ?"
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::row_to_device).transpose()
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<Device>, DbError> {
        let row: Option<DeviceRow> = sqlx::query_as(
            "SELECT id, token, platform, device_name, app_version, cities, units, enabled, registered_at, updated_at
             FROM devices WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::row_to_device).transpose()
    }

    async fn get_all(&self) -> Result<Vec<Device>, DbError> {
        let rows: Vec<DeviceRow> = sqlx::query_as(
            "SELECT id, token, platform, device_name, app_version, cities, units, enabled, registered_at, updated_at
             FROM devices ORDER BY registered_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_device).collect()
    }

    async fn get_enabled(&self) -> Result<Vec<Device>, DbError> {
        let rows: Vec<DeviceRow> = sqlx::query_as(
            "SELECT id, token, platform, device_name, app_version, cities, units, enabled, registered_at, updated_at
             FROM devices WHERE enabled = 1 ORDER BY registered_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_device).collect()
    }

    async fn get_by_city(&self, city: &str) -> Result<Vec<Device>, DbError> {
        // SQLite JSON contains check - cities is stored as JSON array
        let rows: Vec<DeviceRow> = sqlx::query_as(
            r#"SELECT id, token, platform, device_name, app_version, cities, units, enabled, registered_at, updated_at
               FROM devices
               WHERE enabled = 1
               AND (cities LIKE '%"' || ? || '"%' OR cities LIKE '%' || LOWER(?) || '%')
               ORDER BY registered_at DESC"#
        )
        .bind(city)
        .bind(city)
        .fetch_all(&self.pool)
        .await?;

        // Filter to ensure exact city match (case-insensitive)
        let city_lower = city.to_lowercase();
        rows.into_iter()
            .map(Self::row_to_device)
            .filter_map(|r| r.ok())
            .filter(|d| d.cities.iter().any(|c| c.to_lowercase() == city_lower))
            .collect::<Vec<_>>()
            .into_iter()
            .map(Ok)
            .collect()
    }

    async fn upsert(&self, device: &Device) -> Result<(), DbError> {
        let cities_json = serde_json::to_string(&device.cities)?;

        sqlx::query(
            "INSERT INTO devices (id, token, platform, device_name, app_version, cities, units, enabled, registered_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                token = excluded.token,
                platform = excluded.platform,
                device_name = excluded.device_name,
                app_version = excluded.app_version,
                cities = excluded.cities,
                units = excluded.units,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at"
        )
        .bind(&device.id)
        .bind(&device.token)
        .bind(Self::platform_to_str(&device.platform))
        .bind(&device.device_name)
        .bind(&device.app_version)
        .bind(&cities_json)
        .bind(&device.units)
        .bind(if device.enabled { 1 } else { 0 })
        .bind(device.registered_at)
        .bind(device.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove(&self, token: &str) -> Result<bool, DbError> {
        let result = sqlx::query("DELETE FROM devices WHERE token = ?")
            .bind(token)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn count(&self) -> Result<usize, DbError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM devices")
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

    fn create_test_device(token: &str) -> Device {
        Device {
            id: uuid::Uuid::new_v4().to_string(),
            token: token.to_string(),
            platform: Platform::Android,
            device_name: Some("Test Device".to_string()),
            app_version: Some("1.0.0".to_string()),
            cities: vec!["Chicago".to_string(), "London".to_string()],
            units: "metric".to_string(),
            enabled: true,
            registered_at: 1700000000,
            updated_at: 1700000000,
        }
    }

    #[tokio::test]
    async fn test_upsert_and_get_by_token() {
        let pool = setup_test_db().await;
        let repo = SqliteDeviceRepository::new(pool);

        let device = create_test_device("test_token_123");
        repo.upsert(&device).await.unwrap();

        let retrieved = repo.get_by_token("test_token_123").await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, device.id);
        assert_eq!(retrieved.token, device.token);
        assert_eq!(retrieved.cities, device.cities);
    }

    #[tokio::test]
    async fn test_get_by_city() {
        let pool = setup_test_db().await;
        let repo = SqliteDeviceRepository::new(pool);

        let device1 = create_test_device("token1");
        let mut device2 = create_test_device("token2");
        device2.cities = vec!["Paris".to_string()];

        repo.upsert(&device1).await.unwrap();
        repo.upsert(&device2).await.unwrap();

        let chicago_devices = repo.get_by_city("Chicago").await.unwrap();
        assert_eq!(chicago_devices.len(), 1);
        assert_eq!(chicago_devices[0].token, "token1");

        let paris_devices = repo.get_by_city("Paris").await.unwrap();
        assert_eq!(paris_devices.len(), 1);
        assert_eq!(paris_devices[0].token, "token2");
    }

    #[tokio::test]
    async fn test_remove() {
        let pool = setup_test_db().await;
        let repo = SqliteDeviceRepository::new(pool);

        let device = create_test_device("token_to_remove");
        repo.upsert(&device).await.unwrap();

        let removed = repo.remove("token_to_remove").await.unwrap();
        assert!(removed);

        let retrieved = repo.get_by_token("token_to_remove").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_count() {
        let pool = setup_test_db().await;
        let repo = SqliteDeviceRepository::new(pool);

        assert_eq!(repo.count().await.unwrap(), 0);

        repo.upsert(&create_test_device("token1")).await.unwrap();
        repo.upsert(&create_test_device("token2")).await.unwrap();

        assert_eq!(repo.count().await.unwrap(), 2);
    }
}
