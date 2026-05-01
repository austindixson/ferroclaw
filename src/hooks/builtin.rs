//! Built-in hooks for common use cases.
//!
//! This module provides ready-to-use hooks for logging, auditing,
//! rate limiting, and other cross-cutting concerns.

use crate::hooks::{Hook, HookContext, HookResult};
use crate::types::{Capability, ToolCall, ToolResult};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Logs all tool calls and results to stdout.
///
/// This is useful for debugging and monitoring tool usage.
pub struct LoggingHook {
    include_arguments: bool,
    include_results: bool,
}

impl LoggingHook {
    /// Create a new logging hook.
    ///
    /// # Arguments
    ///
    /// * `include_arguments` - If true, log tool call arguments
    /// * `include_results` - If true, log tool results
    pub fn new(include_arguments: bool, include_results: bool) -> Self {
        Self {
            include_arguments,
            include_results,
        }
    }
}

impl Hook for LoggingHook {
    fn pre_tool(&self, ctx: &HookContext, call: &ToolCall) -> HookResult {
        let args_str = if self.include_arguments {
            format!(" args={}", call.arguments)
        } else {
            String::new()
        };
        println!(
            "[{}] Tool called: {}{}",
            ctx.session_id, call.name, args_str
        );
        HookResult::Continue
    }

    fn post_tool(&self, ctx: &HookContext, call: &ToolCall, result: &ToolResult) -> HookResult {
        let result_str = if self.include_results {
            format!(" result={}", result.content)
        } else {
            String::new()
        };
        let status = if result.is_error { "ERROR" } else { "OK" };
        println!(
            "[{}] Tool {}: {} ({}){}",
            ctx.session_id, call.name, status, result.call_id, result_str
        );
        HookResult::Continue
    }

    fn session_start(&self, ctx: &HookContext) {
        println!("[{}] Session started", ctx.session_id);
    }

    fn session_end(&self, ctx: &HookContext) {
        println!("[{}] Session ended", ctx.session_id);
    }
}

/// Records tool executions to an audit log.
///
/// This hook maintains an in-memory log of all tool executions,
/// which can be used for security auditing and compliance.
pub struct AuditHook {
    log: Arc<std::sync::Mutex<AuditLog>>,
}

struct AuditLog {
    entries: Vec<AuditEntry>,
}

struct AuditEntry {
    timestamp: chrono::DateTime<chrono::Utc>,
    session_id: String,
    tool_name: String,
    tool_call_id: String,
    user_id: Option<String>,
    channel_id: Option<String>,
    success: bool,
    error_message: Option<String>,
}

impl AuditHook {
    /// Create a new audit hook.
    pub fn new() -> Self {
        Self {
            log: Arc::new(std::sync::Mutex::new(AuditLog {
                entries: Vec::new(),
            })),
        }
    }

    /// Get a snapshot of the audit log.
    pub fn get_log(&self) -> Vec<AuditEntrySnapshot> {
        let log = self.log.lock().unwrap();
        log.entries
            .iter()
            .map(|entry| AuditEntrySnapshot {
                timestamp: entry.timestamp,
                session_id: entry.session_id.clone(),
                tool_name: entry.tool_name.clone(),
                tool_call_id: entry.tool_call_id.clone(),
                user_id: entry.user_id.clone(),
                channel_id: entry.channel_id.clone(),
                success: entry.success,
                error_message: entry.error_message.clone(),
            })
            .collect()
    }

    /// Clear the audit log.
    pub fn clear_log(&self) {
        let mut log = self.log.lock().unwrap();
        log.entries.clear();
    }

    /// Get the number of audit log entries.
    pub fn len(&self) -> usize {
        let log = self.log.lock().unwrap();
        log.entries.len()
    }

