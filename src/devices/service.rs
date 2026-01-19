use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;
use thiserror::Error;
use uuid::Uuid;

use crate::notifications::{ExpoClient, NotificationMessage, Priority};

use super::models::{Device, DeviceRegistrationRequest, DeviceSettingsRequest};
use super::storage::DeviceStorage;

#[derive(Error, Debug)]
pub enum DevicesError {
    #[error("Device not found")]
    NotFound,

    #[error("Storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("Failed to send notification: {0}")]
    NotificationError(String),
}

/// Service for managing device registrations and sending push notifications
pub struct DevicesService {
    storage: DeviceStorage,
    expo_client: ExpoClient,
}

impl DevicesService {
    /// Create a new devices service
    pub fn new(client: Client, storage_path: impl Into<String>) -> Self {
        Self {
            storage: DeviceStorage::new(storage_path),
            expo_client: ExpoClient::new(client),
        }
    }

    /// Initialize the service (load existing devices from storage)
    pub async fn init(&self) -> Result<(), DevicesError> {
        self.storage.load().await?;
        tracing::info!(
            count = self.storage.count().await,
            "Devices service initialized"
        );
        Ok(())
    }

    /// Get current timestamp
    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// Register a new device or update existing
    pub async fn register(
        &self,
        request: DeviceRegistrationRequest,
    ) -> Result<Device, DevicesError> {
        let now = Self::now();

        // Check if device already exists
        let device = if let Some(mut existing) = self.storage.get_by_token(&request.token).await {
            // Update existing device
            existing.platform = request.platform;
            existing.device_name = request.device_name;
            existing.app_version = request.app_version;
            existing.cities = request.cities;
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

        self.storage.upsert(device.clone()).await?;

        tracing::info!(
            device_id = %device.id,
            platform = ?device.platform,
            "Device registered"
        );

        Ok(device)
    }

    /// Unregister a device
    pub async fn unregister(&self, token: &str) -> Result<bool, DevicesError> {
        let removed = self.storage.remove(token).await?;

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
            .storage
            .get_by_token(&request.token)
            .await
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

        self.storage.upsert(device.clone()).await?;

        tracing::info!(device_id = %device.id, "Device settings updated");

        Ok(device)
    }

    /// Get a device by token
    pub async fn get_by_token(&self, token: &str) -> Option<Device> {
        self.storage.get_by_token(token).await
    }

    /// Get all devices
    pub async fn get_all(&self) -> Vec<Device> {
        self.storage.get_all().await
    }

    /// Get device count
    pub async fn count(&self) -> usize {
        self.storage.count().await
    }

    /// Send a test notification to a device
    pub async fn send_test(&self, token: &str) -> Result<(), DevicesError> {
        let message = NotificationMessage {
            title: "Test Notification".to_string(),
            body: "This is a test notification from Weathrs!".to_string(),
            priority: Priority::Default,
            tags: vec!["test".to_string()],
        };

        self.expo_client
            .send_to_token(token, &message)
            .await
            .map_err(|e| DevicesError::NotificationError(e.to_string()))?;

        Ok(())
    }

    /// Send a notification to all enabled devices
    pub async fn broadcast(&self, message: &NotificationMessage) -> Result<usize, DevicesError> {
        let devices = self.storage.get_enabled().await;

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
        let devices = self.storage.get_by_city(city).await;

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
