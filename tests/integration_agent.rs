//! Integration tests for the agent loop and context manager:
//! token budget tracking, context pruning, event emission.

use ferroclaw::agent::context::ContextManager;
use ferroclaw::types::Message;

// ── Context Manager Advanced ────────────────────────────────────────

#[test]
fn test_context_manager_usage_fraction_zero_budget() {
    let ctx = ContextManager::new(0);
    assert_eq!(ctx.usage_fraction(), 1.0);
    assert_eq!(ctx.remaining(), 0);
}

#[test]
fn test_context_manager_record_multiple_usages() {
    let mut ctx = ContextManager::new(100_000);
    ctx.record_usage(5000, 2000);
    ctx.record_usage(3000, 1000);
    ctx.record_usage(4000, 3000);
    assert_eq!(ctx.tokens_used, 18_000);
    assert_eq!(ctx.remaining(), 82_000);
}

#[test]
fn test_context_manager_remaining_saturates_at_zero() {
    let mut ctx = ContextManager::new(100);
    ctx.record_usage(200, 100);
    assert_eq!(ctx.remaining(), 0);
}

#[test]
fn test_prune_does_not_remove_all_messages() {
    let ctx = ContextManager::new(10); // Extremely small budget
    let mut msgs = vec![
        Message::system("System"),
        Message::user("User 1"),
        Message::assistant("Reply 1"),
        Message::user("User 2"),
        Message::assistant("Reply 2"),
        Message::user("User 3"),
        Message::assistant("Reply 3"),
        Message::user("User 4"),
        Message::assistant("Reply 4"),
        Message::user("User 5"),
        Message::assistant("Reply 5"),
    ];
    ctx.prune_to_fit(&mut msgs);
    // Should still have at least a few messages
    assert!(msgs.len() >= 2);
    // System message should survive
    assert_eq!(msgs[0].role, ferroclaw::types::Role::System);
}

#[test]
fn test_prune_inserts_marker() {
    let ctx = ContextManager::new(50);
    let mut msgs = vec![Message::system("System prompt")];
    for i in 0..30 {
        msgs.push(Message::user(format!(
            "Long user message number {i} with lots of padding"
        )));
        msgs.push(Message::assistant(format!(
            "Long assistant response {i} with more padding"
        )));
    }
    let original_len = msgs.len();
    ctx.prune_to_fit(&mut msgs);
    if msgs.len() < original_len {
        // Should have a compaction marker
        let has_marker = msgs
            .iter()
            .any(|m| m.text().contains("[Context compaction summary]"));
        assert!(has_marker, "Expected context compaction marker in messages");
    }
}

#[test]
fn test_would_exceed_with_large_message() {
    let ctx = ContextManager::new(100);
    let msgs = vec![Message::user("short")];
    let big = Message::user("x".repeat(1000));
    assert!(ctx.would_exceed(&msgs, &big));
}

#[test]
fn test_would_exceed_allows_within_budget() {
    let ctx = ContextManager::new(1_000_000);
    let msgs = vec![Message::user("short")];
    let small = Message::user("also short");
    assert!(!ctx.would_exceed(&msgs, &small));
}

#[test]
fn test_estimate_total_empty() {
    let total = ContextManager::estimate_total(&[]);
    assert_eq!(total, 0);
}

#[test]
fn test_estimate_total_single_message() {
    let msgs = vec![Message::user("Hello world")]; // 11 chars -> ~3 tokens + 1
    let total = ContextManager::estimate_total(&msgs);
    assert!(total >= 1);
}

// ── Agent Event Variants ────────────────────────────────────────────

#[test]
fn test_agent_event_tool_call_start() {
    use ferroclaw::agent::r#loop::AgentEvent;
    let event = AgentEvent::ToolCallStart {
        id: "tc_1".into(),
        name: "read_file".into(),
        arguments: r#"{"path":"x"}"#.into(),
    };
    match event {
        AgentEvent::ToolCallStart {
            id,
            name,
            arguments,
        } => {
            assert_eq!(id, "tc_1");
            assert_eq!(name, "read_file");
            assert!(arguments.contains("path"));
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_agent_event_tool_result() {
    use ferroclaw::agent::r#loop::AgentEvent;
    let event = AgentEvent::ToolResult {
        id: "tc_1".into(),
        name: "read_file".into(),
        content: "file contents".into(),
        is_error: false,
    };
    match event {
        AgentEvent::ToolResult {
            id,
            name,
            content,
            is_error,
        } => {
            assert_eq!(id, "tc_1");
            assert_eq!(name, "read_file");
            assert_eq!(content, "file contents");
            assert!(!is_error);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_agent_event_token_usage() {
    use ferroclaw::agent::r#loop::AgentEvent;
    let event = AgentEvent::TokenUsage {
        input: 100,
        output: 50,
        total_used: 150,
    };
    match event {
        AgentEvent::TokenUsage {
            input,
            output,
            total_used,
        } => {
            assert_eq!(input, 100);
            assert_eq!(output, 50);
            assert_eq!(total_used, 150);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_agent_event_error() {
    use ferroclaw::agent::r#loop::AgentEvent;
    let event = AgentEvent::Error("something broke".into());
    match event {
        AgentEvent::Error(msg) => assert_eq!(msg, "something broke"),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_agent_event_done() {
    use ferroclaw::agent::r#loop::AgentEvent;
    let event = AgentEvent::Done {
        text: "Final answer".into(),
    };
    match event {
        AgentEvent::Done { text } => assert_eq!(text, "Final answer"),
        _ => panic!("Wrong variant"),
    }
}
