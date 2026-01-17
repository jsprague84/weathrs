use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

use super::{NotificationError, NotificationMessage};

/// Client for Gotify push notifications
/// https://gotify.net/docs/pushmsg
pub struct GotifyClient {
    client: Client,
    url: String,
    token: String,
}

#[derive(Serialize)]
struct GotifyMessage {
    title: String,
    message: String,
    priority: u8,
}

impl GotifyClient {
    pub fn new(url: &str, token: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            url: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    pub async fn send(&self, message: &NotificationMessage) -> Result<(), NotificationError> {
        let url = format!("{}/message?token={}", self.url, self.token);

        tracing::debug!(title = %message.title, "Sending gotify notification");

        let gotify_msg = GotifyMessage {
            title: message.title.clone(),
            message: message.body.clone(),
            priority: message.priority.to_gotify_priority(),
        };

        let response = self.client.post(&url).json(&gotify_msg).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(NotificationError::ServiceError(format!(
                "gotify returned {}: {}",
                status, body
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gotify_client_creation() {
        let client = GotifyClient::new("https://gotify.example.com", "my-app-token");
        assert_eq!(client.url, "https://gotify.example.com");
        assert_eq!(client.token, "my-app-token");
    }
}
