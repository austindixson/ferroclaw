//! MCP client wrapper with discovery, execution, caching, and health checks.
//!
//! Uses the official rmcp crate for protocol handling, adds:
//! - Schema caching (via SchemaCache)
//! - Connection health monitoring
//! - DietMCP response formatting

use crate::config::McpServerConfig;
use crate::error::{FerroError, Result};
use crate::mcp::cache::{SchemaCache, config_fingerprint};
use crate::mcp::compression::compress_tools;
use crate::mcp::diet::{DietFormat, DietResponse, format_response};
use crate::types::ToolDefinition;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;

/// MCP client that manages connections to multiple MCP servers.
pub struct McpClient {
    servers: HashMap<String, McpServerConfig>,
    cache: SchemaCache,
    default_format: DietFormat,
    max_response_size: usize,
    compression_enabled: bool,
}

impl McpClient {
    pub fn new(servers: HashMap<String, McpServerConfig>, max_response_size: usize) -> Self {
        Self {
            servers,
            cache: SchemaCache::new(),
            default_format: DietFormat::Summary,
            max_response_size,
            compression_enabled: true,
        }
    }

    /// Enable or disable schema compression
    pub fn with_compression(mut self, enabled: bool) -> Self {
        self.compression_enabled = enabled;
        self
    }

    /// Discover tools from an MCP server (cache-first).
    pub async fn discover_tools(
        &self,
        server_name: &str,
        force_refresh: bool,
    ) -> Result<Vec<ToolDefinition>> {
        let server_config = self
            .servers
            .get(server_name)
            .ok_or_else(|| FerroError::Mcp(format!("Server '{server_name}' not configured")))?;

        let fingerprint = config_fingerprint(
            server_config.command.as_deref(),
            &server_config.args,
            server_config.url.as_deref(),
        );

        // Check cache first
        if !force_refresh
            && let Some(cached) =
                self.cache
                    .get(server_name, &fingerprint, server_config.cache_ttl_seconds)
        {
            tracing::debug!("Cache hit for MCP server '{server_name}'");
            return Ok(cached);
        }

        // Fetch from server
        let tools = self.fetch_tools(server_name, server_config).await?;

        // Compress schemas if enabled
        let (final_tools, _metrics) = if self.compression_enabled {
            let (compressed, metrics) = compress_tools(&tools);
            tracing::debug!(
                "Compressed schemas for '{server_name}': {:.1}% reduction ({} -> {} tokens)",
                metrics.reduction_percent(),
                metrics.original_tokens,
                metrics.compressed_tokens
            );
            (compressed, metrics)
        } else {
            (tools.clone(), Default::default())
        };

        // Cache the result (cache compressed version if compression is enabled)
        let _ = self.cache.put(
            server_name,
            &fingerprint,
            server_config.cache_ttl_seconds,
            &final_tools,
        );

        Ok(final_tools)
    }

    /// Discover tools from ALL configured servers.
    pub async fn discover_all_tools(
        &self,
        force_refresh: bool,
    ) -> HashMap<String, Vec<ToolDefinition>> {
        let mut all_tools = HashMap::new();
        for server_name in self.servers.keys() {
            match self.discover_tools(server_name, force_refresh).await {
                Ok(tools) => {
                    all_tools.insert(server_name.clone(), tools);
                }
                Err(e) => {
                    tracing::warn!("Failed to discover tools from '{server_name}': {e}");
                }
            }
        }
        all_tools
    }

