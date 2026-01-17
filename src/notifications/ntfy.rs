use reqwest::Client;
use std::time::Duration;

use super::{NotificationError, NotificationMessage};

/// Client for ntfy.sh push notifications
/// https://docs.ntfy.sh/publish/
pub struct NtfyClient {
    client: Client,
    url: String,
    topic: String,
    token: Option<String>,
}

impl NtfyClient {
    pub fn new(url: &str, topic: &str, token: Option<&str>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            url: url.trim_end_matches('/').to_string(),
            topic: topic.to_string(),
            token: token.map(|t| t.to_string()),
        }
    }

    pub async fn send(&self, message: &NotificationMessage) -> Result<(), NotificationError> {
        let url = format!("{}/{}", self.url, self.topic);

        tracing::debug!(url = %url, title = %message.title, "Sending ntfy notification");

        let mut request = self
            .client
            .post(&url)
            .header("Title", &message.title)
            .header("Priority", message.priority.to_ntfy_priority().to_string());

        // Add tags if present
        if !message.tags.is_empty() {
            request = request.header("Tags", message.tags.join(","));
        }

        // Add auth token if configured
        if let Some(ref token) = self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.body(message.body.clone()).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(NotificationError::ServiceError(format!(
                "ntfy returned {}: {}",
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
    fn test_ntfy_client_creation() {
        let client = NtfyClient::new("https://ntfy.sh", "test-topic", None);
        assert_eq!(client.url, "https://ntfy.sh");
        assert_eq!(client.topic, "test-topic");
    }

    #[test]
    fn test_ntfy_client_with_token() {
        let client = NtfyClient::new("https://ntfy.sh", "test-topic", Some("my-token"));
        assert!(client.token.is_some());
    }
}