    /// Returns true if the audit log has no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A snapshot of an audit log entry.
#[derive(Debug, Clone)]
pub struct AuditEntrySnapshot {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub session_id: String,
    pub tool_name: String,
    pub tool_call_id: String,
    pub user_id: Option<String>,
    pub channel_id: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
}

impl Default for AuditHook {
    fn default() -> Self {
        Self::new()
    }
}

impl Hook for AuditHook {
    fn post_tool(&self, ctx: &HookContext, call: &ToolCall, result: &ToolResult) -> HookResult {
        let mut log = self.log.lock().unwrap();
        log.entries.push(AuditEntry {
            timestamp: chrono::Utc::now(),
            session_id: ctx.session_id.clone(),
            tool_name: call.name.clone(),
            tool_call_id: result.call_id.clone(),
            user_id: ctx.user_id.clone(),
            channel_id: ctx.channel_id.clone(),
            success: !result.is_error,
            error_message: if result.is_error {
                Some(result.content.clone())
            } else {
                None
            },
        });
        HookResult::Continue
    }
}

/// Rate limits tool execution to prevent abuse.
///
/// This hook tracks the number of tool executions per session
/// and halts execution if a limit is exceeded within a time window.
pub struct RateLimitHook {
    state: Arc<std::sync::Mutex<HashMap<String, RateLimitState>>>,
    max_calls: u64,
    window: Duration,
}

struct RateLimitState {
    calls: Vec<Instant>,
}

impl RateLimitHook {
    /// Create a new rate limit hook.
    ///
    /// # Arguments
    ///
    /// * `max_calls` - Maximum number of tool calls allowed per window
    /// * `window_secs` - Time window in seconds
    pub fn new(max_calls: u64, window_secs: u64) -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(HashMap::new())),
            max_calls,
            window: Duration::from_secs(window_secs),
        }
    }

    /// Reset rate limit state for a session.
    pub fn reset_session(&self, session_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.remove(session_id);
    }

    /// Get the current call count for a session.
    pub fn call_count(&self, session_id: &str) -> usize {
        let state = self.state.lock().unwrap();
        state.get(session_id).map(|s| s.calls.len()).unwrap_or(0)
    }
}

impl Hook for RateLimitHook {
    fn pre_tool(&self, ctx: &HookContext, _call: &ToolCall) -> HookResult {
        let mut state_map = self.state.lock().unwrap();
        let now = Instant::now();

        // Get or create state for this session
        let state = state_map
            .entry(ctx.session_id.clone())
            .or_insert_with(|| RateLimitState { calls: Vec::new() });

        // Remove calls outside the time window
        state
            .calls
            .retain(|&timestamp| now.duration_since(timestamp) < self.window);

        // Check if limit exceeded
        if state.calls.len() as u64 >= self.max_calls {
            return HookResult::Halt(format!(
                "Rate limit exceeded: {} calls per {:?} allowed, {} calls made",
                self.max_calls,
                self.window,
                state.calls.len()
            ));
        }

        // Record this call
        state.calls.push(now);

        HookResult::Continue
    }

    fn session_end(&self, ctx: &HookContext) {
        // Clean up state when session ends
        let mut state_map = self.state.lock().unwrap();
        state_map.remove(&ctx.session_id);
    }
}

/// Enforces capability checks with optional whitelist.
///
/// This hook can be used to implement additional security rules
/// on top of the capability system.
pub struct SecurityHook {
    /// Tools that are always denied, regardless of capabilities.
    denied_tools: Vec<String>,

    /// Tools that are always allowed, regardless of capabilities.
    allowed_tools: Vec<String>,

    /// Optional user-specific capability overrides.
    user_capabilities: Arc<std::sync::Mutex<HashMap<String, Vec<Capability>>>>,
}

