# HookSystem Integration Guide

The HookSystem provides event-driven extensibility for Ferroclaw, allowing you to intercept and modify tool execution, permission checks, and session lifecycle events.

## Overview

Hooks are registered with a `HookManager` and executed in registration order. They can:
- Modify tool arguments before execution
- Modify tool results after execution
- Override permission checks
- Log and audit operations
- Enforce rate limits
- Track metrics

## Core Concepts

### Hook Trait

The `Hook` trait defines lifecycle methods:

```rust
pub trait Hook: Send + Sync {
    fn pre_tool(&self, ctx: &HookContext, call: &ToolCall) -> HookResult;
    fn post_tool(&self, ctx: &HookContext, call: &ToolCall, result: &ToolResult) -> HookResult;
    fn permission_check(&self, ctx: &HookContext, tool_name: &str, required_caps: &[Capability]) -> HookResult;
    fn config_change(&self, ctx: &HookContext, config_key: &str);
    fn session_start(&self, ctx: &HookContext);
    fn session_end(&self, ctx: &HookContext);
}
```

### HookResult

Control flow options:
- `Continue` - Proceed with normal operation
- `Halt(String)` - Stop operation with error message
- `ModifyArguments(Value)` - Change tool arguments (pre_tool only)
- `ModifyResult(ToolResult)` - Change tool result (post_tool only)

### HookContext

Runtime information passed to all hooks:
```rust
pub struct HookContext {
    pub session_id: String,
    pub user_id: Option<String>,
    pub channel_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}
```

## Integration with ToolRegistry

The `ToolRegistry` now includes a `HookManager` that's automatically invoked during tool execution:

```rust
// In ToolRegistry::execute()
// 1. Execute pre_tool hooks (permission checks, argument modification)
let modified_args = self.hooks.execute_pre_tool(&hook_ctx, &call)?;

// 2. Check capabilities (can be overridden by hooks)
if let Err(missing) = capabilities.check(&tool.meta.required_capabilities) {
    // Execute permission_check hooks
    match self.hooks.execute_permission_check(&hook_ctx, name, &tool.meta.required_capabilities)? {
        true => { /* Hook explicitly allowed */ }
        false => { /* Use default capability check */ }
    }
}

// 3. Execute the tool
let result = tool.handler.call(call_id, &modified_args).await?;

// 4. Execute post_tool hooks (result modification, logging, etc.)
let final_result = self.hooks.execute_post_tool(&hook_ctx, &call, &result)?;
```

## Built-in Hooks

### LoggingHook

Logs all tool calls and results to stdout:

```rust
use ferroclaw::hooks::builtin::LoggingHook;

let hook = LoggingHook::new(
    true,  // include arguments
    true,  // include results
);
registry.hooks().register(Box::new(hook));
```

### AuditHook

Records tool executions to an in-memory audit log:

```rust
use ferroclaw::hooks::builtin::AuditHook;

let hook = AuditHook::new();
registry.hooks().register(Box::new(hook.clone()));

// Later: retrieve audit log
let log = hook.get_log();
for entry in log {
    println!("{}: {} ({})", entry.timestamp, entry.tool_name, entry.tool_call_id);
}
```

### RateLimitHook

Throttles tool usage per session:

```rust
use ferroclaw::hooks::builtin::RateLimitHook;

let hook = RateLimitHook::new(
    100,  // max 100 calls
    60,   // per 60 seconds
);
registry.hooks().register(Box::new(hook));
```

### SecurityHook

Enforces additional security rules:

```rust
use ferroclaw::hooks::builtin::SecurityHook;

let hook = SecurityHook::new(
    vec!["dangerous_tool".to_string()],  // denylist
    vec!["safe_tool".to_string()],       // allowlist
);
registry.hooks().register(Box::new(hook));

// Grant additional capabilities to specific users
hook.grant_user_capabilities("user-123", vec![
    Capability::FsWrite,
    Capability::ProcessExec,
]);
```

### MetricsHook

Tracks usage statistics:

```rust
use ferroclaw::hooks::builtin::MetricsHook;

let hook = MetricsHook::new();
registry.hooks().register(Box::new(hook.clone()));

// Later: retrieve metrics
println!("Total calls: {}", hook.total_calls());
println!("Total errors: {}", hook.total_errors());
```

## Custom Hooks

Create custom hooks by implementing the `Hook` trait:

