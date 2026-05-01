//! Event-driven extensibility system for Ferroclaw.
//!
//! Hooks allow intercepting and modifying tool execution, permission checks,
//! and session lifecycle events. They provide a composable way to add
//! cross-cutting concerns like logging, auditing, rate limiting, and caching.
//!
//! # Example
//!
//! ```rust
//! use ferroclaw::hooks::{HookManager, Hook, HookContext, HookResult};
//! use ferroclaw::types::{ToolCall, ToolResult};
//!
//! struct MyHook;
//!
//! impl Hook for MyHook {
//!     fn pre_tool(&self, ctx: &HookContext, call: &ToolCall) -> HookResult {
//!         println!("Tool {} called with args: {}", call.name, call.arguments);
//!         HookResult::Continue
//!     }
//! }
//!
//! let mut manager = HookManager::new();
//! manager.register(Box::new(MyHook));
//! ```

pub mod builtin;

use crate::error::{FerroError, Result};
use crate::types::{Capability, ToolCall, ToolResult};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Result of a hook execution, controlling the flow of the operation.
#[derive(Debug, Clone)]
pub enum HookResult {
    /// Continue with the normal operation.
    Continue,

    /// Halt the operation with an error.
    Halt(String),

    /// Modify the tool call arguments before execution (only valid for pre_tool).
    ModifyArguments(serde_json::Value),

    /// Modify the tool result before returning it (only valid for post_tool).
    ModifyResult(ToolResult),
}

impl HookResult {
    /// Check if this result allows the operation to continue.
    pub fn should_continue(&self) -> bool {
        matches!(
            self,
            Self::Continue | Self::ModifyArguments(_) | Self::ModifyResult(_)
        )
    }

    /// Get the error message if this result halts the operation.
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Halt(msg) => Some(msg),
            _ => None,
        }
    }
}

/// Context passed to all hooks, providing runtime information.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Unique session identifier.
    pub session_id: String,

    /// Optional user identifier.
    pub user_id: Option<String>,

    /// Optional channel identifier (e.g., "slack", "discord").
    pub channel_id: Option<String>,

    /// Timestamp when the hook was invoked.
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Additional metadata hooks can use for custom logic.
    pub metadata: HashMap<String, String>,
}

impl HookContext {
    /// Create a new hook context.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            user_id: None,
            channel_id: None,
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the context.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// A trait for hooks that can intercept and modify Ferroclaw operations.
///
/// Hooks are called in registration order. If any hook returns `HookResult::Halt`,
/// the operation is aborted and subsequent hooks are not called.
pub trait Hook: Send + Sync {
    /// Called before a tool is executed.
    ///
    /// Return `HookResult::ModifyArguments` to change the arguments passed to the tool.
    /// Return `HookResult::Halt` to prevent the tool from executing.
    fn pre_tool(&self, _ctx: &HookContext, _call: &ToolCall) -> HookResult {
        HookResult::Continue
    }

    /// Called after a tool has executed, regardless of success or failure.
    ///
    /// Return `HookResult::ModifyResult` to change the result returned to the agent.
    /// Return `HookResult::Halt` to suppress the result (use with caution).
    fn post_tool(&self, _ctx: &HookContext, _call: &ToolCall, _result: &ToolResult) -> HookResult {
        HookResult::Continue
    }

    /// Called during permission checks to override the capability system.
    ///
    /// Return `HookResult::Halt` with "allow" to grant permission regardless of capabilities.
    /// Return `HookResult::Halt` with any other message to deny permission.
    /// Return `HookResult::Continue` to use the default capability check.
    fn permission_check(
        &self,
        _ctx: &HookContext,
        _tool_name: &str,
        _required_caps: &[Capability],
    ) -> HookResult {
        HookResult::Continue
    }

    /// Called when configuration is reloaded or modified.
    fn config_change(&self, _ctx: &HookContext, _config_key: &str) {}

    /// Called when a new session starts.
    fn session_start(&self, _ctx: &HookContext) {}

    /// Called when a session ends.
    fn session_end(&self, _ctx: &HookContext) {}
}

/// Manager for registering and executing hooks.
///
/// Hooks are stored in an Arc<RwLock<>> to allow shared access across threads.
/// They are executed in registration order, and errors in one hook do not affect others.
#[derive(Clone)]
pub struct HookManager {
    hooks: Arc<RwLock<Vec<Box<dyn Hook>>>>,
}

impl HookManager {
    /// Create a new hook manager with no registered hooks.
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a hook. Hooks are called in registration order.
    pub fn register(&self, hook: Box<dyn Hook>) {
        let mut hooks = self.hooks.write().unwrap();
        hooks.push(hook);
    }

    /// Remove all hooks. Useful for testing or reinitialization.
    pub fn clear(&self) {
        let mut hooks = self.hooks.write().unwrap();
        hooks.clear();
    }

    /// Get the number of registered hooks.
    pub fn len(&self) -> usize {
        let hooks = self.hooks.read().unwrap();
        hooks.len()
    }

