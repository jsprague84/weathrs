mod gotify;
mod ntfy;

pub use gotify::GotifyClient;
pub use ntfy::NtfyClient;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("Failed to send notification: {0}")]
    SendError(#[from] reqwest::Error),

    #[error("Notification service returned error: {0}")]
    ServiceError(String),

    #[error("No notification services configured")]
    NoServicesConfigured,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub tags: Vec<String>,
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

impl Priority {
    pub fn as_ntfy_priority(self) -> u8 {
        match self {
            Priority::Min => 1,
            Priority::Low => 2,
            Priority::Default => 3,
            Priority::High => 4,
            Priority::Urgent => 5,
        }
    }

    pub fn as_gotify_priority(self) -> u8 {
        match self {
            Priority::Min => 0,
            Priority::Low => 2,
            Priority::Default => 5,
            Priority::High => 7,
            Priority::Urgent => 10,
        }
    }
}

/// Unified notification service that can send to multiple backends
pub struct NotificationService {
    ntfy: Option<NtfyClient>,
    gotify: Option<GotifyClient>,
}

impl NotificationService {
    pub fn new(ntfy: Option<NtfyClient>, gotify: Option<GotifyClient>) -> Self {
        Self { ntfy, gotify }
    }

    pub fn from_config(
        ntfy_url: Option<&str>,
        ntfy_topic: Option<&str>,
        ntfy_token: Option<&str>,
        gotify_url: Option<&str>,
        gotify_token: Option<&str>,
    ) -> Self {
        let ntfy = match (ntfy_url, ntfy_topic) {
            (Some(url), Some(topic)) => Some(NtfyClient::new(url, topic, ntfy_token)),
            _ => None,
        };

        let gotify = match (gotify_url, gotify_token) {
            (Some(url), Some(token)) => Some(GotifyClient::new(url, token)),
            _ => None,
        };

        Self { ntfy, gotify }
    }

    pub fn is_configured(&self) -> bool {
        self.ntfy.is_some() || self.gotify.is_some()
    }

    /// Send notification to all configured services
    pub async fn send(&self, message: &NotificationMessage) -> Result<(), NotificationError> {
        if !self.is_configured() {
            return Err(NotificationError::NoServicesConfigured);
        }

        let mut errors = Vec::new();

        if let Some(ref ntfy) = self.ntfy {
            if let Err(e) = ntfy.send(message).await {
                tracing::error!(error = %e, "Failed to send ntfy notification");
                errors.push(e);
            } else {
                tracing::info!("Sent notification via ntfy");
            }
        }

        if let Some(ref gotify) = self.gotify {
            if let Err(e) = gotify.send(message).await {
                tracing::error!(error = %e, "Failed to send gotify notification");
                errors.push(e);
            } else {
                tracing::info!("Sent notification via gotify");
            }
        }

        // Return success if at least one service succeeded
        if errors.len()
            < [self.ntfy.is_some(), self.gotify.is_some()]
                .iter()
                .filter(|&&x| x)
                .count()
        {
            Ok(())
        } else if let Some(e) = errors.into_iter().next() {
            Err(e)
        } else {
            Ok(())
        }
    }

    /// Send notification to ntfy only
    pub async fn send_ntfy(&self, message: &NotificationMessage) -> Result<(), NotificationError> {
        match &self.ntfy {
            Some(client) => client.send(message).await,
            None => Err(NotificationError::NoServicesConfigured),
        }
    }

    /// Send notification to gotify only
    pub async fn send_gotify(
        &self,
        message: &NotificationMessage,
    ) -> Result<(), NotificationError> {
        match &self.gotify {
            Some(client) => client.send(message).await,
            None => Err(NotificationError::NoServicesConfigured),
        }
    }
}
