//! Discord channel adapter.
//!
//! Uses the Discord HTTP API directly (no serenity/poise crate) to keep
//! binary size small. Connects via Gateway WebSocket for receiving messages
//! and REST API for sending.

use crate::channels::{Channel, OutgoingMessage};
use crate::config::DiscordConfig;
use crate::error::{FerroError, Result};
use std::future::Future;
use std::pin::Pin;

pub struct DiscordChannel {
    bot_token: String,
    allowed_guild_ids: Vec<u64>,
    command_prefix: String,
    client: reqwest::Client,
}

impl DiscordChannel {
    pub fn from_config(config: &DiscordConfig) -> Option<Self> {
        let bot_token = std::env::var(&config.bot_token_env).ok()?;
        Some(Self {
            bot_token,
            allowed_guild_ids: config.allowed_guild_ids.clone(),
            command_prefix: config.command_prefix.clone(),
            client: reqwest::Client::new(),
        })
    }

    /// Check if a guild ID is allowed.
    pub fn is_allowed(&self, guild_id: u64) -> bool {
        self.allowed_guild_ids.is_empty() || self.allowed_guild_ids.contains(&guild_id)
    }

    /// Send a message to a Discord channel via REST API.
    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<()> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages",
            channel_id
        );

        // Discord has a 2000 char limit per message; chunk if needed
        let chunks = chunk_message(content, 2000);

        for chunk in chunks {
            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.bot_token))
                .header("Content-Type", "application/json")
                .json(&serde_json::json!({ "content": chunk }))
                .send()
                .await
                .map_err(|e| FerroError::Channel(format!("Discord send error: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(FerroError::Channel(format!(
                    "Discord API error {status}: {body}"
                )));
            }
        }

        Ok(())
    }

    /// Reply to a specific message in a thread.
    pub async fn reply_message(
        &self,
        channel_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<()> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages",
            channel_id
        );

        let chunks = chunk_message(content, 2000);

        for (i, chunk) in chunks.iter().enumerate() {
            let mut body = serde_json::json!({ "content": chunk });
            // Only set message_reference on the first chunk
            if i == 0 {
                body["message_reference"] = serde_json::json!({
                    "message_id": message_id
                });
            }

            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.bot_token))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| FerroError::Channel(format!("Discord reply error: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(FerroError::Channel(format!(
                    "Discord API error {status}: {body}"
                )));
            }
        }

        Ok(())
    }

    pub fn command_prefix(&self) -> &str {
        &self.command_prefix
    }
}

impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }

    fn is_configured(&self) -> bool {
        !self.bot_token.is_empty()
    }

    fn send<'a>(
        &'a self,
        target: &'a str,
        message: OutgoingMessage,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let content = if message.is_error {
                format!("**Error:** {}", message.text)
            } else {
                message.text
            };

            match message.thread_id {
                Some(ref msg_id) => self.reply_message(target, msg_id, &content).await,
                None => self.send_message(target, &content).await,
            }
        })
    }
}

/// Split a message into chunks that fit within Discord's character limit.
pub fn chunk_message(content: &str, max_len: usize) -> Vec<String> {
    if content.len() <= max_len {
        return vec![content.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = content;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        // Try to split at a newline near the limit
        let split_at = remaining[..max_len].rfind('\n').unwrap_or(max_len);

        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start_matches('\n');
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_short_message() {
        let chunks = chunk_message("hello", 2000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "hello");
    }

    #[test]
    fn test_chunk_long_message() {
        let long = "a".repeat(5000);
        let chunks = chunk_message(&long, 2000);
        assert!(chunks.len() >= 3);
        for chunk in &chunks {
            assert!(chunk.len() <= 2000);
        }
    }

    #[test]
    fn test_allowlist_empty_allows_all() {
        let channel = DiscordChannel {
            bot_token: "test".into(),
            allowed_guild_ids: vec![],
            command_prefix: "!fc ".into(),
            client: reqwest::Client::new(),
        };
        assert!(channel.is_allowed(12345));
    }

    #[test]
    fn test_allowlist_enforced() {
        let channel = DiscordChannel {
            bot_token: "test".into(),
            allowed_guild_ids: vec![111, 222],
            command_prefix: "!fc ".into(),
            client: reqwest::Client::new(),
        };
        assert!(channel.is_allowed(111));
        assert!(!channel.is_allowed(999));
    }
}
