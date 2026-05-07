use crate::error::{FerroError, Result};
use crate::provider::{BoxFuture, LlmProvider};
use crate::types::{
    Message, MessageContent, ProviderResponse, Role, TokenUsage, ToolCall, ToolDefinition,
};
use reqwest::Client;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiApiMode {
    ChatCompletions,
    CodexResponses,
}

/// OpenAI-compatible provider. Works with OpenAI, OpenRouter, Ollama, and any
/// endpoint that implements the OpenAI chat completions API.
///
/// For Hermes parity, OpenAI Codex can run in Responses mode against
/// `https://chatgpt.com/backend-api/codex`.
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    api_mode: OpenAiApiMode,
    #[allow(dead_code)]
    max_tokens: u32,
    request_timeout_ms: u64,
    max_retries: u32,
    no_retry_max_tokens_threshold: u32,
    client: Client,
}

struct NvidiaNimRateState {
    recent: VecDeque<Instant>,
    last: Option<Instant>,
}

const NVIDIA_NIM_REQUESTS_PER_MINUTE: usize = 40;
const NVIDIA_NIM_WINDOW: Duration = Duration::from_secs(60);
const NVIDIA_NIM_MIN_INTERVAL: Duration = Duration::from_millis(1600);

static NVIDIA_NIM_RATE_STATE: OnceLock<AsyncMutex<NvidiaNimRateState>> = OnceLock::new();

fn nvidia_nim_state() -> &'static AsyncMutex<NvidiaNimRateState> {
    NVIDIA_NIM_RATE_STATE.get_or_init(|| {
        AsyncMutex::new(NvidiaNimRateState {
            recent: VecDeque::new(),
            last: None,
        })
    })
}

fn reserve_min_interval_slot(
    last: Option<Instant>,
    now: Instant,
    min_interval: Duration,
) -> Option<Duration> {
    let last = last?;
    let elapsed = now.saturating_duration_since(last);
    if elapsed >= min_interval {
        None
    } else {
        Some(min_interval.saturating_sub(elapsed))
    }
}

fn reserve_rate_limit_slot(
    timestamps: &mut VecDeque<Instant>,
    now: Instant,
    max_requests: usize,
    window: Duration,
) -> Option<Duration> {
    while let Some(front) = timestamps.front() {
        if now.saturating_duration_since(*front) >= window {
            timestamps.pop_front();
        } else {
            break;
        }
    }

    if timestamps.len() < max_requests {
        timestamps.push_back(now);
        return None;
    }

    let oldest = *timestamps.front().expect("timestamps should not be empty");
    Some(window.saturating_sub(now.saturating_duration_since(oldest)))
}

impl OpenAiProvider {
    pub fn new(
        api_key: String,
        base_url: String,
        api_mode: OpenAiApiMode,
        max_tokens: u32,
        request_timeout_ms: u64,
        max_retries: u32,
        no_retry_max_tokens_threshold: u32,
    ) -> Self {
        Self {
            api_key,
            base_url,
            api_mode,
            max_tokens,
            request_timeout_ms,
            max_retries,
            no_retry_max_tokens_threshold,
            client: Client::new(),
        }
    }

    pub fn is_codex_backend_url(base_url: &str) -> bool {
        base_url
            .to_ascii_lowercase()
            .contains("chatgpt.com/backend-api/codex")
    }

    fn is_nvidia_nim_base_url(base_url: &str) -> bool {
        base_url
            .to_ascii_lowercase()
            .contains("integrate.api.nvidia.com")
    }

    async fn enforce_nvidia_nim_rate_limit(&self) {
        if !Self::is_nvidia_nim_base_url(&self.base_url) {
            return;
        }

        loop {
            let maybe_wait = {
                let mut state = nvidia_nim_state().lock().await;
                let now = Instant::now();

                if let Some(wait) =
                    reserve_min_interval_slot(state.last, now, NVIDIA_NIM_MIN_INTERVAL)
                {
                    Some(wait)
                } else if let Some(wait) = reserve_rate_limit_slot(
                    &mut state.recent,
                    now,
                    NVIDIA_NIM_REQUESTS_PER_MINUTE,
                    NVIDIA_NIM_WINDOW,
                ) {
                    Some(wait)
                } else {
                    state.last = Some(now);
                    None
                }
            };

            if let Some(wait) = maybe_wait {
                tokio::time::sleep(wait).await;
            } else {
                return;
            }
        }
    }

