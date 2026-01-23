mod expo;

pub use expo::ExpoClient;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("Failed to send notification: {0}")]
    SendError(#[from] reqwest::Error),

    #[error("Notification service returned error: {0}")]
    ServiceError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub tags: Vec<String>,
    /// City for navigation (used by Expo push notifications)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Min,
    Low,
    #[default]
    Default,
    High,
    Urgent,
}
