//! Core agent loop: ReAct (Reason + Act) cycle.
//!
//! 1. Assemble context (system prompt + diet summaries + conversation + memory)
//! 2. Call LLM with tool definitions
//! 3. Parse tool_use blocks
//! 4. Execute tools (with capability checks)
//! 5. Append results, loop until text response or budget exhausted
use crate::agent::context::ContextManager;
use crate::config::Config;
use crate::error::{FerroError, Result};
use crate::mcp::client::McpClient;
use crate::mcp::diet::SkillSummary;
use crate::mcp::registry::build_diet_context;
use crate::provider::LlmProvider;
use crate::tool::ToolRegistry;
use crate::types::{
    CapabilitySet, Message, Role, RunOutcome, RunStopContract, RunStopReason, ToolCall,
    ToolDefinition, ToolSource,
};
use crate::websocket::{WsBroadcaster, WsEvent};

const EXECUTION_POLICY_APPENDIX: &str = "Operational policy:\n- You have real tool access. When a user asks you to perform an action, execute it with tools instead of only describing steps.\n- Do not claim an action is impossible until you actually attempt the relevant tool call(s).\n- If a tool fails (network/TLS/auth/path), report the exact failure and then propose the smallest next workaround.\n- Prefer concrete outputs grounded in tool results over generic advice.\n- If the user asks for parity with Hermes-style behavior, act tool-first and completion-first.";

/// The core agent loop.
pub struct AgentLoop {
    provider: Box<dyn LlmProvider>,
    registry: ToolRegistry,
    mcp_client: Option<McpClient>,
    context: ContextManager,
    config: Config,
    capabilities: CapabilitySet,
    skill_summaries: Vec<SkillSummary>,
    /// Optional WebSocket broadcaster for real-time events.
    ws_broadcaster: Option<WsBroadcaster>,
    /// Agent ID for WebSocket events.
    agent_id: String,
}

/// Events emitted during the agent loop for streaming to the UI.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    TextDelta(String),
    /// LLM round started (1-based iteration within this user turn).
    LlmRound {
        iteration: u32,
    },
    /// Model chose to invoke these tools before execution.
    ModelToolChoice {
        iteration: u32,
        names: Vec<String>,
    },
    /// Multiple tools in one assistant message (Hermes-style batch).
    ParallelToolBatch {
        count: usize,
    },
    ToolCallStart {
        id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        id: String,
        name: String,
        content: String,
        is_error: bool,
    },
    Done {
        text: String,
    },
    Error(String),
    TokenUsage {
        input: u64,
        output: u64,
        total_used: u64,
    },
}

