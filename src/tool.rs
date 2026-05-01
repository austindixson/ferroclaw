use crate::error::{FerroError, Result};
use crate::hooks::{HookContext, HookManager};
use crate::types::{
    Capability, CapabilitySet, ToolCall, ToolDefinition, ToolMeta, ToolResult, ToolSource,
};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = Result<ToolResult>> + Send + 'a>>;

/// A callable tool handler.
pub trait ToolHandler: Send + Sync {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a serde_json::Value) -> ToolFuture<'a>;
}

/// Central registry for all available tools (built-in + MCP + skills).
pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
    hooks: HookManager,
}

struct RegisteredTool {
    meta: ToolMeta,
    handler: Box<dyn ToolHandler>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            hooks: HookManager::new(),
        }
    }

    /// Get a reference to the hook manager.
    pub fn hooks(&self) -> &HookManager {
        &self.hooks
    }

    /// Register a tool with its metadata and handler.
    pub fn register(&mut self, meta: ToolMeta, handler: Box<dyn ToolHandler>) {
        let name = meta.definition.name.clone();
        self.tools.insert(name, RegisteredTool { meta, handler });
    }

    /// Register an MCP-discovered tool. These use a generic handler that delegates
    /// back to the MCP client at execution time.
    pub fn register_mcp_tool(&mut self, definition: ToolDefinition, server: String) {
        let meta = ToolMeta {
            required_capabilities: infer_capabilities_from_name(&definition.name),
            source: ToolSource::Mcp {
                server: server.clone(),
            },
            definition,
        };
        // MCP tools get a placeholder handler; actual execution goes through the MCP client
        let handler = Box::new(McpPlaceholderHandler {
            server_name: server,
        });
        let name = meta.definition.name.clone();
        self.tools.insert(name, RegisteredTool { meta, handler });
    }

    /// Get a tool's metadata by name.
    pub fn get_meta(&self, name: &str) -> Option<&ToolMeta> {
        self.tools.get(name).map(|t| &t.meta)
    }

    /// Execute a tool, checking capabilities first.
    pub async fn execute(
        &self,
        name: &str,
        call_id: &str,
        arguments: &serde_json::Value,
        capabilities: &CapabilitySet,
    ) -> Result<ToolResult> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| FerroError::ToolNotFound(name.to_string()))?;

        // Create hook context
        let hook_ctx = HookContext::new(call_id);

        // Execute pre-tool hooks (permission checks, argument modification)
        let modified_args = self.hooks.execute_pre_tool(
            &hook_ctx,
            &ToolCall {
                id: call_id.to_string(),
                name: name.to_string(),
                arguments: arguments.clone(),
            },
        )?;

        // Check capabilities
        if let Err(missing) = capabilities.check(&tool.meta.required_capabilities) {
            // Execute permission check hooks
            match self.hooks.execute_permission_check(
                &hook_ctx,
                name,
                &tool.meta.required_capabilities,
            ) {
                Ok(true) => {
                    // Hook explicitly allowed the operation
                }
                Ok(false) => {
                    // Use default capability check
                    return Err(FerroError::CapabilityDenied {
                        tool: name.to_string(),
                        required: missing.to_string(),
                        available: format!("{:?}", capabilities.capabilities),
                    });
                }
                Err(e) => {
                    // Hook denied the operation
                    return Err(e);
                }
            }
        }

        // Execute the tool
        let result = tool.handler.call(call_id, &modified_args).await?;

        // Execute post-tool hooks (result modification, logging, etc.)
        let final_result = self.hooks.execute_post_tool(
            &hook_ctx,
            &ToolCall {
                id: call_id.to_string(),
                name: name.to_string(),
                arguments: modified_args,
            },
            &result,
        )?;

        Ok(final_result)
    }

    /// Get all tool definitions for sending to the LLM.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|t| t.meta.definition.clone())
            .collect()
    }

    /// Get all tool metadata for diet compression.
    pub fn all_meta(&self) -> Vec<&ToolMeta> {
        self.tools.values().map(|t| &t.meta).collect()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// List tool names grouped by source.
    pub fn list_by_source(&self) -> HashMap<String, Vec<String>> {
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for (name, tool) in &self.tools {
            let source_key = match &tool.meta.source {
                ToolSource::Builtin => "builtin".to_string(),
                ToolSource::Mcp { server } => format!("mcp:{server}"),
                ToolSource::Skill { path } => format!("skill:{path}"),
            };
            groups.entry(source_key).or_default().push(name.clone());
        }
        for names in groups.values_mut() {
            names.sort();
        }
        groups
    }
}

/// Infer capabilities from tool name using keyword heuristics.
fn infer_capabilities_from_name(name: &str) -> Vec<Capability> {
    let lower = name.to_lowercase();
    let mut caps = Vec::new();

    if lower.contains("read") || lower.contains("list") || lower.contains("get") {
        caps.push(Capability::FsRead);
    }
    if lower.contains("write") || lower.contains("create") || lower.contains("delete") {
        caps.push(Capability::FsWrite);
    }
    if lower.contains("exec") || lower.contains("run") || lower.contains("bash") {
        caps.push(Capability::ProcessExec);
    }
    if lower.contains("fetch") || lower.contains("http") || lower.contains("api") {
        caps.push(Capability::NetOutbound);
    }
    if lower.contains("browser") || lower.contains("navigate") || lower.contains("screenshot") {
        caps.push(Capability::BrowserControl);
    }

    if caps.is_empty() {
        // Default: require read capability for unknown tools
        caps.push(Capability::FsRead);
    }

    caps
}

/// Placeholder handler for MCP tools. The actual execution is routed
/// through the MCP client by the agent loop.
struct McpPlaceholderHandler {
    server_name: String,
}

impl ToolHandler for McpPlaceholderHandler {
    fn call<'a>(&'a self, call_id: &'a str, _arguments: &'a serde_json::Value) -> ToolFuture<'a> {
        let server = self.server_name.clone();
        let id = call_id.to_string();
        Box::pin(async move {
            // This should never be called directly — the agent loop intercepts MCP tool calls
            Ok(ToolResult {
                call_id: id,
                content: format!("[MCP tool on server '{server}' — route through MCP client]"),
                is_error: true,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_capabilities() {
        let caps = infer_capabilities_from_name("read_file");
        assert!(caps.contains(&Capability::FsRead));

        let caps = infer_capabilities_from_name("write_file");
        assert!(caps.contains(&Capability::FsWrite));

        let caps = infer_capabilities_from_name("bash_exec");
        assert!(caps.contains(&Capability::ProcessExec));
    }

    #[test]
    fn test_capability_set_check() {
        let set = CapabilitySet::new([Capability::FsRead, Capability::NetOutbound]);
        assert!(set.check(&[Capability::FsRead]).is_ok());
        assert!(set.check(&[Capability::FsWrite]).is_err());
    }

    #[test]
    fn test_registry_basic() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get_meta("nonexistent").is_none());
    }
}
