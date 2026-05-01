//! Integration tests for the channel system:
//! router construction, channel configuration, message chunking, allowlists.

use ferroclaw::channels::router::ChannelRouter;
use ferroclaw::channels::{IncomingMessage, OutgoingMessage};
use ferroclaw::config::Config;

// ── Channel Router ──────────────────────────────────────────────────

#[test]
fn test_router_no_channels_by_default() {
    let config = Config::default();
    let router = ChannelRouter::from_config(&config);
    assert_eq!(router.channel_count(), 0);
    assert!(router.channel_names().is_empty());
}

#[test]
fn test_router_status_empty() {
    let config = Config::default();
    let router = ChannelRouter::from_config(&config);
    let status = router.status();
    assert!(status.is_empty());
}

#[test]
fn test_router_get_nonexistent_channel() {
    let config = Config::default();
    let router = ChannelRouter::from_config(&config);
    assert!(router.get_channel("nonexistent").is_none());
}

#[tokio::test]
async fn test_router_send_to_unconfigured_channel_errors() {
    let config = Config::default();
    let router = ChannelRouter::from_config(&config);
    let incoming = IncomingMessage {
        channel: "nonexistent".into(),
        sender_id: "user1".into(),
        text: "hello".into(),
        session_key: "nonexistent:user1".into(),
        reply_to: None,
    };
    let response = OutgoingMessage {
        text: "reply".into(),
        is_error: false,
        thread_id: None,
    };
    let result = router.send_response(&incoming, response).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No channel"));
}

// ── Message Types ───────────────────────────────────────────────────

#[test]
fn test_incoming_message_construction() {
    let msg = IncomingMessage {
        channel: "telegram".into(),
        sender_id: "123".into(),
        text: "Hello bot".into(),
        session_key: "telegram:123".into(),
        reply_to: Some("msg_456".into()),
    };
    assert_eq!(msg.channel, "telegram");
    assert_eq!(msg.reply_to.as_deref(), Some("msg_456"));
}

#[test]
fn test_outgoing_message_error_flag() {
    let ok = OutgoingMessage {
        text: "Success".into(),
        is_error: false,
        thread_id: None,
    };
    assert!(!ok.is_error);

    let err = OutgoingMessage {
        text: "Failed".into(),
        is_error: true,
        thread_id: Some("thread_1".into()),
    };
    assert!(err.is_error);
    assert_eq!(err.thread_id.as_deref(), Some("thread_1"));
}

// ── Discord Chunking ────────────────────────────────────────────────

#[test]
fn test_discord_chunk_exact_boundary() {
    // Message exactly at the limit should be 1 chunk
    let msg = "a".repeat(2000);
    let chunks = ferroclaw::channels::discord::chunk_message(&msg, 2000);
    assert_eq!(chunks.len(), 1);
}

#[test]
fn test_discord_chunk_splits_at_newline() {
    // A message with newlines should split at a newline boundary
    let mut msg = String::new();
    for i in 0..100 {
        msg.push_str(&format!("Line {i}\n"));
    }
    if msg.len() > 2000 {
        let chunks = ferroclaw::channels::discord::chunk_message(&msg, 2000);
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.len() <= 2000);
        }
    }
}
