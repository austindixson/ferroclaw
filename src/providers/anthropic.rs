use crate::error::{FerroError, Result};
use crate::provider::{BoxFuture, LlmProvider};
// streaming module available for future SSE support
use crate::types::{
    Message, MessageContent, ProviderResponse, Role, TokenUsage, ToolCall, ToolDefinition,
};
use reqwest::Client;
use serde_json::{Value, json};

pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    #[allow(dead_code)]
    max_tokens: u32,
    request_timeout_ms: u64,
    max_retries: u32,
    no_retry_max_tokens_threshold: u32,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(
        api_key: String,
        base_url: String,
        max_tokens: u32,
        request_timeout_ms: u64,
        max_retries: u32,
        no_retry_max_tokens_threshold: u32,
    ) -> Self {
        Self {
            api_key,
            base_url,
            max_tokens,
            request_timeout_ms,
            max_retries,
            no_retry_max_tokens_threshold,
            client: Client::new(),
        }
    }

    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        max_tokens: u32,
    ) -> Value {
        let system_msgs: Vec<&Message> =
            messages.iter().filter(|m| m.role == Role::System).collect();
        let non_system: Vec<Value> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| self.format_message(m))
            .collect();

        let system_text = system_msgs
            .iter()
            .map(|m| m.text())
            .collect::<Vec<&str>>()
            .join("\n\n");

        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": non_system,
        });

        if !system_text.is_empty() {
            body["system"] = json!(system_text);
        }

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = json!(tool_defs);
        }

        body
    }

    fn format_message(&self, msg: &Message) -> Value {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "user",
            Role::System => unreachable!("System messages filtered before this point"),
        };

        // Tool results are sent as user messages with tool_result content blocks
        if msg.role == Role::Tool
            && let Some(tool_call_id) = &msg.tool_call_id
        {
            return json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": msg.text(),
                }]
            });
        }

        // Assistant messages with tool calls
        if msg.role == Role::Assistant
            && let Some(tool_calls) = &msg.tool_calls
        {
            let mut content_blocks: Vec<Value> = Vec::new();
            let text = msg.text();
            if !text.is_empty() {
                content_blocks.push(json!({"type": "text", "text": text}));
            }

            for tc in tool_calls {
                content_blocks.push(json!({
                    "type": "tool_use",
                    "id": tc.id,
                    "name": tc.name,
                    "input": tc.arguments,
                }));
            }

            return json!({
                "role": "assistant",
                "content": content_blocks,
            });
        }

        json!({
            "role": role,
            "content": msg.text(),
        })
    }

    fn parse_response(&self, body: &Value) -> Result<ProviderResponse> {
        let content = body
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| FerroError::Provider("Missing content in response".into()))?;

        let mut text = String::new();
        let mut tool_calls = Vec::new();

        for block in content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => {
                    let id = block
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let input = block.get("input").cloned().unwrap_or(json!({}));
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
                _ => {}
            }
        }

        let usage = body.get("usage").map(|u| TokenUsage {
            input_tokens: u.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0),
            output_tokens: u.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0),
        });

        let stop_reason = body
            .get("stop_reason")
            .and_then(|s| s.as_str())
            .map(String::from);

        let message = if tool_calls.is_empty() {
            Message::assistant(text)
        } else {
            let mut msg = Message::assistant_with_tool_calls(tool_calls);
            if !text.is_empty() {
                msg.content = MessageContent::Text(text);
            }
            msg
        };

        Ok(ProviderResponse {
            message,
            usage,
            stop_reason,
        })
    }
}

