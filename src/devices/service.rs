use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;
use thiserror::Error;
use uuid::Uuid;

use crate::db::{DbError, DeviceRepository, SqliteDeviceRepository};
use crate::notifications::{ExpoClient, NotificationMessage, Priority};

use super::models::{Device, DeviceRegistrationRequest, DeviceSettingsRequest};

#[derive(Error, Debug)]
pub enum DevicesError {
    #[error("Device not found")]
    NotFound,

    #[error("Database error: {0}")]
    Database(#[from] DbError),

    #[error("Failed to send notification: {0}")]
    NotificationError(String),

    #[error("Invalid token: {0}")]
    InvalidToken(String),
}

/// Service for managing device registrations and sending push notifications
pub struct DevicesService {
    repo: SqliteDeviceRepository,
    expo_client: ExpoClient,
}

impl DevicesService {
    /// Create a new devices service backed by SQLite
    pub fn new(client: Client, pool: sqlx::SqlitePool) -> Self {
        Self {
            repo: SqliteDeviceRepository::new(pool),
            expo_client: ExpoClient::new(client),
        }
    }

    /// Get current timestamp
    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// Remove devices with invalid (non-Expo) push tokens
    pub async fn cleanup_invalid_tokens(&self) {
        let devices = self.get_all().await;
        let mut removed = 0;
        for device in &devices {
            if !device.token.starts_with("ExponentPushToken[") {
                if let Ok(true) = self.repo.remove(&device.token).await {
                    removed += 1;
                }
            }
        }
        if removed > 0 {
            tracing::info!(removed = removed, "Cleaned up devices with invalid tokens");
        }
    }

    /// Register a new device or update existing
    pub async fn register(
        &self,
        request: DeviceRegistrationRequest,
    ) -> Result<Device, DevicesError> {
        // Reject raw FCM tokens — only accept Expo push tokens
        if !request.token.starts_with("ExponentPushToken[") {
            return Err(DevicesError::InvalidToken(
                "Token must be an Expo push token (ExponentPushToken[...])".to_string(),
            ));
        }

        let now = Self::now();

        // Check if device already exists
        let device = if let Some(mut existing) = self.repo.get_by_token(&request.token).await? {
            // Update existing device — only overwrite cities if non-empty
            // (token rotation re-registers without cities)
            existing.platform = request.platform;
            existing.device_name = request.device_name.or(existing.device_name);
            existing.app_version = request.app_version.or(existing.app_version);
            if !request.cities.is_empty() {
                existing.cities = request.cities;
            }
            existing.units = request.units;
            existing.enabled = request.enabled;
            existing.updated_at = now;
            existing
        } else {
            // Create new device
            Device {
                id: Uuid::new_v4().to_string(),
                token: request.token,
                platform: request.platform,
                device_name: request.device_name,
                app_version: request.app_version,
                cities: request.cities,
                units: request.units,
                enabled: request.enabled,
                registered_at: now,
                updated_at: now,
            }
        };

        self.repo.upsert(&device).await?;

        tracing::info!(
            device_id = %device.id,
            platform = ?device.platform,
            "Device registered"
        );

        Ok(device)
    }

    /// Unregister a device
    pub async fn unregister(&self, token: &str) -> Result<bool, DevicesError> {
        let removed = self.repo.remove(token).await?;

        if removed {
            tracing::info!("Device unregistered");
        }

        Ok(removed)
    }

    /// Update device settings
    pub async fn update_settings(
        &self,
        request: DeviceSettingsRequest,
    ) -> Result<Device, DevicesError> {
        let mut device = self
            .repo
            .get_by_token(&request.token)
            .await?
            .ok_or(DevicesError::NotFound)?;

        if let Some(enabled) = request.enabled {
            device.enabled = enabled;
        }
        if let Some(cities) = request.cities {
            device.cities = cities;
        }
        if let Some(units) = request.units {
            device.units = units;
        }
        device.updated_at = Self::now();

        self.repo.upsert(&device).await?;

        tracing::info!(device_id = %device.id, "Device settings updated");

        Ok(device)
    }

    /// Get a device by token
    pub async fn get_by_token(&self, token: &str) -> Option<Device> {
        self.repo.get_by_token(token).await.unwrap_or_else(|e| {
            tracing::error!(error = %e, "Failed to get device by token");
            None
        })
    }

    /// Get all devices
    pub async fn get_all(&self) -> Vec<Device> {
        self.repo.get_all().await.unwrap_or_else(|e| {
            tracing::error!(error = %e, "Failed to get all devices");
            Vec::new()
        })
    }

    /// Get device count
    pub async fn count(&self) -> usize {
        self.repo.count().await.unwrap_or_else(|e| {
            tracing::error!(error = %e, "Failed to get device count");
            0
        })
    }

    /// Send a test notification to a device
    pub async fn send_test(&self, token: &str) -> Result<(), DevicesError> {
        let message = NotificationMessage {
            title: "Test Notification".to_string(),
            subtitle: Some("Weathrs Push Test".to_string()),
            body: "This is a test notification from Weathrs!".to_string(),
            priority: Priority::Default,
            tags: vec!["test".to_string()],
            city: None,
        };

        self.expo_client
            .send_to_token(token, &message)
            .await
            .map_err(|e| DevicesError::NotificationError(e.to_string()))?;

        Ok(())
    }

    /// Send a notification to all enabled devices
    pub async fn broadcast(&self, message: &NotificationMessage) -> Result<usize, DevicesError> {
        let devices = self.repo.get_enabled().await?;

        if devices.is_empty() {
            tracing::debug!("No enabled devices to broadcast to");
            return Ok(0);
        }

        let tokens: Vec<String> = devices.iter().map(|d| d.token.clone()).collect();
        let results = self.expo_client.send_to_tokens(&tokens, message).await;

        let success_count = results.iter().filter(|r| r.is_ok()).count();

        tracing::info!(
            total = devices.len(),
            success = success_count,
            "Broadcast complete"
        );

        Ok(success_count)
    }

    /// Send a notification to devices subscribed to a specific city
    pub async fn send_to_city(
        &self,
        city: &str,
        message: &NotificationMessage,
    ) -> Result<usize, DevicesError> {
        let devices = self.repo.get_by_city(city).await?;

        if devices.is_empty() {
            tracing::debug!(city = %city, "No devices subscribed to city");
            return Ok(0);
        }

        let tokens: Vec<String> = devices.iter().map(|d| d.token.clone()).collect();
        let results = self.expo_client.send_to_tokens(&tokens, message).await;

        let success_count = results.iter().filter(|r| r.is_ok()).count();

        tracing::info!(
            city = %city,
            total = devices.len(),
            success = success_count,
            "City notification complete"
        );

        Ok(success_count)
    }
}
