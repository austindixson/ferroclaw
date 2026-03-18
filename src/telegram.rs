//! Telegram bot integration via raw Bot API + long polling.
//!
//! Modeled after Aetherclaw's Telegram channel:
//! - Long polling via getUpdates (no webhook exposure)
//! - Typing indicators while agent is processing
//! - Message splitting for >4096 char responses
//! - Chat ID allowlist enforcement
//! - /start, /help, /clear commands
//! - Markdown parse mode with plain-text fallback

use crate::agent::AgentLoop;
use crate::config::TelegramConfig;
use crate::error::{FerroError, Result};
use crate::types::Message;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

const TELEGRAM_API: &str = "https://api.telegram.org";
const MAX_MESSAGE_LEN: usize = 4096;
const POLL_TIMEOUT: u64 = 30;
const TYPING_INTERVAL_SECS: u64 = 4;

// ── Telegram API types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Update {
    update_id: i64,
    message: Option<TgMessage>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TgMessage {
    message_id: i64,
    from: Option<TgUser>,
    chat: TgChat,
    text: Option<String>,
    #[serde(default)]
    entities: Vec<TgEntity>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TgUser {
    id: i64,
    first_name: String,
    #[serde(default)]
    username: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TgChat {
    id: i64,
    #[serde(rename = "type")]
    chat_type: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TgEntity {
    #[serde(rename = "type")]
    entity_type: String,
    offset: usize,
    length: usize,
}

#[derive(Debug, Serialize)]
struct SendMessageParams {
    chat_id: i64,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to_message_id: Option<i64>,
}

// ── TelegramBot ─────────────────────────────────────────────────────────────

/// Telegram bot with full long-polling support.
pub struct TelegramBot {
    pub bot_token: String,
    pub allowed_chat_ids: Vec<i64>,
    client: Client,
    bot_username: Mutex<Option<String>>,
}

impl TelegramBot {
    pub fn from_config(config: &TelegramConfig) -> Option<Self> {
        let bot_token = std::env::var(&config.bot_token_env).ok()?;
        if bot_token.is_empty() {
            return None;
        }
        Some(Self {
            bot_token,
            allowed_chat_ids: config.allowed_chat_ids.clone(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(POLL_TIMEOUT + 10))
                .build()
                .unwrap_or_else(|_| Client::new()),
            bot_username: Mutex::new(None),
        })
    }

    /// Check if a chat ID is allowed to interact with the bot.
    pub fn is_allowed(&self, chat_id: i64) -> bool {
        self.allowed_chat_ids.is_empty() || self.allowed_chat_ids.contains(&chat_id)
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", TELEGRAM_API, self.bot_token, method)
    }

    /// Verify the bot token and get bot info.
    pub async fn get_me(&self) -> Result<String> {
        let resp: TelegramResponse<Value> = self
            .client
            .get(self.api_url("getMe"))
            .send()
            .await
            .map_err(|e| FerroError::Channel(format!("getMe failed: {e}")))?
            .json()
            .await
            .map_err(|e| FerroError::Channel(format!("getMe parse failed: {e}")))?;

        if !resp.ok {
            return Err(FerroError::Channel(format!(
                "getMe error: {}",
                resp.description.unwrap_or_default()
            )));
        }

        let username = resp
            .result
            .as_ref()
            .and_then(|r| r.get("username"))
            .and_then(|u| u.as_str())
            .unwrap_or("ferroclaw_bot")
            .to_string();

        *self.bot_username.lock().await = Some(username.clone());
        Ok(username)
    }

    /// Send a typing indicator to a chat.
    pub async fn send_typing(&self, chat_id: i64) {
        let _ = self
            .client
            .post(self.api_url("sendChatAction"))
            .json(&json!({"chat_id": chat_id, "action": "typing"}))
            .send()
            .await;
    }

    /// Send a text message. Returns the message_id on success.
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_to: Option<i64>,
    ) -> Result<i64> {
        // Try with Markdown first
        let params = SendMessageParams {
            chat_id,
            text: text.to_string(),
            parse_mode: Some("Markdown".into()),
            reply_to_message_id: reply_to,
        };

        let resp: TelegramResponse<Value> = self
            .client
            .post(self.api_url("sendMessage"))
            .json(&params)
            .send()
            .await
            .map_err(|e| FerroError::Channel(format!("sendMessage failed: {e}")))?
            .json()
            .await
            .map_err(|e| FerroError::Channel(format!("sendMessage parse failed: {e}")))?;

        if resp.ok {
            let msg_id = resp
                .result
                .as_ref()
                .and_then(|r| r.get("message_id"))
                .and_then(|id| id.as_i64())
                .unwrap_or(0);
            return Ok(msg_id);
        }

        // Markdown parse failed — retry as plain text
        let params = SendMessageParams {
            chat_id,
            text: text.to_string(),
            parse_mode: None,
            reply_to_message_id: reply_to,
        };

        let resp: TelegramResponse<Value> = self
            .client
            .post(self.api_url("sendMessage"))
            .json(&params)
            .send()
            .await
            .map_err(|e| FerroError::Channel(format!("sendMessage retry failed: {e}")))?
            .json()
            .await
            .map_err(|e| FerroError::Channel(format!("sendMessage retry parse failed: {e}")))?;

        if !resp.ok {
            return Err(FerroError::Channel(format!(
                "sendMessage error: {}",
                resp.description.unwrap_or_default()
            )));
        }

        Ok(resp
            .result
            .as_ref()
            .and_then(|r| r.get("message_id"))
            .and_then(|id| id.as_i64())
            .unwrap_or(0))
    }

    /// Send a long message, splitting at 4096 chars while preserving code blocks.
    pub async fn send_long_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_to: Option<i64>,
    ) -> Result<()> {
        let chunks = split_message(text, MAX_MESSAGE_LEN);
        for (i, chunk) in chunks.iter().enumerate() {
            let reply = if i == 0 { reply_to } else { None };
            self.send_message(chat_id, chunk, reply).await?;
        }
        Ok(())
    }

    /// Poll for updates using long polling.
    async fn get_updates(&self, offset: i64) -> Result<Vec<Update>> {
        let resp: TelegramResponse<Vec<Update>> = self
            .client
            .post(self.api_url("getUpdates"))
            .json(&json!({
                "offset": offset,
                "timeout": POLL_TIMEOUT,
                "allowed_updates": ["message"]
            }))
            .send()
            .await
            .map_err(|e| FerroError::Channel(format!("getUpdates failed: {e}")))?
            .json()
            .await
            .map_err(|e| FerroError::Channel(format!("getUpdates parse failed: {e}")))?;

        if !resp.ok {
            return Err(FerroError::Channel(format!(
                "getUpdates error: {}",
                resp.description.unwrap_or_default()
            )));
        }

        Ok(resp.result.unwrap_or_default())
    }

    /// Start the bot long-polling loop. Blocks until cancelled.
    pub async fn run(
        self: Arc<Self>,
        agent_loop: Arc<Mutex<AgentLoop>>,
        histories: Arc<Mutex<HashMap<i64, Vec<Message>>>>,
    ) -> Result<()> {
        // Verify token
        let username = self.get_me().await?;
        tracing::info!("Telegram bot connected: @{username}");

        let mut offset: i64 = 0;

        loop {
            let updates = match self.get_updates(offset).await {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!("Telegram poll error: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };

            for update in updates {
                offset = update.update_id + 1;

                let Some(msg) = update.message else {
                    continue;
                };
                let Some(text) = &msg.text else {
                    continue;
                };
                let text = text.trim().to_string();
                if text.is_empty() {
                    continue;
                }

                let chat_id = msg.chat.id;
                let user_name = msg
                    .from
                    .as_ref()
                    .map(|u| {
                        u.username
                            .clone()
                            .unwrap_or_else(|| u.first_name.clone())
                    })
                    .unwrap_or_else(|| "unknown".into());

                // Allowlist check
                if !self.is_allowed(chat_id) {
                    tracing::debug!("Telegram: rejected message from chat_id={chat_id} ({user_name})");
                    continue;
                }

                tracing::info!("Telegram [{chat_id}] @{user_name}: {text}");

                // Handle commands
                if text.starts_with('/') {
                    let handled = self.handle_command(chat_id, &text).await;
                    if handled {
                        // /clear needs to wipe history
                        if text == "/clear" || text.starts_with("/clear@") {
                            histories.lock().await.remove(&chat_id);
                        }
                        continue;
                    }
                }

                // Strip bot mention in group chats
                let clean_text = strip_bot_mention(&text, &username);

                // Process message through agent loop
                let bot = Arc::clone(&self);
                let agent = Arc::clone(&agent_loop);
                let hist = Arc::clone(&histories);

                tokio::spawn(async move {
                    // Start typing indicator
                    let typing_bot = Arc::clone(&bot);
                    let typing_chat = chat_id;
                    let typing_cancel = tokio_util::sync::CancellationToken::new();
                    let typing_token = typing_cancel.clone();

                    tokio::spawn(async move {
                        loop {
                            typing_bot.send_typing(typing_chat).await;
                            tokio::select! {
                                _ = tokio::time::sleep(std::time::Duration::from_secs(TYPING_INTERVAL_SECS)) => {}
                                _ = typing_token.cancelled() => { return; }
                            }
                        }
                    });

                    // Get or create conversation history for this chat
                    let mut histories = hist.lock().await;
                    let history = histories.entry(chat_id).or_insert_with(Vec::new);

                    // Run agent
                    let response = {
                        let mut agent = agent.lock().await;
                        agent.run(&clean_text, history).await
                    };

                    // Stop typing
                    typing_cancel.cancel();

                    // Send response
                    match response {
                        Ok((text, _events)) => {
                            if let Err(e) = bot.send_long_message(chat_id, &text, None).await {
                                tracing::error!("Telegram send error: {e}");
                            }
                        }
                        Err(e) => {
                            let err_msg = format!("Error: {e}");
                            let _ = bot.send_message(chat_id, &err_msg, None).await;
                        }
                    }
                });
            }
        }
    }

    /// Handle slash commands. Returns true if the command was handled.
    async fn handle_command(&self, chat_id: i64, text: &str) -> bool {
        let cmd = text.split_whitespace().next().unwrap_or("");
        let cmd = cmd.split('@').next().unwrap_or(cmd); // Strip @botname

        match cmd {
            "/start" => {
                let msg = concat!(
                    "🦀 *Ferroclaw* — Security-first AI agent\n\n",
                    "Send me any message and I'll respond using my AI capabilities.\n\n",
                    "*Commands:*\n",
                    "/help — Show this help\n",
                    "/clear — Clear conversation history\n",
                    "/status — Show bot status\n",
                );
                let _ = self.send_message(chat_id, msg, None).await;
                true
            }
            "/help" => {
                let msg = concat!(
                    "*Available commands:*\n\n",
                    "/start — Welcome message\n",
                    "/help — Show this help\n",
                    "/clear — Clear conversation history\n",
                    "/status — Show bot status\n\n",
                    "Send any message to chat with the AI agent. ",
                    "I can read files, run commands, search the web, and more.",
                );
                let _ = self.send_message(chat_id, msg, None).await;
                true
            }
            "/clear" => {
                let _ = self
                    .send_message(chat_id, "Conversation cleared.", None)
                    .await;
                true
            }
            "/status" => {
                let username = self
                    .bot_username
                    .lock()
                    .await
                    .clone()
                    .unwrap_or_else(|| "unknown".into());
                let msg = format!(
                    "*Ferroclaw Status*\n\nBot: @{username}\nVersion: {}\nChat ID: `{chat_id}`",
                    env!("CARGO_PKG_VERSION"),
                );
                let _ = self.send_message(chat_id, &msg, None).await;
                true
            }
            _ => false, // Not a known command
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Strip @botname mentions from message text.
fn strip_bot_mention(text: &str, bot_username: &str) -> String {
    let mention = format!("@{bot_username}");
    text.replace(&mention, "").trim().to_string()
}

/// Split a long message into chunks of at most `max_len` characters.
/// Preserves fenced code blocks when possible.
pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        // Try to split at a natural boundary
        let chunk_end = find_split_point(remaining, max_len);
        let (chunk, rest) = remaining.split_at(chunk_end);
        chunks.push(chunk.trim_end().to_string());
        remaining = rest.trim_start();
    }

    chunks
}

/// Find the best split point within max_len, preferring line breaks.
fn find_split_point(text: &str, max_len: usize) -> usize {
    let search_region = &text[..max_len];

    // Prefer splitting at double newline (paragraph break)
    if let Some(pos) = search_region.rfind("\n\n") {
        if pos > max_len / 2 {
            return pos + 1;
        }
    }

    // Then single newline
    if let Some(pos) = search_region.rfind('\n') {
        if pos > max_len / 2 {
            return pos + 1;
        }
    }

    // Then space
    if let Some(pos) = search_region.rfind(' ') {
        if pos > max_len / 2 {
            return pos + 1;
        }
    }

    // Hard cut as last resort
    max_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_allowlist() {
        let bot = TelegramBot {
            bot_token: "test".into(),
            allowed_chat_ids: vec![123, 456],
            client: Client::new(),
            bot_username: Mutex::new(None),
        };
        assert!(bot.is_allowed(123));
        assert!(!bot.is_allowed(789));
    }

    #[test]
    fn test_telegram_empty_allowlist_allows_all() {
        let bot = TelegramBot {
            bot_token: "test".into(),
            allowed_chat_ids: vec![],
            client: Client::new(),
            bot_username: Mutex::new(None),
        };
        assert!(bot.is_allowed(123));
        assert!(bot.is_allowed(999));
    }

    #[test]
    fn test_strip_bot_mention() {
        assert_eq!(
            strip_bot_mention("@ferrobot hello there", "ferrobot"),
            "hello there"
        );
        assert_eq!(
            strip_bot_mention("hello @ferrobot there", "ferrobot"),
            "hello  there"
        );
        assert_eq!(strip_bot_mention("no mention", "ferrobot"), "no mention");
    }

    #[test]
    fn test_split_message_short() {
        let chunks = split_message("hello", 4096);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn test_split_message_long() {
        let text = "a".repeat(5000);
        let chunks = split_message(&text, 4096);
        assert!(chunks.len() >= 2);
        assert!(chunks[0].len() <= 4096);
    }

    #[test]
    fn test_split_message_at_newline() {
        let mut text = "a".repeat(3000);
        text.push('\n');
        text.push_str(&"b".repeat(3000));
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].ends_with('a'));
        assert!(chunks[1].starts_with('b'));
    }

    #[test]
    fn test_api_url() {
        let bot = TelegramBot {
            bot_token: "123:ABC".into(),
            allowed_chat_ids: vec![],
            client: Client::new(),
            bot_username: Mutex::new(None),
        };
        assert_eq!(
            bot.api_url("sendMessage"),
            "https://api.telegram.org/bot123:ABC/sendMessage"
        );
    }
}