impl LlmProvider for AnthropicProvider {
    fn complete<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [ToolDefinition],
        model: &'a str,
        max_tokens: u32,
    ) -> BoxFuture<'a, Result<ProviderResponse>> {
        Box::pin(async move {
            let body = self.build_request_body(messages, tools, model, max_tokens);
            let configured_attempts = self.max_retries.max(1) as usize;
            let max_attempts = if max_tokens <= self.no_retry_max_tokens_threshold {
                1usize
            } else {
                configured_attempts
            };
            let mut last_err: Option<FerroError> = None;

            for attempt in 1..=max_attempts {
                let response = match self
                    .client
                    .post(format!("{}/v1/messages", self.base_url))
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("content-type", "application/json")
                    .timeout(tokio::time::Duration::from_millis(self.request_timeout_ms))
                    .json(&body)
                    .send()
                    .await
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        let retryable = e.is_timeout() || e.is_connect() || e.is_request();
                        let ferr = FerroError::Provider(format!(
                            "Anthropic HTTP request failed (attempt {attempt}/{max_attempts}): {e}"
                        ));
                        if retryable && attempt < max_attempts {
                            last_err = Some(ferr);
                            tokio::time::sleep(tokio::time::Duration::from_millis(
                                250 * attempt as u64,
                            ))
                            .await;
                            continue;
                        }
                        return Err(ferr);
                    }
                };

                let status = response.status();
                let response_body: Value = response
                    .json()
                    .await
                    .map_err(|e| FerroError::Provider(format!("Failed to parse response: {e}")))?;

                if !status.is_success() {
                    let error_msg = response_body
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error");
                    let retryable_status = status.as_u16() == 408
                        || status.as_u16() == 409
                        || status.as_u16() == 429
                        || status.is_server_error();
                    let ferr = FerroError::Provider(format!(
                        "Anthropic API error ({status}) attempt {attempt}/{max_attempts}: {error_msg}"
                    ));
                    if retryable_status && attempt < max_attempts {
                        last_err = Some(ferr);
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            250 * attempt as u64,
                        ))
                        .await;
                        continue;
                    }
                    return Err(ferr);
                }

                return self.parse_response(&response_body);
            }

            Err(last_err.unwrap_or_else(|| {
                FerroError::Provider("Anthropic request failed after retries".into())
            }))
        })
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn supports_model(&self, model: &str) -> bool {
        model.starts_with("claude-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_user_message() {
        let provider = AnthropicProvider::new(
            "test".into(),
            "https://api.anthropic.com".into(),
            8192,
            15_000,
            2,
            128,
        );
        let msg = Message::user("Hello");
        let formatted = provider.format_message(&msg);
        assert_eq!(formatted["role"], "user");
        assert_eq!(formatted["content"], "Hello");
    }

    #[test]
    fn test_format_tool_result() {
        let provider = AnthropicProvider::new(
            "test".into(),
            "https://api.anthropic.com".into(),
            8192,
            15_000,
            2,
            128,
        );
        let msg = Message::tool_result("tc_123", "file contents here");
        let formatted = provider.format_message(&msg);
        assert_eq!(formatted["role"], "user");
        assert_eq!(formatted["content"][0]["type"], "tool_result");
        assert_eq!(formatted["content"][0]["tool_use_id"], "tc_123");
    }

    #[test]
    fn test_parse_response_text_only() {
        let provider = AnthropicProvider::new(
            "test".into(),
            "https://api.anthropic.com".into(),
            8192,
            15_000,
            2,
            128,
        );
        let body = json!({
            "content": [{"type": "text", "text": "Hello!"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "stop_reason": "end_turn"
        });
        let result = provider.parse_response(&body).unwrap();
        assert_eq!(result.message.text(), "Hello!");
        assert!(result.message.tool_calls.is_none());
    }

    #[test]
    fn test_parse_response_with_tool_use() {
        let provider = AnthropicProvider::new(
            "test".into(),
            "https://api.anthropic.com".into(),
            8192,
            15_000,
            2,
            128,
        );
        let body = json!({
            "content": [
                {"type": "text", "text": "Let me read that file."},
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "name": "read_file",
                    "input": {"path": "/tmp/test.txt"}
                }
            ],
            "usage": {"input_tokens": 50, "output_tokens": 20},
            "stop_reason": "tool_use"
        });
        let result = provider.parse_response(&body).unwrap();
        let tool_calls = result.message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "read_file");
    }

    #[test]
    fn test_build_request_body() {
        let provider = AnthropicProvider::new(
            "test".into(),
            "https://api.anthropic.com".into(),
            8192,
            15_000,
            2,
            128,
        );
        let messages = vec![Message::system("You are helpful."), Message::user("Hello")];
        let tools = vec![ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
            server_name: None,
        }];

        let body = provider.build_request_body(&messages, &tools, "claude-sonnet-4-20250514", 8192);
        assert_eq!(body["system"], "You are helpful.");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1); // Only user msg
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
    }
}
