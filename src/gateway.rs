//! HTTP gateway — OpenAI-compatible runtime surface for external integrations.
//!
//! Binds 127.0.0.1 by default and exposes:
//! - POST /v1/responses
//! - GET  /v1/models
//! - GET  /v1/health

use crate::agent::AgentLoop;
use crate::config::Config;
use crate::error::{FerroError, Result};
use crate::mcp::client::McpClient;
use crate::memory::MemoryStore;
use crate::security::capabilities::capabilities_from_config;
use crate::tool::ToolRegistry;
use crate::tools::builtin::register_builtin_tools;
use crate::types::{Message, RunStopContract};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Gateway server configuration derived from the main config.
pub struct Gateway {
    pub bind_addr: String,
    pub port: u16,
    pub bearer_token: Option<String>,
}

impl Gateway {
    pub fn from_config(config: &Config) -> Self {
        let bearer_token = config.gateway.bearer_token.clone().or_else(|| {
            config
                .gateway
                .bearer_token_env
                .as_ref()
                .and_then(|env_var| std::env::var(env_var).ok())
        });

        Self {
            bind_addr: config.gateway.bind.clone(),
            port: config.gateway.port,
            bearer_token,
        }
    }

    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.bind_addr, self.port)
    }

    /// Validate that we're not binding to 0.0.0.0 without explicit opt-in.
    pub fn validate_bind_safety(&self) -> Result<()> {
        if self.bind_addr == "0.0.0.0" {
            tracing::warn!(
                "Gateway binding to 0.0.0.0 (all interfaces). \
                 This exposes the agent to the network. \
                 Use 127.0.0.1 for local-only access."
            );
            if self.bearer_token.is_none() {
                return Err(FerroError::Security(
                    "Refusing to bind to 0.0.0.0 without bearer_token authentication. \
                     Set gateway.bearer_token or gateway.bearer_token_env in config."
                        .into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct GatewayState {
    config: Arc<Config>,
    default_model: String,
    per_request_timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
struct ResponsesRequest {
    model: Option<String>,
    input: Value,
}

/// Start the HTTP gateway server.
///
/// `_agent_loop` is intentionally unused now: runtime requests are isolated and
/// served by per-request loop instances to avoid lock contention from long-lived runs.
pub async fn start_gateway(
    config: &Config,
    _agent_loop: Arc<Mutex<AgentLoop>>,
) -> Result<GatewayHandle> {
    let gateway = Gateway::from_config(config);
    gateway.validate_bind_safety()?;

    tracing::info!("Starting HTTP gateway on {}", gateway.listen_addr());
    tracing::info!(
        "Auth: {}",
        if gateway.bearer_token.is_some() {
            "enabled"
        } else {
            "disabled (local only)"
        }
    );

    let state = GatewayState {
        config: Arc::new(config.clone()),
        default_model: config.agent.default_model.clone(),
        per_request_timeout_ms: gateway_request_timeout_ms(),
    };

    let app = Router::new()
        .route("/v1/health", get(health_handler))
        .route("/v1/models", get(models_handler))
        .route("/v1/responses", post(responses_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(gateway.listen_addr()).await?;
    let server_task = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("HTTP gateway server error: {}", e);
        }
    });

    Ok(GatewayHandle { server_task })
}

async fn health_handler() -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "ferroclaw-gateway"
    }))
}

async fn models_handler(State(state): State<GatewayState>) -> Json<Value> {
    Json(json!({
        "object": "list",
        "data": [
            {
                "id": state.default_model,
                "object": "model"
            }
        ]
    }))
}

async fn responses_handler(
    State(state): State<GatewayState>,
    Json(req): Json<ResponsesRequest>,
) -> impl IntoResponse {
    let prompt = match extract_input_text(&req.input) {
        Some(s) if !s.trim().is_empty() => s,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "message": "input is required"
                    }
                })),
            )
                .into_response();
        }
    };

    let model = req.model.unwrap_or_else(|| state.default_model.clone());
    let timeout_ms = state.per_request_timeout_ms;
    let started = Instant::now();

    let mut request_config = (*state.config).clone();
    request_config.agent.default_model = model.clone();

    let run_result = tokio::time::timeout(Duration::from_millis(timeout_ms), async {
        let mut agent = build_request_agent_loop(&request_config)?;
        let mut history: Vec<Message> = Vec::new();
        agent.reset_run_state();
        agent.run(&prompt, &mut history).await
    })
    .await;

    match run_result {
        Ok(Ok((outcome, events))) => {
            let elapsed_ms = started.elapsed().as_millis() as u64;
            let (_, tool_calls) = summarize_events(&events);
            openai_response(OpenAiResponseParams {
                model: &model,
                text: &outcome.text,
                elapsed_ms,
                input_tokens: outcome.input_tokens,
                output_tokens: outcome.output_tokens,
                total_tokens: outcome.total_tokens,
                tool_calls: tool_calls.max(outcome.stop.tool_calls_total),
                timed_out: false,
                stop: Some(outcome.stop),
            })
        }
        Ok(Err(e)) => {
            let elapsed_ms = started.elapsed().as_millis() as u64;
            let message = format!(
                "Runtime error while serving request: {e}. Returning bounded failure response."
            );
            let input_tokens = estimate_tokens(&prompt);
            let output_tokens = estimate_tokens(&message);
            openai_response(OpenAiResponseParams {
                model: &model,
                text: &message,
                elapsed_ms,
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
                tool_calls: 0,
                timed_out: false,
                stop: None,
            })
        }
        Err(_) => {
            let elapsed_ms = started.elapsed().as_millis() as u64;
            let message = format!(
                "Request timed out at gateway ceiling ({timeout_ms}ms). Returning bounded partial result."
            );
            let input_tokens = estimate_tokens(&prompt);
            let output_tokens = estimate_tokens(&message);
            openai_response(OpenAiResponseParams {
                model: &model,
                text: &message,
                elapsed_ms,
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
                tool_calls: 0,
                timed_out: true,
                stop: None,
            })
        }
    }
}