    fn parse_retry_after_header(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
        let value = headers.get("retry-after")?.to_str().ok()?.trim();
        let secs = value.parse::<u64>().ok()?;
        Some(Duration::from_secs(secs.max(1)))
    }

    fn build_chat_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        max_tokens: u32,
    ) -> Value {
        let formatted_messages: Vec<Value> = messages
            .iter()
            .map(|m| self.format_chat_message(m))
            .collect();

        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": formatted_messages,
        });

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tool_defs);
        }

        body
    }

    fn format_chat_message(&self, msg: &Message) -> Value {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        if msg.role == Role::Tool {
            return json!({
                "role": "tool",
                "tool_call_id": msg.tool_call_id,
                "content": msg.text(),
            });
        }

        if msg.role == Role::Assistant
            && let Some(tool_calls) = &msg.tool_calls
        {
            let tc_json: Vec<Value> = tool_calls
                .iter()
                .map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments.to_string(),
                        }
                    })
                })
                .collect();
            return json!({
                "role": "assistant",
                "content": msg.text(),
                "tool_calls": tc_json,
            });
        }

        json!({
            "role": role,
            "content": msg.text(),
        })
    }

    fn build_codex_responses_body(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
    ) -> Value {
        let mut instructions = String::new();
        let mut start_idx = 0usize;
        if let Some(first) = messages.first()
            && first.role == Role::System
        {
            instructions = first.text().trim().to_string();
            start_idx = 1;
        }

        let mut input_items: Vec<Value> = Vec::new();
        for msg in messages.iter().skip(start_idx) {
            match msg.role {
                Role::System => {
                    if !msg.text().is_empty() {
                        input_items.push(json!({
                            "role": "user",
                            "content": msg.text(),
                        }));
                    }
                }
                Role::User => {
                    input_items.push(json!({
                        "role": "user",
                        "content": msg.text(),
                    }));
                }
                Role::Assistant => {
                    if !msg.text().is_empty() {
                        input_items.push(json!({
                            "role": "assistant",
                            "content": msg.text(),
                        }));
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tc in tool_calls {
                            input_items.push(json!({
                                "type": "function_call",
                                "call_id": tc.id,
                                "name": tc.name,
                                "arguments": tc.arguments.to_string(),
                            }));
                        }
                    }
                }
                Role::Tool => {
                    if let Some(call_id) = &msg.tool_call_id {
                        input_items.push(json!({
                            "type": "function_call_output",
                            "call_id": call_id,
                            "output": msg.text(),
                        }));
                    }
                }
            }
        }

        let tool_defs: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })
            })
            .collect();

        let mut body = json!({
            "model": model,
            "instructions": instructions,
            "input": input_items,
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "store": false,
        });

        if !tool_defs.is_empty() {
            body["tools"] = json!(tool_defs);
        }

        body
    }

    fn parse_chat_response(&self, body: &Value) -> Result<ProviderResponse> {
        let choice = body
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .ok_or_else(|| FerroError::Provider("No choices in response".into()))?;

        let message = choice
            .get("message")
            .ok_or_else(|| FerroError::Provider("No message in choice".into()))?;

        let text =
            Self::parse_responses_output_text(message.get("content").unwrap_or(&Value::Null));

        let mut tool_calls: Vec<ToolCall> = message
            .get("tool_calls")
            .and_then(|tc| tc.as_array())
            .map(|tcs| {
                tcs.iter()
                    .filter_map(|tc| {
                        let id = tc.get("id")?.as_str()?.to_string();
                        let func = tc.get("function")?;
                        let name = func.get("name")?.as_str()?.to_string();
                        let args_str = func.get("arguments")?.as_str()?;
                        let arguments: Value = serde_json::from_str(args_str).ok()?;
                        Some(ToolCall {
                            id,
                            name,
                            arguments,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        if tool_calls.is_empty()
            && let Some(fc) = message.get("function_call")
            && let Some(name) = fc.get("name").and_then(|v| v.as_str())
        {
            let args_str = fc.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
            let arguments: Value = serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));
            tool_calls.push(ToolCall {
                id: format!("fc_{}", name),
                name: name.to_string(),
                arguments,
            });
        }

        let usage = body.get("usage").map(|u| TokenUsage {
            input_tokens: u
                .get("prompt_tokens")
                .or_else(|| u.get("input_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0),
            output_tokens: u
                .get("completion_tokens")
                .or_else(|| u.get("output_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0),
        });

        let stop_reason = choice
            .get("finish_reason")
            .and_then(|s| s.as_str())
            .map(String::from);

        let msg = if tool_calls.is_empty() {
            Message::assistant(text)
        } else {
            let mut msg = Message::assistant_with_tool_calls(tool_calls);
            if !text.is_empty() {
                msg.content = MessageContent::Text(text);
            }
            msg
        };

        Ok(ProviderResponse {
            message: msg,
            usage,
            stop_reason,
        })
    }

    fn parse_responses_output_text(content: &Value) -> String {
        if let Some(s) = content.as_str() {
            return s.to_string();
        }
        let mut out = String::new();
        if let Some(arr) = content.as_array() {
            for item in arr {
                let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match item_type {
                    "output_text" | "text" => {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            out.push_str(t);
                        }
                    }
                    _ => {
                        if let Some(t) = item
                            .get("text")
                            .and_then(|v| v.get("value"))
                            .and_then(|v| v.as_str())
                        {
                            out.push_str(t);
                        }
                    }
                }
            }
        }
        out
    }

    fn parse_codex_responses_response(&self, body: &Value) -> Result<ProviderResponse> {
        let mut text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        if let Some(output) = body.get("output").and_then(|v| v.as_array()) {
            for item in output {
                let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match item_type {
                    "message" => {
                        if item.get("role").and_then(|v| v.as_str()).unwrap_or("") == "assistant" {
                            let extracted = Self::parse_responses_output_text(
                                item.get("content").unwrap_or(&Value::Null),
                            );
                            if !extracted.is_empty() {
                                text.push_str(&extracted);
                            }
                        }
                    }
                    "function_call" => {
                        let id = item
                            .get("call_id")
                            .and_then(|v| v.as_str())
                            .or_else(|| item.get("id").and_then(|v| v.as_str()))
                            .unwrap_or("")
                            .to_string();
                        let name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args_str = item
                            .get("arguments")
                            .and_then(|v| v.as_str())
                            .unwrap_or("{}");
                        if !id.is_empty() && !name.is_empty() {
                            let arguments: Value = serde_json::from_str(args_str)
                                .unwrap_or_else(|_| json!({ "raw": args_str }));
                            tool_calls.push(ToolCall {
                                id,
                                name,
                                arguments,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        if text.is_empty()
            && let Some(s) = body.get("output_text").and_then(|v| v.as_str())
        {
            text = s.to_string();
        }

        let usage = body.get("usage").map(|u| TokenUsage {
            input_tokens: u
                .get("input_tokens")
                .or_else(|| u.get("prompt_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0),
            output_tokens: u
                .get("output_tokens")
                .or_else(|| u.get("completion_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0),
        });

        let stop_reason = body
            .get("status")
            .and_then(|s| s.as_str())
            .map(str::to_string);

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

    fn parse_error_message_from_jsonish(raw: &str) -> String {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            return v
                .get("error")
                .and_then(|e| {
                    e.get("message")
                        .or_else(|| e.get("error"))
                        .or_else(|| e.get("code"))
                })
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string());
        }
        raw.to_string()
    }

    fn parse_codex_sse_response(raw: &str) -> Result<Value> {
        let mut completed: Option<Value> = None;
        let mut text_buf = String::new();
        let mut output_items: Vec<Value> = Vec::new();

        for line in raw.lines() {
            let line = line.trim();
            if !line.starts_with("data:") {
                continue;
            }
            let payload = line.trim_start_matches("data:").trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }

            let Ok(event) = serde_json::from_str::<Value>(payload) else {
                continue;
            };

            let typ = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match typ {
                "response.completed" => {
                    if let Some(resp) = event.get("response") {
                        completed = Some(resp.clone());
                    }
                }
                "response.output_text.delta" => {
                    if let Some(delta) = event.get("delta").and_then(|v| v.as_str()) {
                        text_buf.push_str(delta);
                    }
                }
                "response.output_item.done" => {
                    if let Some(item) = event.get("item") {
                        output_items.push(item.clone());
                    }
                }
                "error" => {
                    return Err(FerroError::Provider(format!(
                        "Codex stream error: {}",
                        event
                    )));
                }
                _ => {}
            }
        }

        if let Some(mut resp) = completed {
            let output_empty = resp
                .get("output")
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(true);

            if output_empty && !output_items.is_empty() {
                resp["output"] = Value::Array(output_items.clone());
            }

            let output_still_empty = resp
                .get("output")
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(true);
            if output_still_empty && !text_buf.is_empty() {
                resp["output"] = json!([
                    {
                        "type": "message",
                        "role": "assistant",
                        "content": [
                            {
                                "type": "output_text",
                                "text": text_buf,
                            }
                        ]
                    }
                ]);
            }
            return Ok(resp);
        }

        if !output_items.is_empty() {
            return Ok(json!({
                "status": "completed",
                "output": output_items,
            }));
        }

        if !text_buf.is_empty() {
            return Ok(json!({
                "status": "completed",
                "output": [
                    {
                        "type": "message",
                        "role": "assistant",
                        "content": [
                            {
                                "type": "output_text",
                                "text": text_buf,
                            }
                        ]
                    }
                ]
            }));
        }

        Err(FerroError::Provider(
            "Codex stream returned no response.completed event".into(),
        ))
    }
}

impl LlmProvider for OpenAiProvider {
    fn complete<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [ToolDefinition],
        model: &'a str,
        max_tokens: u32,
    ) -> BoxFuture<'a, Result<ProviderResponse>> {
        Box::pin(async move {
            let (url, mut body) = match self.api_mode {
                OpenAiApiMode::ChatCompletions => (
                    format!("{}/chat/completions", self.base_url),
                    self.build_chat_request_body(messages, tools, model, max_tokens),
                ),
                OpenAiApiMode::CodexResponses => (
                    format!("{}/responses", self.base_url),
                    self.build_codex_responses_body(messages, tools, model),
                ),
            };

            if self.api_mode == OpenAiApiMode::CodexResponses {
                body["stream"] = json!(true);
            }

            let configured_attempts = self.max_retries.max(1) as usize;
            let configured_attempts = if Self::is_nvidia_nim_base_url(&self.base_url) {
                configured_attempts.max(4)
            } else {
                configured_attempts
            };
            let max_attempts = if max_tokens <= self.no_retry_max_tokens_threshold {
                1usize
            } else {
                configured_attempts
            };
            let mut last_err: Option<FerroError> = None;

            for attempt in 1..=max_attempts {
                self.enforce_nvidia_nim_rate_limit().await;
                let mut req = self
                    .client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .header("Content-Type", "application/json")
                    // Some Codex/OAuth backends intermittently emit malformed compressed SSE bodies.
                    // For streaming responses, request identity encoding to avoid decode-body failures.
                    .header("Accept-Encoding", "identity")
                    .timeout(tokio::time::Duration::from_millis(self.request_timeout_ms))
                    .json(&body);

                if self.api_mode == OpenAiApiMode::CodexResponses {
                    req = req.header("Accept", "text/event-stream");
                }

                let response = match req
                    .send()
                    .await
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        let retryable = e.is_timeout() || e.is_connect() || e.is_request();
                        let ferr = FerroError::Provider(format!(
                            "OpenAI HTTP request failed (attempt {attempt}/{max_attempts}): {e}"
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
                let retry_after = if status.as_u16() == 429 {
                    Self::parse_retry_after_header(response.headers())
                } else {
                    None
                };

                let (response_body, error_msg) = if self.api_mode == OpenAiApiMode::CodexResponses {
                    let raw = match response.text().await {
                        Ok(raw) => raw,
                        Err(e) => {
                            let ferr = FerroError::Provider(format!(
                                "Failed to read codex stream (attempt {attempt}/{max_attempts}, status {status}): {e}"
                            ));
                            if attempt < max_attempts {
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

                    if !status.is_success() {
                        (None, Some(Self::parse_error_message_from_jsonish(&raw)))
                    } else {
                        let parsed = Self::parse_codex_sse_response(&raw)?;
                        (Some(parsed), None)
                    }
                } else {
                    let json_body: Value = response.json().await.map_err(|e| {
                        FerroError::Provider(format!("Failed to parse response: {e}"))
                    })?;
                    if !status.is_success() {
                        let msg = json_body
                            .get("error")
                            .and_then(|e| {
                                e.get("message")
                                    .or_else(|| e.get("error"))
                                    .or_else(|| e.get("code"))
                            })
                            .and_then(|m| m.as_str())
                            .map(str::to_string)
                            .unwrap_or_else(|| json_body.to_string());
                        (None, Some(msg))
                    } else {
                        (Some(json_body), None)
                    }
                };

                if let Some(error_msg) = error_msg {
                    let retryable_status = status.as_u16() == 408
                        || status.as_u16() == 409
                        || status.as_u16() == 429
                        || status.is_server_error();
                    let mut rendered = format!(
                        "OpenAI API error ({status}) attempt {attempt}/{max_attempts}: {error_msg}"
                    );
                    if error_msg.contains("Missing scopes") {
                        rendered.push_str("\nHint: for Hermes parity, configure providers.openai_codex with base_url = \"https://chatgpt.com/backend-api/codex\" and auth_mode = \"oauth\". This endpoint uses Codex OAuth responses flow instead of OpenAI API project scopes.");
                    }
                    let ferr = FerroError::Provider(rendered);
                    if retryable_status && attempt < max_attempts {
                        last_err = Some(ferr);
                        let retry_sleep = if status.as_u16() == 429 {
                            retry_after.unwrap_or(Duration::from_secs(2))
                        } else {
                            Duration::from_millis(250 * attempt as u64)
                        };
                        tokio::time::sleep(retry_sleep).await;
                        continue;
                    }
                    return Err(ferr);
                }

                let response_body = response_body.expect("response body should exist on success");
                return match self.api_mode {
                    OpenAiApiMode::ChatCompletions => self.parse_chat_response(&response_body),
                    OpenAiApiMode::CodexResponses => {
                        self.parse_codex_responses_response(&response_body)
                    }
                };
            }

            Err(last_err.unwrap_or_else(|| {
                FerroError::Provider("OpenAI request failed after retries".into())
            }))
        })
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn supports_model(&self, model: &str) -> bool {
        // OpenAI-compatible endpoints accept any model string
        !model.starts_with("claude-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openai_response() {
        let provider = OpenAiProvider::new(
            "test".into(),
            "https://api.openai.com/v1".into(),
            OpenAiApiMode::ChatCompletions,
            4096,
            15_000,
            2,
            128,
        );
        let body = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5
            }
        });
        let result = provider.parse_chat_response(&body).unwrap();
        assert_eq!(result.message.text(), "Hello!");
    }

    #[test]
    fn test_parse_openai_response_content_array() {
        let provider = OpenAiProvider::new(
            "test".into(),
            "https://api.openai.com/v1".into(),
            OpenAiApiMode::ChatCompletions,
            4096,
            15_000,
            2,
            128,
        );
        let body = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "Hello "},
                        {"type": "output_text", "text": "world"}
                    ]
                },
                "finish_reason": "stop"
            }]
        });
        let result = provider.parse_chat_response(&body).unwrap();
        assert_eq!(result.message.text(), "Hello world");
    }

    #[test]
    fn test_parse_openai_tool_call() {
        let provider = OpenAiProvider::new(
            "test".into(),
            "https://api.openai.com/v1".into(),
            OpenAiApiMode::ChatCompletions,
            4096,
            15_000,
            2,
            128,
        );
        let body = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"/tmp/test.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let result = provider.parse_chat_response(&body).unwrap();
        let tcs = result.message.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].name, "read_file");
    }

    #[test]
    fn test_parse_openai_legacy_function_call() {
        let provider = OpenAiProvider::new(
            "test".into(),
            "https://api.openai.com/v1".into(),
            OpenAiApiMode::ChatCompletions,
            4096,
            15_000,
            2,
            128,
        );
        let body = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "",
                    "function_call": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"/tmp/test.txt\"}"
                    }
                },
                "finish_reason": "function_call"
            }]
        });
        let result = provider.parse_chat_response(&body).unwrap();
        let tcs = result.message.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].name, "read_file");
    }

    #[test]
    fn test_parse_codex_responses_function_call() {
        let provider = OpenAiProvider::new(
            "test".into(),
            "https://chatgpt.com/backend-api/codex".into(),
            OpenAiApiMode::CodexResponses,
            4096,
            15_000,
            2,
            128,
        );
        let body = json!({
            "status": "completed",
            "output": [
                {
                    "type": "function_call",
                    "call_id": "call_abc",
                    "name": "read_file",
                    "arguments": "{\"path\":\"/tmp/test.txt\"}"
                }
            ],
            "usage": {
                "input_tokens": 3,
                "output_tokens": 7
            }
        });

        let result = provider.parse_codex_responses_response(&body).unwrap();
        let tcs = result.message.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "call_abc");
        assert_eq!(tcs[0].name, "read_file");
    }

    #[test]
    fn test_parse_codex_sse_with_output_item_done_function_call() {
        let raw = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"output\":[]}}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"run_shell_command\",\"arguments\":\"{\\\"command\\\":\\\"echo hi\\\"}\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"output\":[]}}\n\n",
            "data: [DONE]\n"
        );

        let parsed = OpenAiProvider::parse_codex_sse_response(raw).unwrap();
        let output = parsed.get("output").and_then(|v| v.as_array()).unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].get("type").and_then(|v| v.as_str()),
            Some("function_call")
        );
        assert_eq!(
            output[0].get("call_id").and_then(|v| v.as_str()),
            Some("call_1")
        );
    }

    #[test]
    fn test_reserve_rate_limit_slot_under_limit_reserves_immediately() {
        let mut timestamps = VecDeque::new();
        let now = Instant::now();

        let wait = reserve_rate_limit_slot(
            &mut timestamps,
            now,
            NVIDIA_NIM_REQUESTS_PER_MINUTE,
            NVIDIA_NIM_WINDOW,
        );

        assert!(wait.is_none());
        assert_eq!(timestamps.len(), 1);
    }

    #[test]
    fn test_reserve_rate_limit_slot_at_limit_requires_wait() {
        let now = Instant::now();
        let oldest = now - Duration::from_secs(30);
        let mut timestamps = VecDeque::from(vec![oldest; NVIDIA_NIM_REQUESTS_PER_MINUTE]);

        let wait = reserve_rate_limit_slot(
            &mut timestamps,
            now,
            NVIDIA_NIM_REQUESTS_PER_MINUTE,
            NVIDIA_NIM_WINDOW,
        );

        assert_eq!(wait, Some(Duration::from_secs(30)));
        assert_eq!(timestamps.len(), NVIDIA_NIM_REQUESTS_PER_MINUTE);
    }

    #[test]
    fn test_reserve_rate_limit_slot_prunes_expired_entries() {
        let now = Instant::now();
        let expired = now - Duration::from_secs(61);
        let mut timestamps = VecDeque::from(vec![expired; NVIDIA_NIM_REQUESTS_PER_MINUTE]);

        let wait = reserve_rate_limit_slot(
            &mut timestamps,
            now,
            NVIDIA_NIM_REQUESTS_PER_MINUTE,
            NVIDIA_NIM_WINDOW,
        );

        assert!(wait.is_none());
        assert_eq!(timestamps.len(), 1);
    }

    #[test]
    fn test_reserve_min_interval_slot_requires_wait_inside_interval() {
        let now = Instant::now();
        let last = now - Duration::from_millis(500);

        let wait = reserve_min_interval_slot(Some(last), now, NVIDIA_NIM_MIN_INTERVAL);

        assert_eq!(wait, Some(Duration::from_millis(1100)));
    }

    #[test]
    fn test_reserve_min_interval_slot_allows_after_interval() {
        let now = Instant::now();
        let last = now - Duration::from_secs(3);

        let wait = reserve_min_interval_slot(Some(last), now, NVIDIA_NIM_MIN_INTERVAL);

        assert!(wait.is_none());
    }
}