```rust
use ferroclaw::hooks::{Hook, HookContext, HookResult};
use ferroclaw::types::{Capability, ToolCall, ToolResult};

struct MyCustomHook;

impl Hook for MyCustomHook {
    fn pre_tool(&self, ctx: &HookContext, call: &ToolCall) -> HookResult {
        // Check if user has special permission
        if let Some(user_id) = &ctx.user_id {
            if user_id == "admin" {
                // Admins can modify arguments
                if call.name == "sensitive_tool" {
                    return HookResult::ModifyArguments(
                        serde_json::json!({"admin_override": true})
                    );
                }
            }
        }

        HookResult::Continue
    }

    fn post_tool(&self, ctx: &HookContext, call: &ToolCall, result: &ToolResult) -> HookResult {
        // Log errors to external service
        if result.is_error {
            eprintln!("[{}] Tool {} failed: {}", ctx.session_id, call.name, result.content);
        }

        HookResult::Continue
    }

    fn permission_check(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        _required_caps: &[Capability],
    ) -> HookResult {
        // Allow all tools for admin users
        if ctx.user_id.as_ref().map(|u| u == "admin").unwrap_or(false) {
            return HookResult::Halt("allow".to_string());
        }

        // Deny specific tools
        if tool_name == "dangerous_tool" {
            return HookResult::Halt("Access denied".to_string());
        }

        HookResult::Continue
    }

    fn session_start(&self, ctx: &HookContext) {
        println!("Session {} started", ctx.session_id);
    }

    fn session_end(&self, ctx: &HookContext) {
        println!("Session {} ended", ctx.session_id);
    }
}

// Register the hook
registry.hooks().register(Box::new(MyCustomHook));
```

## Hook Execution Order

Hooks are executed in registration order. If any hook returns `Halt`, subsequent hooks are not called:

```
Hook 1 → Hook 2 → Hook 3
          ↓
        Halt
         ↓
    (Hook 3 not called)
```

## Performance Considerations

- Hooks are called synchronously during tool execution
- Keep hook logic fast to avoid delaying operations
- Use async operations within hooks if needed (but spawn tasks to avoid blocking)
- Consider the overhead when registering many hooks

## Error Handling

Hooks that return errors will halt the operation:

```rust
// In a hook
fn pre_tool(&self, _ctx: &HookContext, _call: &ToolCall) -> HookResult {
    if some_condition {
        return HookResult::Halt("Operation not allowed".to_string());
    }
    HookResult::Continue
}

// This will cause ToolRegistry::execute() to return an error
```

## Testing Hooks

Test hooks by registering them and invoking hook methods:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ferroclaw::hooks::{HookManager, HookContext, HookResult};
    use ferroclaw::types::ToolCall;
    use serde_json::json;

    #[test]
    fn test_my_hook() {
        let manager = HookManager::new();
        let hook = MyCustomHook;
        manager.register(Box::new(hook));

        let ctx = HookContext::new("test-session");
        let call = ToolCall {
            id: "call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: json!({}),
        };

        let result = manager.execute_pre_tool(&ctx, &call).unwrap();
        // Assert expected behavior
    }
}
```

## Thread Safety

`HookManager` is thread-safe and can be cloned:

```rust
let manager = HookManager::new();
let manager_clone = manager.clone();

// Both clones share the same registered hooks
manager.register(Box::new(MyHook));
manager_clone.register(Box::new(AnotherHook));
```

## Cleanup

Hooks are automatically cleaned up when sessions end:

```rust
// Implement session_end for cleanup
fn session_end(&self, ctx: &HookContext) {
    // Clean up resources
    self.state.remove(&ctx.session_id);
}
```

## Best Practices

1. **Keep hooks focused** - Each hook should do one thing well
2. **Use metadata** - Store custom data in `HookContext.metadata`
3. **Handle errors gracefully** - Return `Halt` with clear error messages
4. **Test thoroughly** - Hook bugs affect all tool operations
5. **Document side effects** - Clearly document what hooks modify
6. **Consider performance** - Hooks add overhead to every tool call
7. **Use built-in hooks** - Prefer built-in hooks over custom implementations
8. **Log important events** - Use hooks for auditing and debugging

## Example: Complete Integration

```rust
use ferroclaw::tool::ToolRegistry;
use ferroclaw::hooks::builtin::{LoggingHook, AuditHook, RateLimitHook};

// Create tool registry
let registry = ToolRegistry::new();

// Register hooks
registry.hooks().register(Box::new(LoggingHook::new(true, false)));
registry.hooks().register(Box::new(AuditHook::new()));
registry.hooks().register(Box::new(RateLimitHook::new(100, 60)));

// Use registry normally
let result = registry.execute(
    "read_file",
    "call-123",
    &json!({"path": "/tmp/file.txt"}),
    &capabilities,
).await?;

// Hooks were automatically invoked:
// 1. LoggingHook logged the call
// 2. AuditHook recorded the call
// 3. RateLimitHook checked the limit
// 4. Tool executed
// 5. AuditHook recorded the result
// 6. LoggingHook logged the result
```

## Troubleshooting

### Hook not being called
- Ensure hook is registered with `registry.hooks().register(Box::new(hook))`
- Check that tools are executed through `ToolRegistry::execute()`

### Hook causes performance issues
- Profile hook execution time
- Move expensive operations to background tasks
- Consider removing unnecessary hooks

### Hook prevents tool execution
- Check if hook returns `HookResult::Halt`
- Verify permission_check hook returns "allow" string for explicit allow
- Review hook execution order

### Hooks interfere with each other
- Use `HookContext.metadata` to share state between hooks
- Be careful with `ModifyArguments` and `ModifyResult` in multiple hooks
- Consider combining related hooks into one
