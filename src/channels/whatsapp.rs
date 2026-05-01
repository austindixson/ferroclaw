//! WhatsApp channel adapter.
//!
//! Uses the WhatsApp Business Cloud API (Meta Graph API) for sending and
//! receiving messages. Requires a Meta Business account and WhatsApp Business API access.

use crate::channels::{Channel, OutgoingMessage};
use crate::config::WhatsAppConfig;
use crate::error::{FerroError, Result};
use std::future::Future;
use std::pin::Pin;

pub struct WhatsAppChannel {
    api_token: String,
    phone_number_id: String,
    webhook_verify_token: Option<String>,
    allowed_numbers: Vec<String>,
    client: reqwest::Client,
}

impl WhatsAppChannel {
    pub fn from_config(config: &WhatsAppConfig) -> Option<Self> {
        let api_token = std::env::var(&config.api_token_env).ok()?;

        Some(Self {
            api_token,
            phone_number_id: config.phone_number_id.clone(),
            webhook_verify_token: config.webhook_verify_token.clone(),
            allowed_numbers: config.allowed_numbers.clone(),
            client: reqwest::Client::new(),
        })
    }

    /// Check if a phone number is allowed.
    pub fn is_allowed(&self, phone_number: &str) -> bool {
        self.allowed_numbers.is_empty() || self.allowed_numbers.contains(&phone_number.to_string())
    }

    /// Verify webhook challenge from Meta (for webhook registration).
    pub fn verify_webhook(&self, mode: &str, token: &str, challenge: &str) -> Option<String> {
        if mode == "subscribe"
            && let Some(ref verify_token) = self.webhook_verify_token
            && token == verify_token
        {
            return Some(challenge.to_string());
        }
        None
    }

    /// Send a text message via WhatsApp Business Cloud API.
    pub async fn send_text(&self, to: &str, text: &str) -> Result<()> {
        let url = format!(
            "https://graph.facebook.com/v21.0/{}/messages",
            self.phone_number_id
        );

        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "text",
            "text": {
                "body": text
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| FerroError::Channel(format!("WhatsApp send error: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FerroError::Channel(format!(
                "WhatsApp API error {status}: {body}"
            )));
        }

        Ok(())
    }

    /// Mark a message as read.
    pub async fn mark_read(&self, message_id: &str) -> Result<()> {
        let url = format!(
            "https://graph.facebook.com/v21.0/{}/messages",
            self.phone_number_id
        );

        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "status": "read",
            "message_id": message_id
        });

        self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| FerroError::Channel(format!("WhatsApp mark-read error: {e}")))?;

        Ok(())
    }
}

impl Channel for WhatsAppChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    fn is_configured(&self) -> bool {
        !self.api_token.is_empty() && !self.phone_number_id.is_empty()
    }

    fn send<'a>(
        &'a self,
        target: &'a str,
        message: OutgoingMessage,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let text = if message.is_error {
                format!("[Error] {}", message.text)
            } else {
                message.text
            };

            // WhatsApp has a 4096 char limit; truncate if needed
            let text = if text.len() > 4096 {
                format!("{}...\n[Message truncated]", &text[..4000])
            } else {
                text
            };

            self.send_text(target, &text).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowlist() {
        let channel = WhatsAppChannel {
            api_token: "test".into(),
            phone_number_id: "123".into(),
            webhook_verify_token: None,
            allowed_numbers: vec!["+1111111111".into()],
            client: reqwest::Client::new(),
        };
        assert!(channel.is_allowed("+1111111111"));
        assert!(!channel.is_allowed("+9999999999"));
    }

    #[test]
    fn test_webhook_verify() {
        let channel = WhatsAppChannel {
            api_token: "test".into(),
            phone_number_id: "123".into(),
            webhook_verify_token: Some("mytoken".into()),
            allowed_numbers: vec![],
            client: reqwest::Client::new(),
        };
        assert_eq!(
            channel.verify_webhook("subscribe", "mytoken", "challenge123"),
            Some("challenge123".into())
        );
        assert_eq!(
            channel.verify_webhook("subscribe", "wrong", "challenge123"),
            None
        );
    }
}
