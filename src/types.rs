use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant_with_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text(String::new()),
            tool_call_id: None,
            tool_calls: Some(tool_calls),
        }
    }

    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: MessageContent::Text(content.into()),
            tool_call_id: Some(call_id.into()),
            tool_calls: None,
        }
    }

    pub fn text(&self) -> &str {
        match &self.content {
            MessageContent::Text(t) => t,
            MessageContent::Blocks(_) => "",
        }
    }

    pub fn estimated_tokens(&self) -> u64 {
        let text_len = match &self.content {
            MessageContent::Text(t) => t.len(),
            MessageContent::Blocks(blocks) => blocks.iter().map(|b| b.estimated_size()).sum(),
        };
        // Rough estimate: 1 token ≈ 4 chars
        (text_len as u64) / 4 + 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

impl ContentBlock {
    pub fn estimated_size(&self) -> usize {
        match self {
            Self::Text { text } => text.len(),
            Self::ToolUse { name, input, .. } => name.len() + input.to_string().len(),
            Self::ToolResult { content, .. } => content.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

impl ToolDefinition {
    pub fn compact_signature(&self) -> String {
        let props = self
            .input_schema
            .get("properties")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();

        let required: Vec<String> = self
            .input_schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let parts: Vec<String> = props
            .iter()
            .map(|(name, schema)| {
                let type_hint = json_type_to_hint(schema);
                let prefix = if required.contains(name) { "" } else { "?" };
                format!("{prefix}{name}: {type_hint}")
            })
            .collect();

        format!("{}({})", self.name, parts.join(", "))
    }

    pub fn required_params(&self) -> Vec<String> {
        self.input_schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn json_type_to_hint(schema: &serde_json::Value) -> String {
    let json_type = schema.get("type").and_then(|t| t.as_str()).unwrap_or("any");

    if let Some(enum_vals) = schema.get("enum").and_then(|e| e.as_array()) {
        let vals: Vec<String> = enum_vals
            .iter()
            .take(5)
            .map(|v| format!("\"{}\"", v.as_str().unwrap_or("?")))
            .collect();
        let suffix = if enum_vals.len() > 5 { " | ..." } else { "" };
        return format!("{}{}", vals.join(" | "), suffix);
    }

    if json_type == "array"
        && let Some(items) = schema.get("items")
    {
        let item_type = json_type_to_hint(items);
        return format!("list[{item_type}]");
    }

    match json_type {
        "string" => "str".into(),
        "integer" => "int".into(),
        "number" => "float".into(),
        "boolean" => "bool".into(),
        "array" => "list".into(),
        "object" => "dict".into(),
        other => other.into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub event_type: StreamEventType,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEventType {
    TextDelta,
    ToolCallStart,
    ToolCallDelta,
    ToolCallEnd,
    Done,
    Error,
}

#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub message: Message,
    pub usage: Option<TokenUsage>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStopReason {
    AssistantFinal,
    BudgetIterations,
    BudgetTokens,
    BudgetWallClock,
    BudgetToolsIteration,
    BudgetToolsTotal,
    ErrorNonRetryable,
    ErrorRetryExhausted,
    ErrorEmptyFinalAfterTools,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStopContract {
    pub reason: RunStopReason,
    pub iterations: u32,
    pub tool_calls_total: u32,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutcome {
    pub text: String,
    pub stop: RunStopContract,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub tool_calls: u32,
}

/// Capabilities that tools can require and sessions can grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    FsRead,
    FsWrite,
    NetOutbound,
    NetListen,
    ProcessExec,
    MemoryRead,
    MemoryWrite,
    BrowserControl,
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FsRead => write!(f, "fs_read"),
            Self::FsWrite => write!(f, "fs_write"),
            Self::NetOutbound => write!(f, "net_outbound"),
            Self::NetListen => write!(f, "net_listen"),
            Self::ProcessExec => write!(f, "process_exec"),
            Self::MemoryRead => write!(f, "memory_read"),
            Self::MemoryWrite => write!(f, "memory_write"),
            Self::BrowserControl => write!(f, "browser_control"),
        }
    }
}

/// Metadata attached to a registered tool for security enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMeta {
    pub definition: ToolDefinition,
    pub required_capabilities: Vec<Capability>,
    pub source: ToolSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSource {
    Builtin,
    Mcp { server: String },
    Skill { path: String },
}

/// Session-scoped capability set.
#[derive(Debug, Clone)]
pub struct CapabilitySet {
    pub capabilities: std::collections::HashSet<Capability>,
}

impl CapabilitySet {
    pub fn new(caps: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            capabilities: caps.into_iter().collect(),
        }
    }

    pub fn all() -> Self {
        use Capability::*;
        Self::new([
            FsRead,
            FsWrite,
            NetOutbound,
            NetListen,
            ProcessExec,
            MemoryRead,
            MemoryWrite,
            BrowserControl,
        ])
    }

    pub fn has(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    pub fn check(&self, required: &[Capability]) -> std::result::Result<(), Capability> {
        for cap in required {
            if !self.has(*cap) {
                return Err(*cap);
            }
        }
        Ok(())
    }
}
