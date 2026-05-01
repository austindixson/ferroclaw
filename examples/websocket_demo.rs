//! WebSocket server demonstration.
//!
//! Run this example to see the WebSocket server in action.
//!
//! The server will listen on ws://127.0.0.1:8420
//!
//! You can connect using a WebSocket client:
//! - wscat -c ws://127.0.0.1:8420
//! - Python: websokets library
//! - JavaScript: WebSocket API
//!
//! Example JavaScript client:
//! ```javascript
//! const ws = new WebSocket('ws://127.0.0.1:8420');
//! ws.onmessage = (event) => {
//!     const data = JSON.parse(event.data);
//!     console.log('Received:', data);
//! };
//! ```

use ferroclaw::websocket::{AgentState, ToolState, WsEvent, WsServer};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("websocket_demo=debug,ferroclaw=debug")
        .init();

    println!("Starting WebSocket server demo...");
    println!("Server will listen on ws://127.0.0.1:8420");
    println!();
    println!("Connect with a WebSocket client to see events:");
    println!("  wscat -c ws://127.0.0.1:8420");
    println!();

    // Create and start the WebSocket server
    let ws_server = WsServer::new("127.0.0.1".to_string(), 8420);
    let broadcaster = ws_server.broadcaster();

    // Spawn the server in the background
    tokio::spawn(async move {
        if let Err(e) = ws_server.start().await {
            eprintln!("WebSocket server error: {}", e);
        }
    });

    // Wait for server to start
    sleep(Duration::from_millis(100)).await;

    println!("WebSocket server is running!");
    println!("Broadcasting demo events every 2 seconds...");
    println!("Press Ctrl+C to stop");
    println!();

    // Simulate some agent activity
    let mut counter = 0u32;
    loop {
        counter += 1;

        // Agent state update
        let agent_id = format!("demo-agent-{}", counter % 3);
        let state = match counter % 4 {
            0 => AgentState::Idle,
            1 => AgentState::Thinking,
            2 => AgentState::Executing,
            _ => AgentState::Error,
        };

        let event = WsEvent::agent_state(agent_id, state);
        broadcaster.broadcast(event)?;
        println!("✓ Broadcasted agent state update");

        sleep(Duration::from_millis(500)).await;

        // Tool call start
        let call_id = format!("call-{}", counter);
        let tool_name = vec!["read_file", "write_file", "grep", "bash"][counter as usize % 4];
        let event = WsEvent::tool_start(
            call_id.clone(),
            tool_name.to_string(),
            serde_json::json!({"demo": "arguments"}),
        );
        broadcaster.broadcast(event)?;
        println!("✓ Broadcasted tool call start: {}", tool_name);

        sleep(Duration::from_millis(500)).await;

        // Tool output chunk
        let event = WsEvent::tool_chunk(
            call_id.clone(),
            format!("Output chunk from {}", tool_name),
            false,
        );
        broadcaster.broadcast(event)?;
        println!("✓ Broadcasted tool output chunk");

        sleep(Duration::from_millis(500)).await;

        // Tool completion
        let event = WsEvent::tool_update(
            call_id,
            if counter.is_multiple_of(3) {
                ToolState::Failed
            } else {
                ToolState::Completed
            },
        );
        broadcaster.broadcast(event)?;
        println!("✓ Broadcasted tool completion");

        println!("---");
        sleep(Duration::from_secs(2)).await;
    }
}
