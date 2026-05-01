//! Integration tests for core types: messages, tool calls, context management.

use ferroclaw::agent::context::ContextManager;
use ferroclaw::types::{Capability, CapabilitySet, Message, ToolCall, ToolDefinition};
use serde_json::json;

#[test]
fn test_message_constructors() {
    let sys = Message::system("You are helpful.");
    assert_eq!(sys.role, ferroclaw::types::Role::System);
    assert_eq!(sys.text(), "You are helpful.");

    let user = Message::user("Hello");
    assert_eq!(user.role, ferroclaw::types::Role::User);

    let asst = Message::assistant("Hi there!");
    assert_eq!(asst.role, ferroclaw::types::Role::Assistant);
    assert!(asst.tool_calls.is_none());

    let tr = Message::tool_result("tc_1", "file contents");
    assert_eq!(tr.role, ferroclaw::types::Role::Tool);
    assert_eq!(tr.tool_call_id.as_deref(), Some("tc_1"));
}

#[test]
fn test_message_with_tool_calls() {
    let tcs = vec![
        ToolCall {
            id: "tc_1".into(),
            name: "read_file".into(),
            arguments: json!({"path": "/tmp/test"}),
        },
        ToolCall {
            id: "tc_2".into(),
            name: "list_directory".into(),
            arguments: json!({"path": "/tmp"}),
        },
    ];
    let msg = Message::assistant_with_tool_calls(tcs);
    let tool_calls = msg.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].name, "read_file");
    assert_eq!(tool_calls[1].name, "list_directory");
}

#[test]
fn test_message_token_estimation() {
    let short = Message::user("Hi");
    let long = Message::user("word ".repeat(1000));

    let short_tokens = short.estimated_tokens();
    let long_tokens = long.estimated_tokens();

    assert!(short_tokens < long_tokens);
    assert!(short_tokens >= 1);
    // "word " * 1000 = 5000 chars ≈ 1250 tokens
    assert!(long_tokens > 1000);
}

#[test]
fn test_tool_definition_compact_signature() {
    let tool = ToolDefinition {
        name: "edit_file".into(),
        description: "Edit a file".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "edits": {"type": "array", "items": {"type": "object"}},
                "dryRun": {"type": "boolean"}
            },
            "required": ["path", "edits"]
        }),
        server_name: None,
    };

    let sig = tool.compact_signature();
    assert!(sig.starts_with("edit_file("));
    assert!(sig.contains("path: str"));
    assert!(sig.contains("edits: list[dict]"));
    assert!(sig.contains("?dryRun: bool"));
}

#[test]
fn test_tool_definition_enum_type_hint() {
    let tool = ToolDefinition {
        name: "set_mode".into(),
        description: "Set mode".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["fast", "balanced", "thorough"]
                }
            },
            "required": ["mode"]
        }),
        server_name: None,
    };
    let sig = tool.compact_signature();
    assert!(sig.contains("\"fast\""));
    assert!(sig.contains("\"balanced\""));
    assert!(sig.contains("\"thorough\""));
}

#[test]
fn test_tool_definition_required_params() {
    let tool = ToolDefinition {
        name: "search".into(),
        description: "Search".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer"}
            },
            "required": ["query"]
        }),
        server_name: None,
    };
    let required = tool.required_params();
    assert_eq!(required, vec!["query"]);
}

// ── Context Manager ─────────────────────────────────────────────────

#[test]
fn test_context_manager_budget_tracking() {
    let mut ctx = ContextManager::new(10_000);
    assert_eq!(ctx.remaining(), 10_000);
    assert_eq!(ctx.usage_fraction(), 0.0);

    ctx.record_usage(3000, 1000);
    assert_eq!(ctx.remaining(), 6_000);
    assert!((ctx.usage_fraction() - 0.4).abs() < 0.001);

    ctx.record_usage(6000, 0);
    assert_eq!(ctx.remaining(), 0);
    assert!((ctx.usage_fraction() - 1.0).abs() < 0.001);
}

#[test]
fn test_context_manager_prune_preserves_system() {
    let ctx = ContextManager::new(50); // Very small budget
    let mut msgs = vec![
        Message::system("You are Ferroclaw."),
        Message::user("Message 1"),
        Message::assistant("Response 1"),
    ];
    // Add many messages to exceed budget
    for i in 2..20 {
        msgs.push(Message::user(format!(
            "Long message number {i} with padding text"
        )));
        msgs.push(Message::assistant(format!(
            "Long response number {i} with more padding"
        )));
    }

    let original_len = msgs.len();
    ctx.prune_to_fit(&mut msgs);

    // System message must survive
    assert_eq!(msgs[0].role, ferroclaw::types::Role::System);
    assert!(msgs[0].text().contains("Ferroclaw"));
    // Should have fewer messages
    assert!(msgs.len() < original_len);
}

#[test]
fn test_context_manager_no_prune_when_under_budget() {
    let ctx = ContextManager::new(1_000_000);
    let mut msgs = vec![
        Message::system("System"),
        Message::user("Hello"),
        Message::assistant("Hi"),
    ];
    let original_len = msgs.len();
    ctx.prune_to_fit(&mut msgs);
    assert_eq!(msgs.len(), original_len);
}

#[test]
fn test_context_manager_would_exceed() {
    let ctx = ContextManager::new(100);
    let msgs = vec![Message::user("x".repeat(300))]; // Already ~75 tokens
    let new_msg = Message::user("y".repeat(200)); // ~50 more tokens
    assert!(ctx.would_exceed(&msgs, &new_msg));
}

// ── Capability Set ──────────────────────────────────────────────────

#[test]
fn test_capability_display() {
    assert_eq!(format!("{}", Capability::FsRead), "fs_read");
    assert_eq!(format!("{}", Capability::NetOutbound), "net_outbound");
    assert_eq!(format!("{}", Capability::ProcessExec), "process_exec");
}

#[test]
fn test_capability_set_check_multiple() {
    let set = CapabilitySet::new([
        Capability::FsRead,
        Capability::FsWrite,
        Capability::NetOutbound,
    ]);
    // Check multiple at once
    assert!(
        set.check(&[Capability::FsRead, Capability::FsWrite])
            .is_ok()
    );
    assert!(
        set.check(&[Capability::FsRead, Capability::ProcessExec])
            .is_err()
    );
}

#[test]
fn test_capability_serialization() {
    let cap = Capability::FsRead;
    let json = serde_json::to_string(&cap).unwrap();
    assert_eq!(json, "\"fs_read\"");
    let deserialized: Capability = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, Capability::FsRead);
}