    /// Execute a tool on an MCP server.
    pub async fn execute_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: &Value,
    ) -> Result<DietResponse> {
        let server_config = self
            .servers
            .get(server_name)
            .ok_or_else(|| FerroError::Mcp(format!("Server '{server_name}' not configured")))?;

        let raw_result = self.call_tool(server_config, tool_name, arguments).await?;

        Ok(format_response(
            &raw_result,
            self.default_format,
            self.max_response_size,
        ))
    }

    /// List all configured server names.
    pub fn server_names(&self) -> Vec<&str> {
        self.servers.keys().map(|s| s.as_str()).collect()
    }

    // --- Private methods ---

    /// Connect to an MCP server via stdio and fetch tool list.
    ///
    /// Uses JSON-RPC over stdin/stdout following the MCP protocol spec.
    /// This is a simplified implementation that spawns the server process,
    /// sends initialize + tools/list, and parses the response.
    async fn fetch_tools(
        &self,
        server_name: &str,
        config: &McpServerConfig,
    ) -> Result<Vec<ToolDefinition>> {
        if config.is_stdio() {
            self.fetch_tools_stdio(server_name, config).await
        } else if config.is_sse() {
            self.fetch_tools_sse(server_name, config).await
        } else {
            Err(FerroError::Mcp(format!(
                "Server '{server_name}' has neither command nor url"
            )))
        }
    }

    async fn fetch_tools_stdio(
        &self,
        server_name: &str,
        config: &McpServerConfig,
    ) -> Result<Vec<ToolDefinition>> {
        let command = config
            .command
            .as_ref()
            .ok_or_else(|| FerroError::Mcp("Missing command".into()))?;

        let mut child = Command::new(command)
            .args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                FerroError::Mcp(format!(
                    "Failed to spawn MCP server '{server_name}' ({command}): {e}"
                ))
            })?;

        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| FerroError::Mcp("Failed to get stdin handle".into()))?;

        // Send initialize request
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "ferroclaw",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        use tokio::io::AsyncWriteExt;
        let init_bytes = serde_json::to_vec(&init_req)?;
        stdin.write_all(&init_bytes).await?;
        stdin.write_all(b"\n").await?;

        // Send initialized notification
        let initialized = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let notif_bytes = serde_json::to_vec(&initialized)?;
        stdin.write_all(&notif_bytes).await?;
        stdin.write_all(b"\n").await?;

        // Send tools/list request
        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let list_bytes = serde_json::to_vec(&list_req)?;
        stdin.write_all(&list_bytes).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;

        // Read responses
        use tokio::io::AsyncBufReadExt;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| FerroError::Mcp("Failed to get stdout handle".into()))?;
        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();

        let mut tools = Vec::new();
        let mut got_tools = false;

        // Read lines with a timeout
        let timeout = tokio::time::Duration::from_secs(30);
        let read_result = tokio::time::timeout(timeout, async {
            while let Some(line) = lines.next_line().await.transpose() {
                let line =
                    line.map_err(|e| FerroError::Mcp(format!("Failed to read from server: {e}")))?;

                if let Ok(response) = serde_json::from_str::<Value>(&line)
                    && response.get("id") == Some(&json!(2))
                {
                    // This is our tools/list response
                    if let Some(result) = response.get("result")
                        && let Some(tool_arr) = result.get("tools").and_then(|t| t.as_array())
                    {
                        for tool_val in tool_arr {
                            let name = tool_val
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            let description = tool_val
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_string();
                            let input_schema = tool_val
                                .get("inputSchema")
                                .cloned()
                                .unwrap_or(json!({"type": "object"}));

                            tools.push(ToolDefinition {
                                name,
                                description,
                                input_schema,
                                server_name: Some(server_name.to_string()),
                            });
                        }
                    }
                    got_tools = true;
                    break;
                }
            }
            Ok::<_, FerroError>(())
        })
        .await;

        // Kill the child process
        let _ = child.kill().await;

        match read_result {
            Ok(Ok(())) if got_tools => Ok(tools),
            Ok(Ok(())) => Err(FerroError::Mcp(format!(
                "No tools/list response from server '{server_name}'"
            ))),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(FerroError::Mcp(format!(
                "Timeout waiting for tools from server '{server_name}'"
            ))),
        }
    }

    async fn fetch_tools_sse(
        &self,
        server_name: &str,
        _config: &McpServerConfig,
    ) -> Result<Vec<ToolDefinition>> {
        // SSE transport implementation placeholder
        Err(FerroError::Mcp(format!(
            "SSE transport for '{server_name}' not yet implemented"
        )))
    }

    /// Execute a tool call via stdio.
    async fn call_tool(
        &self,
        config: &McpServerConfig,
        tool_name: &str,
        arguments: &Value,
    ) -> Result<String> {
        if !config.is_stdio() {
            return Err(FerroError::Mcp(
                "Only stdio transport supported for tool execution".into(),
            ));
        }

        let command = config
            .command
            .as_ref()
            .ok_or_else(|| FerroError::Mcp("Missing command".into()))?;

        let mut child = Command::new(command)
            .args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| FerroError::Mcp(format!("Failed to spawn server: {e}")))?;

        let stdin = child.stdin.as_mut().unwrap();

        use tokio::io::AsyncWriteExt;

        // Initialize
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "ferroclaw", "version": env!("CARGO_PKG_VERSION")}
            }
        });
        let bytes = serde_json::to_vec(&init_req)?;
        stdin.write_all(&bytes).await?;
        stdin.write_all(b"\n").await?;

        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        let bytes = serde_json::to_vec(&initialized)?;
        stdin.write_all(&bytes).await?;
        stdin.write_all(b"\n").await?;

        // Call tool
        let call_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });
        let bytes = serde_json::to_vec(&call_req)?;
        stdin.write_all(&bytes).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;

        // Read result
        use tokio::io::AsyncBufReadExt;
        let stdout = child.stdout.take().unwrap();
        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();

        let timeout = tokio::time::Duration::from_secs(60);
        let result = tokio::time::timeout(timeout, async {
            while let Some(line) = lines.next_line().await.transpose() {
                let line = line.map_err(|e| FerroError::Mcp(format!("Read error: {e}")))?;

                if let Ok(response) = serde_json::from_str::<Value>(&line)
                    && response.get("id") == Some(&json!(2))
                {
                    if let Some(result) = response.get("result") {
                        // Extract text content from content array
                        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
                            let text: String = content
                                .iter()
                                .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join("\n");
                            let _ = child.kill().await;
                            return Ok(text);
                        }
                    }
                    if let Some(error) = response.get("error") {
                        let msg = error
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Unknown error");
                        let _ = child.kill().await;
                        return Err(FerroError::Tool(msg.to_string()));
                    }
                }
            }
            Err(FerroError::Mcp("No response from tool call".into()))
        })
        .await;

        let _ = child.kill().await;

        match result {
            Ok(r) => r,
            Err(_) => Err(FerroError::Mcp("Tool call timed out".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_server_names() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            McpServerConfig {
                command: Some("echo".into()),
                args: vec![],
                env: HashMap::new(),
                url: None,
                headers: HashMap::new(),
                cache_ttl_seconds: 3600,
            },
        );
        let client = McpClient::new(servers, 50000);
        assert_eq!(client.server_names(), vec!["test"]);
    }
}
