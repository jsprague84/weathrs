use serde::{Deserialize, Serialize};

/// Platform type for the device
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Ios,
    Android,
    Web,
}

/// A registered device for push notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Unique device ID (generated on registration)
    pub id: String,

    /// Expo push token
    pub token: String,

    /// Device platform
    pub platform: Platform,

    /// Optional device name
    pub device_name: Option<String>,

    /// App version
    pub app_version: Option<String>,

    /// Cities to receive notifications for
    #[serde(default)]
    pub cities: Vec<String>,

    /// Temperature units preference
    #[serde(default = "default_units")]
    pub units: String,

    /// Whether notifications are enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Registration timestamp
    pub registered_at: i64,

    /// Last updated timestamp
    pub updated_at: i64,
}

fn default_units() -> String {
    "imperial".to_string()
}

fn default_true() -> bool {
    true
}

/// Request to register a new device
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRegistrationRequest {
    pub token: String,
    pub platform: Platform,
    pub device_name: Option<String>,
    pub app_version: Option<String>,
    #[serde(default)]
    pub cities: Vec<String>,
    #[serde(default = "default_units")]
    pub units: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Request to unregister a device
#[derive(Debug, Deserialize)]
pub struct DeviceUnregisterRequest {
    pub token: String,
}

/// Request to update device settings
#[derive(Debug, Deserialize)]
pub struct DeviceSettingsRequest {
    pub token: String,
    pub enabled: Option<bool>,
    pub cities: Option<Vec<String>>,
    pub units: Option<String>,
}

/// Request to send a test notification
#[derive(Debug, Deserialize)]
pub struct TestNotificationRequest {
    pub token: String,
}

/// Response for device operations
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl DeviceResponse {
    pub fn success(device_id: Option<String>) -> Self {
        Self {
            success: true,
            device_id,
            message: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            device_id: None,
            message: Some(message.into()),
        }
    }
}