    /// Check if any hooks are registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Execute all pre_tool hooks.
    ///
    /// Returns the modified arguments if any hook requested changes,
    /// or the original arguments if all hooks returned Continue.
    ///
    /// Returns an error if any hook halted execution.
    pub fn execute_pre_tool(
        &self,
        ctx: &HookContext,
        call: &ToolCall,
    ) -> Result<serde_json::Value> {
        let hooks = self.hooks.read().unwrap();
        let mut current_args = call.arguments.clone();

        for hook in hooks.iter() {
            match hook.pre_tool(ctx, call) {
                HookResult::Continue => {}
                HookResult::Halt(msg) => {
                    return Err(FerroError::HookFailed {
                        hook: std::any::type_name_of_val(hook),
                        reason: msg,
                    });
                }
                HookResult::ModifyArguments(new_args) => {
                    current_args = new_args;
                }
                HookResult::ModifyResult(_) => {
                    return Err(FerroError::HookFailed {
                        hook: std::any::type_name_of_val(hook),
                        reason: "ModifyResult is not valid in pre_tool hook".to_string(),
                    });
                }
            }
        }

        Ok(current_args)
    }

    /// Execute all post_tool hooks.
    ///
    /// Returns the modified result if any hook requested changes,
    /// or the original result if all hooks returned Continue.
    ///
    /// Returns an error if any hook halted execution.
    pub fn execute_post_tool(
        &self,
        ctx: &HookContext,
        call: &ToolCall,
        result: &ToolResult,
    ) -> Result<ToolResult> {
        let hooks = self.hooks.read().unwrap();
        let mut current_result = result.clone();

        for hook in hooks.iter() {
            match hook.post_tool(ctx, call, result) {
                HookResult::Continue => {}
                HookResult::Halt(msg) => {
                    return Err(FerroError::HookFailed {
                        hook: std::any::type_name_of_val(hook),
                        reason: msg,
                    });
                }
                HookResult::ModifyResult(new_result) => {
                    current_result = new_result;
                }
                HookResult::ModifyArguments(_) => {
                    return Err(FerroError::HookFailed {
                        hook: std::any::type_name_of_val(hook),
                        reason: "ModifyArguments is not valid in post_tool hook".to_string(),
                    });
                }
            }
        }

        Ok(current_result)
    }

    /// Execute all permission_check hooks.
    ///
    /// Returns Ok(true) if any hook explicitly allowed the operation,
    /// Ok(false) if all hooks returned Continue (use default check),
    /// or Err if any hook denied the operation.
    pub fn execute_permission_check(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        required_caps: &[Capability],
    ) -> Result<bool> {
        let hooks = self.hooks.read().unwrap();

        for hook in hooks.iter() {
            match hook.permission_check(ctx, tool_name, required_caps) {
                HookResult::Continue => {}
                HookResult::Halt(msg) => {
                    if msg.to_lowercase() == "allow" {
                        return Ok(true);
                    } else {
                        return Err(FerroError::HookFailed {
                            hook: std::any::type_name_of_val(hook),
                            reason: msg,
                        });
                    }
                }
                HookResult::ModifyArguments(_) | HookResult::ModifyResult(_) => {
                    return Err(FerroError::HookFailed {
                        hook: std::any::type_name_of_val(hook),
                        reason: "Modify operations are not valid in permission_check hook"
                            .to_string(),
                    });
                }
            }
        }

        Ok(false)
    }

    /// Execute all config_change hooks.
    pub fn execute_config_change(&self, ctx: &HookContext, config_key: &str) {
        let hooks = self.hooks.read().unwrap();
        for hook in hooks.iter() {
            hook.config_change(ctx, config_key);
        }
    }

    /// Execute all session_start hooks.
    pub fn execute_session_start(&self, ctx: &HookContext) {
        let hooks = self.hooks.read().unwrap();
        for hook in hooks.iter() {
            hook.session_start(ctx);
        }
    }

    /// Execute all session_end hooks.
    pub fn execute_session_end(&self, ctx: &HookContext) {
        let hooks = self.hooks.read().unwrap();
        for hook in hooks.iter() {
            hook.session_end(ctx);
        }
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_result_continue() {
        let result = HookResult::Continue;
        assert!(result.should_continue());
        assert!(result.error_message().is_none());
    }

    #[test]
    fn test_hook_result_halt() {
        let result = HookResult::Halt("Access denied".to_string());
        assert!(!result.should_continue());
        assert_eq!(result.error_message(), Some("Access denied"));
    }

    #[test]
    fn test_hook_context_new() {
        let ctx = HookContext::new("session-123");
        assert_eq!(ctx.session_id, "session-123");
        assert!(ctx.user_id.is_none());
        assert!(ctx.channel_id.is_none());
    }

    #[test]
    fn test_hook_context_with_metadata() {
        let ctx = HookContext::new("session-123")
            .with_metadata("ip", "127.0.0.1")
            .with_metadata("user_agent", "test");

        assert_eq!(ctx.metadata.len(), 2);
        assert_eq!(ctx.metadata.get("ip"), Some(&"127.0.0.1".to_string()));
    }

    #[test]
    fn test_hook_manager_new() {
        let manager = HookManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_hook_manager_register() {
        struct TestHook;
        impl Hook for TestHook {}

        let manager = HookManager::new();
        manager.register(Box::new(TestHook));
        assert_eq!(manager.len(), 1);
        assert!(!manager.is_empty());
    }

    #[test]
    fn test_hook_manager_clear() {
        struct TestHook;
        impl Hook for TestHook {}

        let manager = HookManager::new();
        manager.register(Box::new(TestHook));
        manager.clear();
        assert!(manager.is_empty());
    }
}

#[cfg(test)]
mod hooks_test;
