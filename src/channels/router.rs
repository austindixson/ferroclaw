//! Channel router — dispatches messages between channels and the agent loop.
//!
//! The router:
//! 1. Collects configured channels at startup
//! 2. Routes incoming messages from any channel to the agent
//! 3. Sends agent responses back through the originating channel

use crate::channels::{Channel, ChannelStatus, IncomingMessage, OutgoingMessage};
use crate::config::Config;
use crate::error::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// Central message router across all configured channels.
pub struct ChannelRouter {
    channels: HashMap<String, Arc<dyn Channel>>,
    message_counts: HashMap<String, u64>,
}

impl ChannelRouter {
    /// Build a router from the config, initializing all configured channels.
    pub fn from_config(config: &Config) -> Self {
        let mut channels: HashMap<String, Arc<dyn Channel>> = HashMap::new();

        // Telegram (from existing config)
        if let Some(ref tg_config) = config.telegram {
            if let Some(bot) = crate::telegram::TelegramBot::from_config(tg_config) {
                tracing::info!("Telegram channel configured");
                channels.insert(
                    "telegram".into(),
                    Arc::new(TelegramChannelAdapter(Arc::new(bot))),
                );
            }
        }

        // Discord
        if let Some(ref discord_config) = config.channels.discord {
            if let Some(ch) = crate::channels::discord::DiscordChannel::from_config(discord_config)
            {
                tracing::info!("Discord channel configured");
                channels.insert("discord".into(), Arc::new(ch));
            }
        }

        // Slack
        if let Some(ref slack_config) = config.channels.slack {
            if let Some(ch) = crate::channels::slack::SlackChannel::from_config(slack_config) {
                tracing::info!("Slack channel configured");
                channels.insert("slack".into(), Arc::new(ch));
            }
        }

        // WhatsApp
        if let Some(ref wa_config) = config.channels.whatsapp {
            if let Some(ch) = crate::channels::whatsapp::WhatsAppChannel::from_config(wa_config) {
                tracing::info!("WhatsApp channel configured");
                channels.insert("whatsapp".into(), Arc::new(ch));
            }
        }

        // Signal
        if let Some(ref signal_config) = config.channels.signal {
            let ch = crate::channels::signal::SignalChannel::from_config(signal_config);
            tracing::info!("Signal channel configured");
            channels.insert("signal".into(), Arc::new(ch));
        }

        // Email
        if let Some(ref email_config) = config.channels.email {
            if let Some(ch) = crate::channels::email::EmailChannel::from_config(email_config) {
                tracing::info!("Email channel configured");
                channels.insert("email".into(), Arc::new(ch));
            }
        }

        // Home Assistant
        if let Some(ref ha_config) = config.channels.homeassistant {
            if let Some(ch) =
                crate::channels::homeassistant::HomeAssistantChannel::from_config(ha_config)
            {
                tracing::info!("Home Assistant channel configured");
                channels.insert("homeassistant".into(), Arc::new(ch));
            }
        }

        let message_counts = channels.keys().map(|k| (k.clone(), 0u64)).collect();

        Self {
            channels,
            message_counts,
        }
    }

    /// Get a channel by name.
    pub fn get_channel(&self, name: &str) -> Option<&Arc<dyn Channel>> {
        self.channels.get(name)
    }

    /// Send a response through the channel that originated the message.
    pub async fn send_response(
        &self,
        incoming: &IncomingMessage,
        response: OutgoingMessage,
    ) -> Result<()> {
        let channel = self
            .channels
            .get(&incoming.channel)
            .ok_or_else(|| {
                crate::error::FerroError::Channel(format!(
                    "No channel '{}' configured",
                    incoming.channel
                ))
            })?;

        channel.send(&incoming.sender_id, response).await
    }

    /// List all configured channels and their status.
    pub fn status(&self) -> Vec<ChannelStatus> {
        self.channels
            .iter()
            .map(|(name, ch)| ChannelStatus {
                name: name.clone(),
                connected: ch.is_configured(),
                message_count: *self.message_counts.get(name).unwrap_or(&0),
                last_activity: None,
            })
            .collect()
    }

    /// List names of all configured channels.
    pub fn channel_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.channels.keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of configured channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

/// Adapter to wrap existing TelegramBot as a Channel.
pub struct TelegramChannelAdapter(pub Arc<crate::telegram::TelegramBot>);

impl Channel for TelegramChannelAdapter {
    fn name(&self) -> &str {
        "telegram"
    }

    fn is_configured(&self) -> bool {
        !self.0.bot_token.is_empty()
    }

    fn send<'a>(
        &'a self,
        target: &'a str,
        message: OutgoingMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let chat_id: i64 = target
                .parse()
                .map_err(|_| crate::error::FerroError::Channel(format!("Invalid chat_id: {target}")))?;
            self.0.send_long_message(chat_id, &message.text, None).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_default_config() {
        let config = Config::default();
        let router = ChannelRouter::from_config(&config);
        // No channels configured by default
        assert_eq!(router.channel_count(), 0);
    }

    #[test]
    fn test_router_status() {
        let config = Config::default();
        let router = ChannelRouter::from_config(&config);
        let status = router.status();
        assert!(status.is_empty());
    }
}
