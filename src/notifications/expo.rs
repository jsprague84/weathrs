use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{NotificationError, NotificationMessage, Priority};

const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";

/// Expo push notification client
pub struct ExpoClient {
    client: Client,
}

/// Expo push message format
#[derive(Debug, Serialize)]
pub struct ExpoPushMessage {
    /// Expo push token
    pub to: String,

    /// Notification title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Notification body
    pub body: String,

    /// Custom data payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Priority: default, normal, high
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,

    /// Sound: default or null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,

    /// Badge count for iOS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<i32>,

    /// Android channel ID
    #[serde(rename = "channelId", skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,

    /// TTL in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<i32>,
}

/// Response from Expo push API
#[derive(Debug, Deserialize)]
pub struct ExpoPushResponse {
    pub data: Vec<ExpoPushTicket>,
}

/// Individual push ticket
#[derive(Debug, Deserialize)]
pub struct ExpoPushTicket {
    pub status: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub details: Option<serde_json::Value>,
}

impl ExpoClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Convert our priority to Expo priority
    fn convert_priority(priority: Priority) -> String {
        match priority {
            Priority::Min | Priority::Low => "normal".to_string(),
            Priority::Default => "default".to_string(),
            Priority::High | Priority::Urgent => "high".to_string(),
        }
    }

    /// Send a push notification to a single device
    pub async fn send_to_token(
        &self,
        token: &str,
        message: &NotificationMessage,
    ) -> Result<ExpoPushTicket, NotificationError> {
        let push_message = ExpoPushMessage {
            to: token.to_string(),
            title: Some(message.title.clone()),
            body: message.body.clone(),
            data: None,
            priority: Some(Self::convert_priority(message.priority)),
            sound: Some("default".to_string()),
            badge: None,
            channel_id: Some("weather".to_string()),
            ttl: Some(3600), // 1 hour
        };

        let response = self
            .client
            .post(EXPO_PUSH_URL)
            .header("Accept", "application/json")
            .header("Accept-Encoding", "gzip, deflate")
            .header("Content-Type", "application/json")
            .json(&push_message)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(NotificationError::ServiceError(format!(
                "Expo API returned {}: {}",
                status, body
            )));
        }

        let push_response: ExpoPushResponse = response.json().await?;

        if let Some(ticket) = push_response.data.into_iter().next() {
            if ticket.status == "ok" {
                tracing::info!(
                    ticket_id = ?ticket.id,
                    "Successfully sent Expo push notification"
                );
                Ok(ticket)
            } else {
                let error_msg = ticket
                    .message
                    .unwrap_or_else(|| "Unknown error".to_string());
                tracing::error!(
                    status = %ticket.status,
                    message = %error_msg,
                    "Expo push notification failed"
                );
                Err(NotificationError::ServiceError(error_msg))
            }
        } else {
            Err(NotificationError::ServiceError(
                "No ticket in Expo response".to_string(),
            ))
        }
    }

    /// Send push notifications to multiple devices
    pub async fn send_to_tokens(
        &self,
        tokens: &[String],
        message: &NotificationMessage,
    ) -> Vec<Result<ExpoPushTicket, NotificationError>> {
        let mut results = Vec::with_capacity(tokens.len());

        // Expo recommends batching up to 100 notifications
        for chunk in tokens.chunks(100) {
            let messages: Vec<ExpoPushMessage> = chunk
                .iter()
                .map(|token| ExpoPushMessage {
                    to: token.clone(),
                    title: Some(message.title.clone()),
                    body: message.body.clone(),
                    data: None,
                    priority: Some(Self::convert_priority(message.priority)),
                    sound: Some("default".to_string()),
                    badge: None,
                    channel_id: Some("weather".to_string()),
                    ttl: Some(3600),
                })
                .collect();

            match self
                .client
                .post(EXPO_PUSH_URL)
                .header("Accept", "application/json")
                .header("Accept-Encoding", "gzip, deflate")
                .header("Content-Type", "application/json")
                .json(&messages)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<ExpoPushResponse>().await {
                            Ok(push_response) => {
                                for ticket in push_response.data {
                                    if ticket.status == "ok" {
                                        results.push(Ok(ticket));
                                    } else {
                                        let error_msg = ticket
                                            .message
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        results
                                            .push(Err(NotificationError::ServiceError(error_msg)));
                                    }
                                }
                            }
                            Err(e) => {
                                let error_msg = e.to_string();
                                for _ in chunk {
                                    results.push(Err(NotificationError::ServiceError(
                                        error_msg.clone(),
                                    )));
                                }
                            }
                        }
                    } else {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        let error = NotificationError::ServiceError(format!(
                            "Expo API returned {}: {}",
                            status, body
                        ));
                        for _ in chunk {
                            results.push(Err(NotificationError::ServiceError(format!(
                                "Expo API returned {}: {}",
                                status, body
                            ))));
                        }
                        tracing::error!(%error, "Batch push failed");
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    for _ in chunk {
                        results.push(Err(NotificationError::ServiceError(error_msg.clone())));
                    }
                }
            }
        }

        let success_count = results.iter().filter(|r| r.is_ok()).count();
        tracing::info!(
            total = tokens.len(),
            success = success_count,
            "Sent batch push notifications"
        );

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_conversion() {
        assert_eq!(ExpoClient::convert_priority(Priority::Min), "normal");
        assert_eq!(ExpoClient::convert_priority(Priority::Low), "normal");
        assert_eq!(ExpoClient::convert_priority(Priority::Default), "default");
        assert_eq!(ExpoClient::convert_priority(Priority::High), "high");
        assert_eq!(ExpoClient::convert_priority(Priority::Urgent), "high");
    }
}
