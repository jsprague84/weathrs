#![allow(dead_code)]

mod device_repo;
pub mod history_repo;
mod job_repo;

pub use device_repo::{DeviceRepository, SqliteDeviceRepository};
pub use job_repo::{JobRepository, SqliteJobRepository};

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Database configuration
pub struct DbConfig {
    pub url: String,
    pub max_connections: u32,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            url: "sqlite:data/weathrs.db".to_string(),
            max_connections: 5,
        }
    }
}

/// Create and configure a SQLite connection pool
pub async fn create_pool(config: &DbConfig) -> Result<SqlitePool, DbError> {
    // Ensure the data directory exists
    if let Some(db_path) = config.url.strip_prefix("sqlite:") {
        if let Some(parent) = Path::new(db_path).parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    DbError::Migration(format!("Failed to create database directory: {}", e))
                })?;
            }
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .connect(&format!("{}?mode=rwc", config.url))
        .await?;

    Ok(pool)
}

/// Run database migrations
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), DbError> {
    // Run migrations in order
    let migration_001 = include_str!("../../migrations/001_create_tables.sql");
    sqlx::raw_sql(migration_001).execute(pool).await?;

    let migration_002 = include_str!("../../migrations/002_create_history_table.sql");
    sqlx::raw_sql(migration_002).execute(pool).await?;

    tracing::info!("Database migrations completed");
    Ok(())
}

/// Import devices from JSON file to SQLite
pub async fn import_devices_from_json(
    pool: &SqlitePool,
    json_path: &str,
) -> Result<usize, DbError> {
    use crate::devices::Device;

    let path = Path::new(json_path);
    if !path.exists() {
        tracing::debug!(
            "No devices JSON file found at {}, skipping import",
            json_path
        );
        return Ok(0);
    }

    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| DbError::Migration(format!("Failed to read devices JSON: {}", e)))?;

    let devices: Vec<Device> = serde_json::from_str(&content)?;

    if devices.is_empty() {
        return Ok(0);
    }

    let repo = SqliteDeviceRepository::new(pool.clone());
    let mut imported = 0;

    for device in devices {
        // Check if device already exists in database
        if repo.get_by_token(&device.token).await?.is_none() {
            repo.upsert(&device).await?;
            imported += 1;
        }
    }

    if imported > 0 {
        tracing::info!(count = imported, "Imported devices from JSON to SQLite");
    }

    Ok(imported)
}

/// Import scheduler jobs from JSON file to SQLite
pub async fn import_jobs_from_json(pool: &SqlitePool, json_path: &str) -> Result<usize, DbError> {
    use crate::scheduler::ForecastJob;

    let path = Path::new(json_path);
    if !path.exists() {
        tracing::debug!("No jobs JSON file found at {}, skipping import", json_path);
        return Ok(0);
    }

    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| DbError::Migration(format!("Failed to read jobs JSON: {}", e)))?;

    let jobs: Vec<ForecastJob> = serde_json::from_str(&content)?;

    if jobs.is_empty() {
        return Ok(0);
    }

    let repo = SqliteJobRepository::new(pool.clone());
    let mut imported = 0;

    for job in jobs {
        // Check if job already exists in database
        if repo.get(&job.id).await?.is_none() {
            repo.upsert(&job).await?;
            imported += 1;
        }
    }

    if imported > 0 {
        tracing::info!(count = imported, "Imported jobs from JSON to SQLite");
    }

    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_pool() {
        let config = DbConfig {
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
        };
        let pool = create_pool(&config).await.expect("Failed to create pool");
        run_migrations(&pool)
            .await
            .expect("Failed to run migrations");
    }
}
