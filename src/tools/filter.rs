//! Filtered tool registry for subagent tool access control

use crate::error::{FerroError, Result};
use crate::tool::ToolRegistry;
use crate::types::ToolDefinition;
use std::collections::HashSet;
use std::sync::Arc;

/// A wrapper around ToolRegistry that filters available tools
///
/// This is used to implement the `allowed_tools` parameter in AgentTool,
/// ensuring that subagents only have access to the tools they're supposed to.
pub struct FilteredToolRegistry {
    /// Inner tool registry (shared reference)
    inner: Arc<ToolRegistry>,
    /// Set of allowed tool names (None = all tools allowed)
    allowed: Option<HashSet<String>>,
}

impl FilteredToolRegistry {
    /// Create a new filtered tool registry
    ///
    /// If `allowed_tools` is None or empty, all tools are allowed.
    /// Otherwise, only tools with names in `allowed_tools` are accessible.
    pub fn new(registry: Arc<ToolRegistry>, allowed_tools: Option<Vec<String>>) -> Self {
        let allowed = if allowed_tools.as_ref().is_some_and(|t| !t.is_empty()) {
            Some(allowed_tools.unwrap().into_iter().collect())
        } else {
            None
        };

        Self {
            inner: registry,
            allowed,
        }
    }

    /// Check if a tool is accessible
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        match &self.allowed {
            Some(allowed) => allowed.contains(tool_name),
            None => true, // No restriction
        }
    }

    /// Get all tool definitions (filtered)
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.inner
            .definitions()
            .into_iter()
            .filter(|t| self.is_tool_allowed(&t.name))
            .collect()
    }

    /// Get tool metadata for a specific tool
    pub fn get_meta(&self, tool_name: &str) -> Option<&crate::types::ToolMeta> {
        if self.is_tool_allowed(tool_name) {
            self.inner.get_meta(tool_name)
        } else {
            None
        }
    }

    /// Get all tool metadata (filtered)
    pub fn all_meta(&self) -> Vec<&crate::types::ToolMeta> {
        self.inner
            .all_meta()
            .into_iter()
            .filter(|m| self.is_tool_allowed(&m.definition.name))
            .collect()
    }

    /// Execute a tool (with access check)
    pub async fn execute(
        &self,
        tool_name: &str,
        call_id: &str,
        arguments: &serde_json::Value,
        capabilities: &crate::types::CapabilitySet,
    ) -> Result<crate::types::ToolResult> {
        if !self.is_tool_allowed(tool_name) {
            return Err(FerroError::Tool(format!(
                "Tool '{}' is not in the allowed tool list",
                tool_name
            )));
        }

        self.inner
            .execute(tool_name, call_id, arguments, capabilities)
            .await
    }

    /// Get the number of available tools
    pub fn tool_count(&self) -> usize {
        match &self.allowed {
            Some(allowed) => allowed.len(),
            None => self.inner.definitions().len(),
        }
    }

    /// Get the list of allowed tool names
    pub fn allowed_tools(&self) -> Option<&HashSet<String>> {
        self.allowed.as_ref()
    }

    /// Check if filtering is enabled
    pub fn is_filtered(&self) -> bool {
        self.allowed.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolHandler;
    use crate::types::{Capability, ToolMeta, ToolSource};

    // Mock tool handler for testing
    struct MockToolHandler;
    impl ToolHandler for MockToolHandler {
        fn call<'a>(
            &'a self,
            call_id: &'a str,
            _arguments: &'a serde_json::Value,
        ) -> crate::tool::ToolFuture<'a> {
            Box::pin(async move {
                Ok(crate::types::ToolResult {
                    call_id: call_id.to_string(),
                    content: "Mock result".to_string(),
                    is_error: false,
                })
            })
        }
    }

    fn create_mock_registry() -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();

        // Register some mock tools
        registry.register(
            ToolMeta {
                definition: ToolDefinition {
                    name: "read_file".into(),
                    description: "Read a file".into(),
                    input_schema: serde_json::json!({}),
                    server_name: None,
                },
                required_capabilities: vec![Capability::FsRead],
                source: ToolSource::Builtin,
            },
            Box::new(MockToolHandler),
        );

        registry.register(
            ToolMeta {
                definition: ToolDefinition {
                    name: "write_file".into(),
                    description: "Write a file".into(),
                    input_schema: serde_json::json!({}),
                    server_name: None,
                },
                required_capabilities: vec![Capability::FsWrite],
                source: ToolSource::Builtin,
            },
            Box::new(MockToolHandler),
        );

        registry.register(
            ToolMeta {
                definition: ToolDefinition {
                    name: "bash".into(),
                    description: "Execute bash command".into(),
                    input_schema: serde_json::json!({}),
                    server_name: None,
                },
                required_capabilities: vec![Capability::ProcessExec],
                source: ToolSource::Builtin,
            },
            Box::new(MockToolHandler),
        );

        Arc::new(registry)
    }

    #[test]
    fn test_filtered_registry_all_tools_allowed() {
        let registry = create_mock_registry();
        let filtered = FilteredToolRegistry::new(registry, None);

        assert!(!filtered.is_filtered());
        assert!(filtered.is_tool_allowed("read_file"));
        assert!(filtered.is_tool_allowed("write_file"));
        assert!(filtered.is_tool_allowed("bash"));
        assert_eq!(filtered.definitions().len(), 3);
    }

    #[test]
    fn test_filtered_registry_empty_allowed_list() {
        let registry = create_mock_registry();
        let filtered = FilteredToolRegistry::new(registry, Some(vec![]));

        // Empty list should allow all tools
        assert!(!filtered.is_filtered());
        assert!(filtered.is_tool_allowed("read_file"));
    }

    #[test]
    fn test_filtered_registry_restricted_access() {
        let registry = create_mock_registry();
        let allowed = vec!["read_file".to_string(), "write_file".to_string()];
        let filtered = FilteredToolRegistry::new(registry, Some(allowed));

        assert!(filtered.is_filtered());
        assert!(filtered.is_tool_allowed("read_file"));
        assert!(filtered.is_tool_allowed("write_file"));
        assert!(!filtered.is_tool_allowed("bash"));
        assert_eq!(filtered.definitions().len(), 2);
    }

    #[test]
    fn test_filtered_registry_get_meta() {
        let registry = create_mock_registry();
        let allowed = vec!["read_file".to_string()];
        let filtered = FilteredToolRegistry::new(registry, Some(allowed));

        assert!(filtered.get_meta("read_file").is_some());
        assert!(filtered.get_meta("write_file").is_none());
        assert!(filtered.get_meta("bash").is_none());
    }

    #[test]
    fn test_filtered_registry_all_meta() {
        let registry = create_mock_registry();
        let allowed = vec!["read_file".to_string(), "bash".to_string()];
        let filtered = FilteredToolRegistry::new(registry, Some(allowed));

        let meta_list = filtered.all_meta();
        assert_eq!(meta_list.len(), 2);
        assert!(meta_list.iter().any(|m| m.definition.name == "read_file"));
        assert!(meta_list.iter().any(|m| m.definition.name == "bash"));
        assert!(!meta_list.iter().any(|m| m.definition.name == "write_file"));
    }

    #[test]
    fn test_filtered_registry_tool_count() {
        let registry = create_mock_registry();

        let unrestricted = FilteredToolRegistry::new(registry.clone(), None);
        assert_eq!(unrestricted.tool_count(), 3);

        let restricted = FilteredToolRegistry::new(registry, Some(vec!["read_file".to_string()]));
        assert_eq!(restricted.tool_count(), 1);
    }

    #[test]
    fn test_filtered_registry_allowed_tools() {
        let registry = create_mock_registry();
        let allowed = vec!["read_file".to_string(), "write_file".to_string()];
        let filtered = FilteredToolRegistry::new(registry, Some(allowed.clone()));

        let allowed_set = filtered.allowed_tools().unwrap();
        assert_eq!(allowed_set.len(), 2);
        assert!(allowed_set.contains("read_file"));
        assert!(allowed_set.contains("write_file"));
        assert!(!allowed_set.contains("bash"));
    }

    #[tokio::test]
    async fn test_filtered_registry_execute_allowed() {
        let registry = create_mock_registry();
        let filtered = FilteredToolRegistry::new(registry, Some(vec!["read_file".to_string()]));

        let capabilities = crate::types::CapabilitySet::new([Capability::FsRead]);
        let result = filtered
            .execute("read_file", "call-1", &serde_json::json!({}), &capabilities)
            .await;

        assert!(result.is_ok());
        assert!(!result.unwrap().is_error);
    }

    #[tokio::test]
    async fn test_filtered_registry_execute_denied() {
        let registry = create_mock_registry();
        let filtered = FilteredToolRegistry::new(registry, Some(vec!["read_file".to_string()]));

        let capabilities = crate::types::CapabilitySet::new([Capability::FsWrite]);
        let result = filtered
            .execute(
                "write_file",
                "call-1",
                &serde_json::json!({}),
                &capabilities,
            )
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not in the allowed tool list")
        );
    }

    #[test]
    fn test_filtered_registry_case_sensitivity() {
        let registry = create_mock_registry();
        let allowed = vec!["READ_FILE".to_string()]; // Different case
        let filtered = FilteredToolRegistry::new(registry, Some(allowed));

        // Tool names are case-sensitive
        assert!(!filtered.is_tool_allowed("read_file"));
        assert!(filtered.is_tool_allowed("READ_FILE"));
    }
}
