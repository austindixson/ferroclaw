//! Integration tests for WebSocket server.

use ferroclaw::websocket::{AgentState, ToolState, WsBroadcaster, WsEvent};
use std::time::Duration;

#[test]
fn test_ws_event_agent_state_serialization() {
    let event = WsEvent::agent_state("test-agent-123".to_string(), AgentState::Thinking);
    let json = event.to_json().unwrap();

    assert!(json.contains("agent_state_update"));
    assert!(json.contains("test-agent-123"));
    assert!(json.contains("thinking"));
}

#[test]
fn test_ws_event_tool_start_serialization() {
    let event = WsEvent::tool_start(
        "call-456".to_string(),
        "read_file".to_string(),
        serde_json::json!({"path": "/tmp/test.txt"}),
    );
    let json = event.to_json().unwrap();

    assert!(json.contains("tool_call_start"));
    assert!(json.contains("read_file"));
    assert!(json.contains("/tmp/test.txt"));
}

#[test]
fn test_ws_event_tool_update_serialization() {
    let event = WsEvent::tool_update("call-789".to_string(), ToolState::Running);
    let json = event.to_json().unwrap();

    assert!(json.contains("tool_call_update"));
    assert!(json.contains("running"));
}

#[test]
fn test_ws_event_tool_chunk_serialization() {
    let event = WsEvent::tool_chunk("call-101".to_string(), "Output line 1\n".to_string(), false);
    let json = event.to_json().unwrap();

    assert!(json.contains("tool_output_chunk"));
    assert!(json.contains("Output line 1"));
    assert!(json.contains("\"is_final\":false"));
}

#[test]
fn test_ws_broadcaster_no_receivers() {
    let broadcaster = WsBroadcaster::new(10);

    // Broadcasting without receivers should not fail
    let event = WsEvent::agent_state("test".to_string(), AgentState::Idle);
    assert!(broadcaster.broadcast(event).is_ok());
}

#[tokio::test]
async fn test_ws_broadcaster_with_subscriber() {
    let broadcaster = WsBroadcaster::new(10);
    let mut rx = broadcaster.subscribe();

    // Spawn a task to receive events
    let handle = tokio::spawn(async move { rx.recv().await.unwrap() });

    // Broadcast an event
    let event = WsEvent::agent_state("test-agent".to_string(), AgentState::Executing);
    broadcaster.broadcast(event).unwrap();

    // Wait for receiver to get it
    let received = tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .unwrap()
        .unwrap();

    match received {
        WsEvent::AgentStateUpdate {
            agent_id, state, ..
        } => {
            assert_eq!(agent_id, "test-agent");
            assert_eq!(state, AgentState::Executing);
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn test_agent_state_equality() {
    assert_eq!(AgentState::Idle, AgentState::Idle);
    assert_ne!(AgentState::Thinking, AgentState::Executing);
}

#[test]
fn test_tool_state_equality() {
    assert_eq!(ToolState::Pending, ToolState::Pending);
    assert_ne!(ToolState::Running, ToolState::Completed);
}

#[test]
fn test_ws_event_final_chunk() {
    let event = WsEvent::tool_chunk("call-202".to_string(), "Final output".to_string(), true);
    let json = event.to_json().unwrap();

    assert!(json.contains("\"is_final\":true"));
    assert!(json.contains("Final output"));
}

#[tokio::test]
async fn test_multiple_broadcaster_subscribers() {
    let broadcaster = WsBroadcaster::new(10);

    // Multiple subscribers should all receive the same event
    let mut rx1 = broadcaster.subscribe();
    let mut rx2 = broadcaster.subscribe();

    let event = WsEvent::agent_state("test".to_string(), AgentState::Error);
    broadcaster.broadcast(event.clone()).unwrap();

    // Both should receive
    let recv1 = rx1.recv().await.unwrap();
    let recv2 = rx2.recv().await.unwrap();

    // Serialize both and compare
    assert_eq!(recv1.to_json().unwrap(), recv2.to_json().unwrap());
}