impl AgentLoop {
    pub fn new(
        provider: Box<dyn LlmProvider>,
        registry: ToolRegistry,
        mcp_client: Option<McpClient>,
        config: Config,
        capabilities: CapabilitySet,
        skill_summaries: Vec<SkillSummary>,
    ) -> Self {
        let context = ContextManager::new(config.agent.token_budget);
        Self {
            provider,
            registry,
            mcp_client,
            context,
            config,
            capabilities,
            skill_summaries,
            ws_broadcaster: None,
            agent_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Set WebSocket broadcaster for real-time event broadcasting.
    pub fn with_ws_broadcaster(mut self, broadcaster: WsBroadcaster) -> Self {
        self.ws_broadcaster = Some(broadcaster);
        self
    }

    /// Set a custom agent ID for WebSocket events.
    pub fn with_agent_id(mut self, id: String) -> Self {
        self.agent_id = id;
        self
    }

    /// Reset per-run budget accounting (used by stateless HTTP runtime requests).
    pub fn reset_run_state(&mut self) {
        self.context.tokens_used = 0;
    }

    /// Broadcast a WebSocket event if broadcaster is configured.
    fn broadcast_event(&self, event: WsEvent) {
        if let Some(broadcaster) = &self.ws_broadcaster
            && let Err(e) = broadcaster.broadcast(event)
        {
            tracing::warn!("Failed to broadcast WebSocket event: {}", e);
        }
    }

    /// Run the agent loop for a single user message.
    /// Returns final assistant outcome and all events.
    pub async fn run(
        &mut self,
        user_message: &str,
        history: &mut Vec<Message>,
    ) -> Result<(RunOutcome, Vec<AgentEvent>)> {
        let mut events = Vec::new();
        let outcome = self
            .run_with_callback(user_message, history, |e: &AgentEvent| {
                events.push(e.clone())
            })
            .await?;
        Ok((outcome, events))
    }

    /// Run the agent loop, invoking `on_event` for every [`AgentEvent`] as it occurs (streaming).
    pub async fn run_with_callback<F>(
        &mut self,
        user_message: &str,
        history: &mut Vec<Message>,
        mut on_event: F,
    ) -> Result<RunOutcome>
    where
        F: FnMut(&AgentEvent),
    {
        // Per-turn accounting: reset usage before each run so token budget checks and
        // UI status represent the current run, not cumulative prior turns.
        self.context.tokens_used = 0;

        let started = std::time::Instant::now();
        let mut tool_calls_total: u32 = 0;
        let mut tool_batches: u32 = 0;
        let mut last_input_tokens: u64 = 0;
        let mut last_output_tokens: u64 = 0;
        // Broadcast agent thinking state
        self.broadcast_event(WsEvent::agent_state(
            self.agent_id.clone(),
            crate::websocket::AgentState::Thinking,
        ));

        // Build system message with diet context
        let builtin_defs: Vec<ToolDefinition> = self
            .registry
            .all_meta()
            .iter()
            .filter(|m| matches!(m.source, ToolSource::Builtin))
            .map(|m| m.definition.clone())
            .collect();

        let diet_context = build_diet_context(&self.skill_summaries, &builtin_defs);
        let system_prompt = format!(
            "{}\n\n{}\n\n{}",
            self.config.agent.system_prompt, EXECUTION_POLICY_APPENDIX, diet_context
        );

        // Ensure system message is first
        if history.is_empty() || history[0].role != crate::types::Role::System {
            history.insert(0, Message::system(&system_prompt));
        } else {
            history[0] = Message::system(&system_prompt);
        }

        // Add user message
        history.push(Message::user(user_message));

        // Invariant I1: provider input must contain at least one user message.
        if !history.iter().any(|m| m.role == Role::User) {
            return Ok(make_run_outcome(RunOutcomeParams {
                text: "Stopped: invariant violated (missing user message).".to_string(),
                reason: RunStopReason::ErrorNonRetryable,
                iterations: 0,
                tool_calls_total,
                elapsed_ms: started.elapsed().as_millis() as u64,
                input_tokens: last_input_tokens,
                output_tokens: last_output_tokens,
                notes: Some("loop invariant I1 violated: no user message".to_string()),
            }));
        }

        // Get all tool definitions for provider, plus a built-in-only fallback set
        let all_tools = self.registry.definitions();
        let builtin_tools: Vec<ToolDefinition> = self
            .registry
            .all_meta()
            .iter()
            .filter(|m| matches!(m.source, ToolSource::Builtin))
            .map(|m| m.definition.clone())
            .collect();

        let mut iteration = 0;
        let max_iterations = self.config.agent.max_iterations;
        let max_tool_calls_per_iteration = self.config.agent.max_tool_calls_per_iteration;
        let max_tool_calls_total = self.config.agent.max_tool_calls_total;
        let wall_clock_budget_ms = effective_wall_clock_budget_ms(&self.config, user_message);

        loop {
            if let Some(outcome) = stop_for_wall_clock_budget(
                started,
                wall_clock_budget_ms,
                iteration,
                tool_calls_total,
                last_input_tokens,
                last_output_tokens,
            ) {
                self.broadcast_event(WsEvent::agent_state(
                    self.agent_id.clone(),
                    crate::websocket::AgentState::Error,
                ));
                return Ok(outcome);
            }

            iteration += 1;
            if iteration > max_iterations {
                // Broadcast error state
                self.broadcast_event(WsEvent::agent_state(
                    self.agent_id.clone(),
                    crate::websocket::AgentState::Error,
                ));
                return Ok(make_run_outcome(RunOutcomeParams {
                    text: "Stopped: max iterations reached.".to_string(),
                    reason: RunStopReason::BudgetIterations,
                    iterations: max_iterations,
                    tool_calls_total,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    input_tokens: last_input_tokens,
                    output_tokens: last_output_tokens,
                    notes: None,
                }));
            }

            // Check token budget
            if self.context.remaining() == 0 {
                // Broadcast error state
                self.broadcast_event(WsEvent::agent_state(
                    self.agent_id.clone(),
                    crate::websocket::AgentState::Error,
                ));
                return Ok(make_run_outcome(RunOutcomeParams {
                    text: "Stopped: token budget exhausted.".to_string(),
                    reason: RunStopReason::BudgetTokens,
                    iterations: iteration,
                    tool_calls_total,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    input_tokens: last_input_tokens,
                    output_tokens: last_output_tokens,
                    notes: Some(format!(
                        "used={} limit={}",
                        self.context.tokens_used, self.context.token_budget
                    )),
                }));
            }

            // Prune context if needed
            self.context.prune_to_fit(history);

            let round = AgentEvent::LlmRound { iteration };
            on_event(&round);

            // Call LLM with fallback chain.
            // If we hit provider context overflow, retry once with a reduced toolset
            // (built-ins only, then no tools) and a smaller completion cap.
            let max_tokens = effective_max_tokens(
                &self.config,
                &self.config.agent.default_model,
                started,
                wall_clock_budget_ms,
            );

            let response = match self
                .call_with_fallback(history, &all_tools, max_tokens, &mut on_event)
                .await
            {
                Ok(resp) => resp,
                Err(err) if is_context_overflow_error(&err) => {
                    let ev = AgentEvent::Error(
                        "Provider context overflow detected; retrying with compact toolset".into(),
                    );
                    on_event(&ev);

                    // Try built-in tools only first.
                    let reduced_max_tokens = max_tokens.min(1024);
                    match self
                        .call_with_fallback(
                            history,
                            &builtin_tools,
                            reduced_max_tokens,
                            &mut on_event,
                        )
                        .await
                    {
                        Ok(resp) => resp,
                        Err(err2) if is_context_overflow_error(&err2) => {
                            let ev = AgentEvent::Error(
                                "Context still too large; retrying with tools disabled".into(),
                            );
                            on_event(&ev);
                            match self
                                .call_with_fallback(history, &[], reduced_max_tokens, &mut on_event)
                                .await
                            {
                                Ok(resp) => resp,
                                Err(final_err) => {
                                    return Ok(outcome_from_error(
                                        final_err,
                                        iteration,
                                        tool_calls_total,
                                        started.elapsed().as_millis() as u64,
                                        last_input_tokens,
                                        last_output_tokens,
                                    ));
                                }
                            }
                        }
                        Err(other) => {
                            return Ok(outcome_from_error(
                                other,
                                iteration,
                                tool_calls_total,
                                started.elapsed().as_millis() as u64,
                                last_input_tokens,
                                last_output_tokens,
                            ));
                        }
                    }
                }
                Err(other) => {
                    return Ok(outcome_from_error(
                        other,
                        iteration,
                        tool_calls_total,
                        started.elapsed().as_millis() as u64,
                        last_input_tokens,
                        last_output_tokens,
                    ));
                }
            };

            // Track usage
            if let Some(usage) = &response.usage {
                self.context
                    .record_usage(usage.input_tokens, usage.output_tokens);
                last_input_tokens = usage.input_tokens;
                last_output_tokens = usage.output_tokens;
                let u = AgentEvent::TokenUsage {
                    input: usage.input_tokens,
                    output: usage.output_tokens,
                    total_used: self.context.tokens_used,
                };
                on_event(&u);
            }

            // Check for tool calls
            if let Some(outcome) = stop_for_wall_clock_budget(
                started,
                wall_clock_budget_ms,
                iteration,
                tool_calls_total,
                last_input_tokens,
                last_output_tokens,
            ) {
                self.broadcast_event(WsEvent::agent_state(
                    self.agent_id.clone(),
                    crate::websocket::AgentState::Error,
                ));
                return Ok(outcome);
            }

            let tool_calls = response.message.tool_calls.clone();
            history.push(response.message);

            if let Some(tool_calls) = tool_calls {
                if tool_calls.is_empty() {
                    // No tool calls — return text response
                    let text = history
                        .last()
                        .map(|m| m.text().to_string())
                        .unwrap_or_default();
                    let done = AgentEvent::Done { text: text.clone() };
                    on_event(&done);

                    // Broadcast agent idle state
                    self.broadcast_event(WsEvent::agent_state(
                        self.agent_id.clone(),
                        crate::websocket::AgentState::Idle,
                    ));

                    let (reason, final_text, note) = if text.trim().is_empty() && tool_batches > 0 {
                        (
                            RunStopReason::ErrorEmptyFinalAfterTools,
                            "Stopped: assistant returned empty final text after tool execution."
                                .to_string(),
                            Some(
                                "loop invariant I4 violated: empty final text after tool batch"
                                    .to_string(),
                            ),
                        )
                    } else {
                        (RunStopReason::AssistantFinal, text, None)
                    };

                    return Ok(make_run_outcome(RunOutcomeParams {
                        text: final_text,
                        reason,
                        iterations: iteration,
                        tool_calls_total,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        input_tokens: last_input_tokens,
                        output_tokens: last_output_tokens,
                        notes: note,
                    }));
                }

                if let Some((reason, note)) = check_tool_call_caps(
                    tool_calls.len(),
                    tool_calls_total,
                    max_tool_calls_per_iteration,
                    max_tool_calls_total,
                ) {
                    self.broadcast_event(WsEvent::agent_state(
                        self.agent_id.clone(),
                        crate::websocket::AgentState::Error,
                    ));
                    return Ok(make_run_outcome(RunOutcomeParams {
                        text: "Stopped: tool call budget exceeded.".to_string(),
                        reason,
                        iterations: iteration,
                        tool_calls_total,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        input_tokens: last_input_tokens,
                        output_tokens: last_output_tokens,
                        notes: Some(note),
                    }));
                }

                let names: Vec<String> = tool_calls.iter().map(|tc| tc.name.clone()).collect();
                let choice = AgentEvent::ModelToolChoice { iteration, names };
                on_event(&choice);

                if tool_calls.len() > 1 {
                    let batch = AgentEvent::ParallelToolBatch {
                        count: tool_calls.len(),
                    };
                    on_event(&batch);
                }

                tool_batches = tool_batches.saturating_add(1);
                let expected_results = tool_calls.len();
                let mut emitted_results = 0usize;

                // Execute each tool call
                for tc in &tool_calls {
                    let args_str =
                        serde_json::to_string(&tc.arguments).unwrap_or_else(|_| "{}".into());
                    let start = AgentEvent::ToolCallStart {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: args_str,
                    };
                    on_event(&start);

                    // Broadcast tool start event
                    self.broadcast_event(WsEvent::tool_start(
                        tc.id.clone(),
                        tc.name.clone(),
                        tc.arguments.clone(),
                    ));

                    // Broadcast agent executing state
                    self.broadcast_event(WsEvent::agent_state(
                        self.agent_id.clone(),
                        crate::websocket::AgentState::Executing,
                    ));

                    let result = self.execute_tool_call(tc).await;

                    let (content, is_error) = match result {
                        Ok(tr) => (tr.content, tr.is_error),
                        Err(e) => (format!("Error: {e}"), true),
                    };

                    // Broadcast tool result as output chunk
                    self.broadcast_event(WsEvent::tool_chunk(tc.id.clone(), content.clone(), true));

                    // Broadcast tool completion state
                    self.broadcast_event(WsEvent::tool_update(
                        tc.id.clone(),
                        if is_error {
                            crate::websocket::ToolState::Failed
                        } else {
                            crate::websocket::ToolState::Completed
                        },
                    ));

                    let tres = AgentEvent::ToolResult {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        content: content.clone(),
                        is_error,
                    };
                    on_event(&tres);
                    emitted_results = emitted_results.saturating_add(1);
                    tool_calls_total = tool_calls_total.saturating_add(1);

                    history.push(Message::tool_result(&tc.id, &content));
                }

                // Invariant I2: every emitted tool call must produce a result/error event.
                if emitted_results != expected_results {
                    return Ok(make_run_outcome(RunOutcomeParams {
                        text: "Stopped: tool lifecycle invariant violation.".to_string(),
                        reason: RunStopReason::ErrorNonRetryable,
                        iterations: iteration,
                        tool_calls_total,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        input_tokens: last_input_tokens,
                        output_tokens: last_output_tokens,
                        notes: Some(format!(
                            "loop invariant I2 violated: expected {} results, got {}",
                            expected_results, emitted_results
                        )),
                    }));
                }
            } else {
                // No tool calls — text response
                let text = history
                    .last()
                    .map(|m| m.text().to_string())
                    .unwrap_or_default();
                let done = AgentEvent::Done { text: text.clone() };
                on_event(&done);

                // Broadcast agent idle state
                self.broadcast_event(WsEvent::agent_state(
                    self.agent_id.clone(),
                    crate::websocket::AgentState::Idle,
                ));

                let (reason, final_text, note) = if text.trim().is_empty() && tool_batches > 0 {
                    (
                        RunStopReason::ErrorEmptyFinalAfterTools,
                        "Stopped: assistant returned empty final text after tool execution."
                            .to_string(),
                        Some(
                            "loop invariant I4 violated: empty final text after tool batch"
                                .to_string(),
                        ),
                    )
                } else {
                    (RunStopReason::AssistantFinal, text, None)
                };

                return Ok(make_run_outcome(RunOutcomeParams {
                    text: final_text,
                    reason,
                    iterations: iteration,
                    tool_calls_total,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    input_tokens: last_input_tokens,
                    output_tokens: last_output_tokens,
                    notes: note,
                }));
            }
        }
    }

    /// Try primary model, then each fallback in order.
    /// Returns first successful response.
    async fn call_with_fallback<F>(
        &self,
        history: &[Message],
        tools: &[ToolDefinition],
        max_tokens: u32,
        on_event: &mut F,
    ) -> Result<crate::types::ProviderResponse>
    where
        F: FnMut(&AgentEvent),
    {
        let primary = &self.config.agent.default_model;
        let fallbacks = &self.config.agent.fallback_models;

        // Try primary model
        match self
            .provider
            .complete(history, tools, primary, max_tokens)
            .await
        {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                if fallbacks.is_empty() {
                    return Err(e);
                }
                tracing::warn!("Primary model '{}' failed: {}", primary, e);
                let ev =
                    AgentEvent::Error(format!("Model '{}' failed, trying fallbacks...", primary));
                on_event(&ev);
            }
        }

        // Try each fallback
        for (i, fallback_model) in fallbacks.iter().enumerate() {
            tracing::info!(
                "Trying fallback model {}/{}: {}",
                i + 1,
                fallbacks.len(),
                fallback_model
            );
            match self
                .provider
                .complete(history, tools, fallback_model, max_tokens)
                .await
            {
                Ok(resp) => {
                    tracing::info!("Fallback model '{}' succeeded", fallback_model);
                    let ev = AgentEvent::Error(format!("Using fallback model: {}", fallback_model));
                    on_event(&ev);
                    return Ok(resp);
                }
                Err(e) => {
                    tracing::warn!("Fallback model '{}' failed: {}", fallback_model, e);
                }
            }
        }

        Err(FerroError::Provider(format!(
            "All models failed. Tried: {}, {}",
            primary,
            fallbacks.join(", ")
        )))
    }

    async fn execute_tool_call(&self, tc: &ToolCall) -> Result<crate::types::ToolResult> {
        // Check if this is an MCP tool
        if let Some(meta) = self.registry.get_meta(&tc.name)
            && let ToolSource::Mcp { server } = &meta.source
            && let Some(mcp) = &self.mcp_client
        {
            // Route through MCP client
            let diet_response = mcp.execute_tool(server, &tc.name, &tc.arguments).await?;
            return Ok(crate::types::ToolResult {
                call_id: tc.id.clone(),
                content: diet_response.content,
                is_error: false,
            });
        }

        // Execute through registry (built-in tools)
        self.registry
            .execute(&tc.name, &tc.id, &tc.arguments, &self.capabilities)
            .await
    }

    // Helper methods for orchestration
    /// Get a reference to tool registry
    pub fn get_tool_registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Get current token budget
    pub fn get_token_budget(&self) -> u64 {
        self.context.token_budget
    }
}

fn check_tool_call_caps(
    requested_in_iteration: usize,
    tool_calls_total_so_far: u32,
    max_tool_calls_per_iteration: u32,
    max_tool_calls_total: u32,
) -> Option<(RunStopReason, String)> {
    let requested = requested_in_iteration as u32;

    if max_tool_calls_per_iteration > 0 && requested > max_tool_calls_per_iteration {
        return Some((
            RunStopReason::BudgetToolsIteration,
            format!(
                "iteration requested={} limit={}",
                requested, max_tool_calls_per_iteration
            ),
        ));
    }

    if max_tool_calls_total > 0 {
        let would_total = tool_calls_total_so_far.saturating_add(requested);
        if would_total > max_tool_calls_total {
            return Some((
                RunStopReason::BudgetToolsTotal,
                format!(
                    "total_so_far={} requested={} limit={}",
                    tool_calls_total_so_far, requested, max_tool_calls_total
                ),
            ));
        }
    }

    None
}

struct RunOutcomeParams {
    text: String,
    reason: RunStopReason,
    iterations: u32,
    tool_calls_total: u32,
    elapsed_ms: u64,
    input_tokens: u64,
    output_tokens: u64,
    notes: Option<String>,
}

fn make_run_outcome(params: RunOutcomeParams) -> RunOutcome {
    let RunOutcomeParams {
        text,
        reason,
        iterations,
        tool_calls_total,
        elapsed_ms,
        input_tokens,
        output_tokens,
        notes,
    } = params;

    RunOutcome {
        text,
        stop: RunStopContract {
            reason,
            iterations,
            tool_calls_total,
            elapsed_ms,
            notes,
        },
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        tool_calls: tool_calls_total,
    }
}

fn classify_stop_reason_from_error(err: &FerroError) -> RunStopReason {
    match err {
        FerroError::MaxIterations(_) => RunStopReason::BudgetIterations,
        FerroError::BudgetExhausted { .. } => RunStopReason::BudgetTokens,
        FerroError::Provider(msg) => {
            let m = msg.to_ascii_lowercase();
            if (m.contains("retry") && m.contains("exhaust"))
                || m.contains("max attempts")
                || m.contains("too many retries")
            {
                RunStopReason::ErrorRetryExhausted
            } else {
                RunStopReason::ErrorNonRetryable
            }
        }
        _ => RunStopReason::ErrorNonRetryable,
    }
}

fn outcome_from_error(
    err: FerroError,
    iteration: u32,
    tool_calls_total: u32,
    elapsed_ms: u64,
    input_tokens: u64,
    output_tokens: u64,
) -> RunOutcome {
    let reason = classify_stop_reason_from_error(&err);
    make_run_outcome(RunOutcomeParams {
        text: format!("Stopped: {err}"),
        reason,
        iterations: iteration,
        tool_calls_total,
        elapsed_ms,
        input_tokens,
        output_tokens,
        notes: Some(err.to_string()),
    })
}

fn resolve_provider_max_tokens(config: &Config, model: &str) -> u32 {
    let model_l = model.to_ascii_lowercase();
    let bare_codex_model = !model_l.contains('/')
        && (model_l.starts_with("codex-")
            || model_l.contains("-codex")
            || model_l.starts_with("gpt-5."));

    if model_l.starts_with("openaicodex:") || model_l.starts_with("codex-") || bare_codex_model {
        return config
            .providers
            .openai_codex
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("google:") || model_l.starts_with("gemini-") {
        return config
            .providers
            .google
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("xai:") || model_l.starts_with("grok-") {
        return config
            .providers
            .xai
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("nvidia:")
        || model_l.starts_with("z-ai/")
        || model_l.starts_with("nvidia/")
    {
        return config
            .providers
            .nvidia
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("llamacpp:") {
        return config
            .providers
            .llamacpp
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("mistral:") || model_l.starts_with("mistral-") {
        return config
            .providers
            .mistral
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("azure:") || model_l.starts_with("azure-openai:") {
        return config
            .providers
            .azure_openai
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("copilot:") || model_l.starts_with("githubcopilot:") {
        return config
            .providers
            .github_copilot
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("vertex:") || model_l.starts_with("googlevertex:") {
        return config
            .providers
            .google_vertex
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }
    if model_l.starts_with("bedrock:") {
        return config
            .providers
            .bedrock
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }

    if model.contains('/') {
        return config
            .providers
            .openrouter
            .as_ref()
            .map(|o| o.max_tokens)
            .unwrap_or(8192);
    }

    if model.starts_with("claude-") {
        return config
            .providers
            .anthropic
            .as_ref()
            .map(|a| a.max_tokens)
            .unwrap_or(8192);
    }

    if model.starts_with("glm-") {
        return config
            .providers
            .zai
            .as_ref()
            .map(|z| z.max_tokens)
            .unwrap_or(8192);
    }

    if let Some(o) = config.providers.openai.as_ref() {
        return o.max_tokens;
    }
    if let Some(o) = config.providers.openai_codex.as_ref() {
        return o.max_tokens;
    }
    8192
}

fn effective_wall_clock_budget_ms(config: &Config, user_message: &str) -> Option<u64> {
    let configured = if config.agent.max_wall_clock_ms > 0 {
        Some(config.agent.max_wall_clock_ms)
    } else {
        None
    };

    let strict_detected = if config.agent.deadline_aware_completion {
        parse_prompt_wall_clock_budget_ms(user_message)
    } else {
        None
    };

    match (configured, strict_detected) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn parse_prompt_wall_clock_budget_ms(user_message: &str) -> Option<u64> {
    let lower = user_message.to_ascii_lowercase();
    if let Some(idx) = lower.find("max_ms=") {
        let digits: String = lower[idx + 7..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(v) = digits.parse::<u64>()
            && v > 0
        {
            return Some(v);
        }
    }

    if lower.contains("strict wall-clock") || lower.contains("strict wall clock") {
        return Some(5_000);
    }

    None
}

fn effective_max_tokens(
    config: &Config,
    model: &str,
    started: std::time::Instant,
    wall_clock_budget_ms: Option<u64>,
) -> u32 {
    let base = resolve_provider_max_tokens(config, model);
    if !config.agent.deadline_aware_completion {
        return base;
    }

    let Some(deadline_ms) = wall_clock_budget_ms else {
        return base;
    };

    let elapsed_ms = started.elapsed().as_millis() as u64;
    if elapsed_ms >= deadline_ms {
        return config.agent.deadline_tight_max_tokens.min(base);
    }

    let remaining_ms = deadline_ms.saturating_sub(elapsed_ms);
    if remaining_ms <= config.agent.deadline_tight_ms {
        return config.agent.deadline_tight_max_tokens.min(base);
    }

    base
}

fn stop_for_wall_clock_budget(
    started: std::time::Instant,
    wall_clock_budget_ms: Option<u64>,
    iteration: u32,
    tool_calls_total: u32,
    input_tokens: u64,
    output_tokens: u64,
) -> Option<RunOutcome> {
    let limit = wall_clock_budget_ms?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    if elapsed_ms < limit {
        return None;
    }

    Some(make_run_outcome(RunOutcomeParams {
        text: "Stopped: wall-clock budget exhausted.".to_string(),
        reason: RunStopReason::BudgetWallClock,
        iterations: iteration,
        tool_calls_total,
        elapsed_ms,
        input_tokens,
        output_tokens,
        notes: Some(format!("elapsed_ms={} limit_ms={}", elapsed_ms, limit)),
    }))
}

fn is_context_overflow_error(err: &FerroError) -> bool {
    let FerroError::Provider(msg) = err else {
        return false;
    };

    let m = msg.to_ascii_lowercase();
    (m.contains("context") && (m.contains("length") || m.contains("window") || m.contains("token")))
        || m.contains("maximum context")
        || m.contains("requested") && m.contains("max")
        || m.contains("too long")
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AnthropicConfig, OpenAiConfig, OpenRouterConfig, ProvidersConfig};

    #[test]
    fn test_agent_event_variants() {
        let event = AgentEvent::TextDelta("hello".into());
        match event {
            AgentEvent::TextDelta(t) => assert_eq!(t, "hello"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_detects_context_overflow_error_strings() {
        let e1 = FerroError::Provider(
            "OpenRouter API error (400): This model's maximum context length is 400000 tokens. However, your messages resulted in 678640 tokens".into(),
        );
        assert!(is_context_overflow_error(&e1));

        let e2 = FerroError::Provider("timeout after 240000ms".into());
        assert!(!is_context_overflow_error(&e2));
    }

    #[test]
    fn test_resolve_provider_max_tokens_for_openrouter_model() {
        let cfg = Config {
            providers: ProvidersConfig {
                anthropic: Some(AnthropicConfig {
                    api_key_env: "ANTHROPIC_API_KEY".into(),
                    base_url: "https://api.anthropic.com".into(),
                    max_tokens: 1111,
                    request_timeout_ms: 15_000,
                    max_retries: 2,
                    no_retry_max_tokens_threshold: 128,
                }),
                openai: Some(OpenAiConfig {
                    api_key_env: "OPENAI_API_KEY".into(),
                    base_url: "https://api.openai.com/v1".into(),
                    auth_mode: "api_key".into(),
                    oauth_token_env: "OPENAI_OAUTH_TOKEN".into(),
                    max_tokens: 2222,
                    request_timeout_ms: 15_000,
                    max_retries: 2,
                    no_retry_max_tokens_threshold: 128,
                }),
                openrouter: Some(OpenRouterConfig {
                    api_key_env: "OPENROUTER_API_KEY".into(),
                    base_url: "https://openrouter.ai/api/v1".into(),
                    site_url: None,
                    site_name: None,
                    max_tokens: 3333,
                    request_timeout_ms: 15_000,
                    max_retries: 2,
                    no_retry_max_tokens_threshold: 128,
                }),
                ..ProvidersConfig::default()
            },
            ..Config::default()
        };

        assert_eq!(
            resolve_provider_max_tokens(&cfg, "openai/gpt-5.3-codex"),
            3333
        );
        assert_eq!(
            resolve_provider_max_tokens(&cfg, "claude-sonnet-4-20250514"),
            1111
        );
        assert_eq!(resolve_provider_max_tokens(&cfg, "gpt-4.1"), 2222);
    }

    #[test]
    fn test_classifies_retry_exhausted_provider_errors() {
        let e = FerroError::Provider("too many retries; retry exhausted after max attempts".into());
        assert_eq!(
            classify_stop_reason_from_error(&e),
            RunStopReason::ErrorRetryExhausted
        );

        let e2 = FerroError::Provider("invalid schema for tool input".into());
        assert_eq!(
            classify_stop_reason_from_error(&e2),
            RunStopReason::ErrorNonRetryable
        );
    }

    #[test]
    fn test_make_run_outcome_preserves_stop_contract_fields() {
        let out = make_run_outcome(RunOutcomeParams {
            text: "final".to_string(),
            reason: RunStopReason::AssistantFinal,
            iterations: 3,
            tool_calls_total: 5,
            elapsed_ms: 1200,
            input_tokens: 100,
            output_tokens: 40,
            notes: Some("ok".to_string()),
        });

        assert_eq!(out.text, "final");
        assert_eq!(out.stop.reason, RunStopReason::AssistantFinal);
        assert_eq!(out.stop.iterations, 3);
        assert_eq!(out.stop.tool_calls_total, 5);
        assert_eq!(out.stop.elapsed_ms, 1200);
        assert_eq!(out.input_tokens, 100);
        assert_eq!(out.output_tokens, 40);
        assert_eq!(out.total_tokens, 140);
        assert_eq!(out.tool_calls, 5);
    }

    #[test]
    fn test_tool_cap_checks_emit_expected_reasons() {
        let iteration_cap =
            check_tool_call_caps(9, 0, 8, 64).expect("should stop on per-iteration cap");
        assert_eq!(iteration_cap.0, RunStopReason::BudgetToolsIteration);

        let total_cap = check_tool_call_caps(3, 63, 8, 64).expect("should stop on total cap");
        assert_eq!(total_cap.0, RunStopReason::BudgetToolsTotal);

        assert!(check_tool_call_caps(2, 10, 8, 64).is_none());
    }

    #[derive(serde::Deserialize)]
    struct ConformanceCase {
        id: String,
        kind: String,
        expect_stop_reason: RunStopReason,
        requested_in_iteration: Option<usize>,
        tool_calls_total_so_far: Option<u32>,
        max_tool_calls_per_iteration: Option<u32>,
        max_tool_calls_total: Option<u32>,
        provider_error: Option<String>,
    }

    #[test]
    fn test_conformance_cases_json_runner_assertions() {
        let raw = include_str!("../../evals/tasks.conformance.json");
        let cases: Vec<ConformanceCase> =
            serde_json::from_str(raw).expect("valid conformance JSON");
        assert!(!cases.is_empty(), "conformance cases must not be empty");

        for case in cases {
            match case.kind.as_str() {
                "tool_cap" => {
                    let result = check_tool_call_caps(
                        case.requested_in_iteration
                            .expect("requested_in_iteration missing"),
                        case.tool_calls_total_so_far
                            .expect("tool_calls_total_so_far missing"),
                        case.max_tool_calls_per_iteration
                            .expect("max_tool_calls_per_iteration missing"),
                        case.max_tool_calls_total
                            .expect("max_tool_calls_total missing"),
                    )
                    .unwrap_or_else(|| panic!("{} expected stop, got none", case.id));
                    assert_eq!(result.0, case.expect_stop_reason, "case {}", case.id);
                }
                "error_classifier" => {
                    let err = FerroError::Provider(
                        case.provider_error
                            .clone()
                            .expect("provider_error missing for error_classifier case"),
                    );
                    let reason = classify_stop_reason_from_error(&err);
                    assert_eq!(reason, case.expect_stop_reason, "case {}", case.id);
                }
                other => panic!("unknown conformance case kind: {}", other),
            }
        }
    }
}