impl SecurityHook {
    /// Create a new security hook.
    ///
    /// # Arguments
    ///
    /// * `denied_tools` - Tools that are always denied
    /// * `allowed_tools` - Tools that are always allowed
    pub fn new(denied_tools: Vec<String>, allowed_tools: Vec<String>) -> Self {
        Self {
            denied_tools,
            allowed_tools,
            user_capabilities: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Grant additional capabilities to a specific user.
    pub fn grant_user_capabilities(&self, user_id: &str, caps: Vec<Capability>) {
        let mut user_caps = self.user_capabilities.lock().unwrap();
        user_caps.insert(user_id.to_string(), caps);
    }

    /// Revoke capabilities from a specific user.
    pub fn revoke_user_capabilities(&self, user_id: &str) {
        let mut user_caps = self.user_capabilities.lock().unwrap();
        user_caps.remove(user_id);
    }
}

impl Hook for SecurityHook {
    fn permission_check(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        _required_caps: &[Capability],
    ) -> HookResult {
        // Check denylist
        if self.denied_tools.contains(&tool_name.to_string()) {
            return HookResult::Halt(format!(
                "Tool '{}' is explicitly denied by security policy",
                tool_name
            ));
        }

        // Check allowlist
        if self.allowed_tools.contains(&tool_name.to_string()) {
            return HookResult::Halt("allow".to_string());
        }

        // Check user-specific capabilities
        if let Some(user_id) = &ctx.user_id {
            let user_caps = self.user_capabilities.lock().unwrap();
            if let Some(caps) = user_caps.get(user_id) {
                for cap in _required_caps {
                    if !caps.contains(cap) {
                        return HookResult::Halt(format!(
                            "User '{}' lacks capability '{:?}' for tool '{}'",
                            user_id, cap, tool_name
                        ));
                    }
                }
                return HookResult::Halt("allow".to_string());
            }
        }

        HookResult::Continue
    }
}

/// Tracks tool usage statistics.
///
/// This hook maintains counters for tool executions, which can be
/// used for analytics and monitoring.
pub struct MetricsHook {
    tool_calls: Arc<AtomicU64>,
    tool_errors: Arc<AtomicU64>,
}

impl MetricsHook {
    /// Create a new metrics hook.
    pub fn new() -> Self {
        Self {
            tool_calls: Arc::new(AtomicU64::new(0)),
            tool_errors: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get the total number of tool calls.
    pub fn total_calls(&self) -> u64 {
        self.tool_calls.load(Ordering::Relaxed)
    }

    /// Get the total number of tool errors.
    pub fn total_errors(&self) -> u64 {
        self.tool_errors.load(Ordering::Relaxed)
    }

    /// Reset all metrics.
    pub fn reset(&self) {
        self.tool_calls.store(0, Ordering::Relaxed);
        self.tool_errors.store(0, Ordering::Relaxed);
    }
}

impl Default for MetricsHook {
    fn default() -> Self {
        Self::new()
    }
}

impl Hook for MetricsHook {
    fn pre_tool(&self, _ctx: &HookContext, _call: &ToolCall) -> HookResult {
        self.tool_calls.fetch_add(1, Ordering::Relaxed);
        HookResult::Continue
    }

    fn post_tool(&self, _ctx: &HookContext, _call: &ToolCall, result: &ToolResult) -> HookResult {
        if result.is_error {
            self.tool_errors.fetch_add(1, Ordering::Relaxed);
        }
        HookResult::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_logging_hook() {
        let hook = LoggingHook::new(true, true);
        let ctx = HookContext::new("test-session");
        let call = ToolCall {
            id: "call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: json!({"arg": "value"}),
        };

        assert!(matches!(hook.pre_tool(&ctx, &call), HookResult::Continue));

        let result = ToolResult {
            call_id: "call-1".to_string(),
            content: "success".to_string(),
            is_error: false,
        };
        assert!(matches!(
            hook.post_tool(&ctx, &call, &result),
            HookResult::Continue
        ));
    }

    #[test]
    fn test_audit_hook() {
        let hook = AuditHook::new();
        let ctx = HookContext::new("test-session");
        let call = ToolCall {
            id: "call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: json!({}),
        };
        let result = ToolResult {
            call_id: "call-1".to_string(),
            content: "success".to_string(),
            is_error: false,
        };

        hook.post_tool(&ctx, &call, &result);

        assert_eq!(hook.len(), 1);
        let log = hook.get_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].tool_name, "test_tool");
        assert!(log[0].success);
    }

    #[test]
    fn test_rate_limit_hook() {
        let hook = RateLimitHook::new(3, 60); // 3 calls per 60 seconds
        let ctx = HookContext::new("test-session");
        let call = ToolCall {
            id: "call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: json!({}),
        };

        // First 3 calls should succeed
        for _ in 0..3 {
            assert!(matches!(hook.pre_tool(&ctx, &call), HookResult::Continue));
        }

        // 4th call should be rate limited
        assert!(matches!(hook.pre_tool(&ctx, &call), HookResult::Halt(_)));

        // Reset and try again
        hook.reset_session("test-session");
        assert!(matches!(hook.pre_tool(&ctx, &call), HookResult::Continue));
    }

    #[test]
    fn test_security_hook() {
        let hook = SecurityHook::new(
            vec!["dangerous_tool".to_string()],
            vec!["safe_tool".to_string()],
        );
        let ctx = HookContext::new("test-session");

        // Denied tool
        let result = hook.permission_check(&ctx, "dangerous_tool", &[]);
        assert!(matches!(result, HookResult::Halt(_)));

        // Allowed tool
        let result = hook.permission_check(&ctx, "safe_tool", &[]);
        assert!(matches!(result, HookResult::Halt(msg) if msg == "allow"));

        // Other tool
        let result = hook.permission_check(&ctx, "other_tool", &[]);
        assert!(matches!(result, HookResult::Continue));
    }

    #[test]
    fn test_metrics_hook() {
        let hook = MetricsHook::new();
        let ctx = HookContext::new("test-session");
        let call = ToolCall {
            id: "call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: json!({}),
        };
        let result = ToolResult {
            call_id: "call-1".to_string(),
            content: "success".to_string(),
            is_error: false,
        };

        hook.pre_tool(&ctx, &call);
        hook.post_tool(&ctx, &call, &result);

        assert_eq!(hook.total_calls(), 1);
        assert_eq!(hook.total_errors(), 0);

        let error_result = ToolResult {
            call_id: "call-2".to_string(),
            content: "error".to_string(),
            is_error: true,
        };
        hook.pre_tool(&ctx, &call);
        hook.post_tool(&ctx, &call, &error_result);

        assert_eq!(hook.total_calls(), 2);
        assert_eq!(hook.total_errors(), 1);
    }
}
