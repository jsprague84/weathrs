mod expo;
mod gotify;
mod ntfy;

pub use expo::ExpoClient;
pub use gotify::GotifyClient;
pub use ntfy::{NtfyAuth, NtfyClient};

use reqwest::Client;
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

/// Configuration for building NotificationService
pub struct NotificationServiceConfig<'a> {
    pub client: Client,
    pub ntfy_url: Option<&'a str>,
    pub ntfy_topic: Option<&'a str>,
    pub ntfy_token: Option<&'a str>,
    pub ntfy_username: Option<&'a str>,
    pub ntfy_password: Option<&'a str>,
    pub gotify_url: Option<&'a str>,
    pub gotify_token: Option<&'a str>,
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

    pub fn from_config(config: NotificationServiceConfig<'_>) -> Self {
        let ntfy = match (config.ntfy_url, config.ntfy_topic) {
            (Some(url), Some(topic)) => {
                // Determine auth method: prefer token, then basic auth, then none
                let auth = if let Some(token) = config.ntfy_token {
                    Some(NtfyAuth::Token(token.to_string()))
                } else if let (Some(username), Some(password)) =
                    (config.ntfy_username, config.ntfy_password)
                {
                    Some(NtfyAuth::Basic {
                        username: username.to_string(),
                        password: password.to_string(),
                    })
                } else {
                    None
                };
                Some(NtfyClient::new(config.client.clone(), url, topic, auth))
            }
            _ => None,
        };

        let gotify = match (config.gotify_url, config.gotify_token) {
            (Some(url), Some(token)) => Some(GotifyClient::new(config.client, url, token)),
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
