use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::models::Device;

/// File-based storage for device registrations
pub struct DeviceStorage {
    devices: Arc<RwLock<HashMap<String, Device>>>,
    file_path: String,
}

impl DeviceStorage {
    /// Create a new device storage with the given file path
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            file_path: file_path.into(),
        }
    }

    /// Load devices from file
    pub async fn load(&self) -> Result<(), std::io::Error> {
        let path = Path::new(&self.file_path);

        if !path.exists() {
            tracing::debug!("Device storage file does not exist, starting fresh");
            return Ok(());
        }

        let content = tokio::fs::read_to_string(path).await?;
        let devices: HashMap<String, Device> = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut storage = self.devices.write().await;
        *storage = devices;

        tracing::info!(count = storage.len(), "Loaded devices from storage");

        Ok(())
    }

    /// Save devices to file
    pub async fn save(&self) -> Result<(), std::io::Error> {
        let devices = self.devices.read().await;
        let content = serde_json::to_string_pretty(&*devices)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Create parent directory if needed
        if let Some(parent) = Path::new(&self.file_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&self.file_path, content).await?;

        tracing::debug!(count = devices.len(), "Saved devices to storage");

        Ok(())
    }

    /// Add or update a device
    pub async fn upsert(&self, device: Device) -> Result<(), std::io::Error> {
        {
            let mut devices = self.devices.write().await;
            devices.insert(device.token.clone(), device);
        }
        self.save().await
    }

    /// Get a device by token
    pub async fn get_by_token(&self, token: &str) -> Option<Device> {
        let devices = self.devices.read().await;
        devices.get(token).cloned()
    }

    /// Get a device by ID
    #[allow(dead_code)]
    pub async fn get_by_id(&self, id: &str) -> Option<Device> {
        let devices = self.devices.read().await;
        devices.values().find(|d| d.id == id).cloned()
    }

    /// Remove a device by token
    pub async fn remove(&self, token: &str) -> Result<bool, std::io::Error> {
        let existed = {
            let mut devices = self.devices.write().await;
            devices.remove(token).is_some()
        };

        if existed {
            self.save().await?;
        }

        Ok(existed)
    }

    /// Get all devices
    pub async fn get_all(&self) -> Vec<Device> {
        let devices = self.devices.read().await;
        devices.values().cloned().collect()
    }

    /// Get all enabled devices
    pub async fn get_enabled(&self) -> Vec<Device> {
        let devices = self.devices.read().await;
        devices.values().filter(|d| d.enabled).cloned().collect()
    }

    /// Get devices for a specific city
    pub async fn get_by_city(&self, city: &str) -> Vec<Device> {
        let devices = self.devices.read().await;
        let city_lower = city.to_lowercase();
        devices
            .values()
            .filter(|d| {
                d.enabled
                    && (d.cities.is_empty()
                        || d.cities.iter().any(|c| c.to_lowercase() == city_lower))
            })
            .cloned()
            .collect()
    }

    /// Get device count
    pub async fn count(&self) -> usize {
        let devices = self.devices.read().await;
        devices.len()
    }
}