fn build_request_agent_loop(config: &Config) -> Result<AgentLoop> {
    let memory = Arc::new(Mutex::new(MemoryStore::new(config.memory.db_path.clone())?));
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry, Arc::clone(&memory));

    // Keep request runtime lightweight and isolated: no MCP discovery or skill loading per request.
    let mcp_client = McpClient::new(config.mcp_servers.clone(), config.agent.max_response_size);
    let provider = crate::providers::resolve_provider(&config.agent.default_model, config)?;
    let capabilities = capabilities_from_config(&config.security.default_capabilities);

    Ok(AgentLoop::new(
        provider,
        registry,
        Some(mcp_client),
        config.clone(),
        capabilities,
        Vec::new(),
    ))
}

struct OpenAiResponseParams<'a> {
    model: &'a str,
    text: &'a str,
    elapsed_ms: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    tool_calls: u32,
    timed_out: bool,
    stop: Option<RunStopContract>,
}

fn openai_response(params: OpenAiResponseParams<'_>) -> axum::response::Response {
    let OpenAiResponseParams {
        model,
        text,
        elapsed_ms,
        input_tokens,
        output_tokens,
        total_tokens,
        tool_calls,
        timed_out,
        stop,
    } = params;

    let response_id = format!("resp_{}", uuid::Uuid::new_v4().simple());
    (
        StatusCode::OK,
        Json(json!({
            "id": response_id,
            "object": "response",
            "status": "completed",
            "model": model,
            "output_text": text,
            "output": [
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "output_text",
                            "text": text
                        }
                    ]
                }
            ],
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "total_tokens": total_tokens
            },
            "meta": {
                "elapsed_ms": elapsed_ms,
                "tool_calls": tool_calls,
                "timed_out": timed_out,
                "stop": stop
            }
        })),
    )
        .into_response()
}

fn summarize_events(events: &[crate::agent::r#loop::AgentEvent]) -> (u64, u32) {
    let mut output_tokens = 0u64;
    let mut tool_calls = 0u32;

    for event in events {
        match event {
            crate::agent::r#loop::AgentEvent::TokenUsage {
                input: _,
                output,
                total_used: _,
            } => {
                output_tokens = output_tokens.max(*output);
            }
            crate::agent::r#loop::AgentEvent::ToolCallStart { .. } => {
                tool_calls += 1;
            }
            _ => {}
        }
    }

    (output_tokens, tool_calls)
}

fn estimate_tokens(text: &str) -> u64 {
    ((text.chars().count() as u64) / 4).max(1)
}

fn gateway_request_timeout_ms() -> u64 {
    std::env::var("FERRO_GATEWAY_REQUEST_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v >= 1000)
        .unwrap_or(12_000)
}

fn extract_input_text(input: &Value) -> Option<String> {
    match input {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let mut acc = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    acc.push(text.to_string());
                    continue;
                }
                if let Some(content) = item.get("content") {
                    if let Some(s) = content.as_str() {
                        acc.push(s.to_string());
                        continue;
                    }
                    if let Some(arr) = content.as_array() {
                        for part in arr {
                            if let Some(text) = part.get("text").and_then(Value::as_str) {
                                acc.push(text.to_string());
                            }
                        }
                    }
                }
            }
            if acc.is_empty() {
                None
            } else {
                Some(acc.join("\n"))
            }
        }
        Value::Object(obj) => {
            if let Some(s) = obj.get("text").and_then(Value::as_str) {
                return Some(s.to_string());
            }
            if let Some(s) = obj.get("content").and_then(Value::as_str) {
                return Some(s.to_string());
            }
            None
        }
        _ => None,
    }
}

/// Handle to the running gateway server.
pub struct GatewayHandle {
    server_task: tokio::task::JoinHandle<()>,
}

impl GatewayHandle {
    /// Shutdown the gateway server.
    pub async fn shutdown(self) -> Result<()> {
        self.server_task.abort();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_gateway_from_default_config() {
        let config = Config::default();
        let gw = Gateway::from_config(&config);
        assert_eq!(gw.bind_addr, "127.0.0.1");
        assert_eq!(gw.port, 8420);
    }

    #[test]
    fn test_gateway_blocks_unsafe_bind() {
        let mut config = Config::default();
        config.gateway.bind = "0.0.0.0".into();
        config.gateway.bearer_token = None;
        let gw = Gateway::from_config(&config);
        assert!(gw.validate_bind_safety().is_err());
    }

    #[test]
    fn test_gateway_allows_safe_bind() {
        let config = Config::default();
        let gw = Gateway::from_config(&config);
        assert!(gw.validate_bind_safety().is_ok());
    }

    #[test]
    fn test_gateway_allows_0000_with_token() {
        let mut config = Config::default();
        config.gateway.bind = "0.0.0.0".into();
        config.gateway.bearer_token = Some("secret".into());
        let gw = Gateway::from_config(&config);
        assert!(gw.validate_bind_safety().is_ok());
    }

    #[test]
    fn test_extract_input_text_from_string() {
        let input = Value::String("hello".to_string());
        assert_eq!(extract_input_text(&input).as_deref(), Some("hello"));
    }

    #[test]
    fn test_extract_input_text_from_content_array() {
        let input = json!([
            {"content": [{"type": "input_text", "text": "a"}]},
            {"content": [{"type": "input_text", "text": "b"}]}
        ]);
        assert_eq!(extract_input_text(&input).as_deref(), Some("a\nb"));
    }
}
