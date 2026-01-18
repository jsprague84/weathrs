use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Client;

use super::{NotificationError, NotificationMessage};

/// Authentication method for ntfy
#[derive(Clone)]
pub enum NtfyAuth {
    /// Bearer token authentication
    Token(String),
    /// Basic authentication (username, password)
    Basic { username: String, password: String },
}

/// Client for ntfy.sh push notifications
/// https://docs.ntfy.sh/publish/
pub struct NtfyClient {
    client: Client,
    url: String,
    topic: String,
    auth: Option<NtfyAuth>,
}

impl NtfyClient {
    pub fn new(client: Client, url: &str, topic: &str, auth: Option<NtfyAuth>) -> Self {
        Self {
            client,
            url: url.trim_end_matches('/').to_string(),
            topic: topic.to_string(),
            auth,
        }
    }

    /// Create with token authentication
    pub fn with_token(client: Client, url: &str, topic: &str, token: &str) -> Self {
        Self::new(client, url, topic, Some(NtfyAuth::Token(token.to_string())))
    }

    /// Create with basic authentication (username/password)
    pub fn with_basic_auth(
        client: Client,
        url: &str,
        topic: &str,
        username: &str,
        password: &str,
    ) -> Self {
        Self::new(
            client,
            url,
            topic,
            Some(NtfyAuth::Basic {
                username: username.to_string(),
                password: password.to_string(),
            }),
        )
    }

    pub async fn send(&self, message: &NotificationMessage) -> Result<(), NotificationError> {
        let url = format!("{}/{}", self.url, self.topic);

        tracing::debug!(url = %url, title = %message.title, "Sending ntfy notification");

        let mut request = self
            .client
            .post(&url)
            .header("Title", &message.title)
            .header("Priority", message.priority.as_ntfy_priority().to_string());

        // Add tags if present
        if !message.tags.is_empty() {
            request = request.header("Tags", message.tags.join(","));
        }

        // Add authentication if configured
        if let Some(ref auth) = self.auth {
            request = match auth {
                NtfyAuth::Token(token) => {
                    request.header("Authorization", format!("Bearer {}", token))
                }
                NtfyAuth::Basic { username, password } => {
                    let credentials = STANDARD.encode(format!("{}:{}", username, password));
                    request.header("Authorization", format!("Basic {}", credentials))
                }
            };
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

    fn test_client() -> Client {
        Client::new()
    }

    #[test]
    fn test_ntfy_client_creation() {
        let client = NtfyClient::new(test_client(), "https://ntfy.sh", "test-topic", None);
        assert_eq!(client.url, "https://ntfy.sh");
        assert_eq!(client.topic, "test-topic");
        assert!(client.auth.is_none());
    }

    #[test]
    fn test_ntfy_client_with_token() {
        let client =
            NtfyClient::with_token(test_client(), "https://ntfy.sh", "test-topic", "my-token");
        assert!(client.auth.is_some());
        assert!(matches!(client.auth, Some(NtfyAuth::Token(_))));
    }

    #[test]
    fn test_ntfy_client_with_basic_auth() {
        let client = NtfyClient::with_basic_auth(
            test_client(),
            "https://ntfy.sh",
            "test-topic",
            "myuser",
            "mypass",
        );
        assert!(client.auth.is_some());
        assert!(matches!(client.auth, Some(NtfyAuth::Basic { .. })));
    }
}
