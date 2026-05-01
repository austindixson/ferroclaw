//! Agent orchestration: subagent spawning, coordination, and communication
//!
//! This module provides infrastructure for running multiple agents in parallel
//! or hierarchically, enabling advanced multi-agent patterns like:
//! - Parent-child task delegation
//! - Agent-to-agent messaging
//! - Result aggregation
//! - Collaboration patterns

use crate::agent::context::ContextManager;
use crate::agent::r#loop::AgentLoop;
use crate::error::{FerroError, Result};
use std::collections::{HashMap, VecDeque};

/// Configuration for a subagent
#[derive(Debug, Clone)]
pub struct SubagentConfig {
    /// Unique identifier for this agent
    pub agent_id: String,
    /// Type of agent (planner, coder, reviewer, etc.)
    pub agent_type: String,
    /// Custom system prompt (overrides agent_type default)
    pub system_prompt: Option<String>,
    /// Tools this agent can access (empty = all available tools)
    pub allowed_tools: Vec<String>,
    /// Whether this agent has isolated memory
    pub memory_isolation: bool,
    /// Token budget for this agent (0 = use parent's budget)
    pub token_budget: u64,
    /// Maximum iterations for this agent
    pub max_iterations: Option<u32>,
}

impl SubagentConfig {
    /// Create a new subagent configuration
    pub fn new(agent_id: String, agent_type: String) -> Self {
        Self {
            agent_id,
            agent_type,
            system_prompt: None,
            allowed_tools: Vec::new(),
            memory_isolation: true,
            token_budget: 0,
            max_iterations: None,
        }
    }

    /// Set a custom system prompt
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set allowed tools (empty list = all tools)
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Enable or disable memory isolation
    pub fn with_memory_isolation(mut self, isolated: bool) -> Self {
        self.memory_isolation = isolated;
        self
    }

    /// Set token budget (0 = use parent's budget)
    pub fn with_token_budget(mut self, budget: u64) -> Self {
        self.token_budget = budget;
        self
    }

    /// Set max iterations
    pub fn with_max_iterations(mut self, iterations: u32) -> Self {
        self.max_iterations = Some(iterations);
        self
    }
}

/// Message between agents
#[derive(Debug, Clone)]
pub struct AgentMessage {
    /// ID of sending agent
    pub from_agent_id: String,
    /// ID of receiving agent (empty = broadcast)
    pub to_agent_id: String,
    /// Message content
    pub content: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl AgentMessage {
    /// Create a new agent message
    pub fn new(from: impl Into<String>, to: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            from_agent_id: from.into(),
            to_agent_id: to.into(),
            content: content.into(),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Message bus for agent-to-agent communication
pub struct AgentMessageBus {
    /// Message queues per agent
    queues: HashMap<String, VecDeque<AgentMessage>>,
    /// Broadcast messages
    broadcast_queue: VecDeque<AgentMessage>,
}

impl AgentMessageBus {
    /// Create a new message bus
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            broadcast_queue: VecDeque::new(),
        }
    }

    /// Register an agent with the message bus
    pub fn register(&mut self, agent_id: String) {
        self.queues.entry(agent_id).or_default();
    }

    /// Send a message to a specific agent
    pub fn send(&mut self, msg: AgentMessage) -> Result<()> {
        if msg.to_agent_id.is_empty() {
            // Broadcast message
            self.broadcast_queue.push_back(msg);
        } else if self.queues.contains_key(&msg.to_agent_id) {
            self.queues
                .get_mut(&msg.to_agent_id)
                .unwrap()
                .push_back(msg);
        } else {
            return Err(FerroError::Tool(format!(
                "Cannot send message to unregistered agent: {}",
                msg.to_agent_id
            )));
        }
        Ok(())
    }

    /// Receive ONE message for a specific agent
    /// Returns direct message if available, otherwise one broadcast message (excluding own broadcasts)
    pub fn receive(&mut self, agent_id: &str) -> Vec<AgentMessage> {
        let mut messages = Vec::new();

        // First try to get a direct message for this agent
        if let Some(queue) = self.queues.get_mut(agent_id)
            && let Some(msg) = queue.pop_front()
        {
            messages.push(msg);
            return messages; // Return direct message immediately
        }

        // If no direct message, try to get a broadcast message
        // Don't receive own broadcasts
        let mut i = 0;
        while i < self.broadcast_queue.len() {
            let msg = &self.broadcast_queue[i];
            if msg.from_agent_id != agent_id {
                messages.push(self.broadcast_queue.remove(i).unwrap());
                return messages;
            }
            i += 1;
        }

        messages
    }

    /// Check if an agent has pending messages
    pub fn has_messages(&self, agent_id: &str) -> bool {
        self.queues.get(agent_id).is_some_and(|q| !q.is_empty())
            || self
                .broadcast_queue
                .iter()
                .any(|msg| msg.from_agent_id != agent_id)
    }

    /// Get count of pending messages for an agent
    /// Counts direct messages + broadcast messages not from this agent
    pub fn message_count(&self, agent_id: &str) -> usize {
        let direct = self.queues.get(agent_id).map_or(0, |q| q.len());
        let broadcast = self
            .broadcast_queue
            .iter()
            .filter(|msg| msg.from_agent_id != agent_id)
            .count();
        direct + broadcast
    }
}

impl Default for AgentMessageBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Orchestrator for managing multiple agents
///
/// The orchestrator handles:
/// - Spawning subagents with shared resources
/// - Coordinating agent execution
/// - Message passing between agents
/// - Result aggregation
pub struct Orchestrator {
    /// Parent agent loop
    parent: AgentLoop,
    /// Child agent loops
    children: HashMap<String, AgentLoop>,
    /// Message bus for agent communication
    message_bus: AgentMessageBus,
    /// Configuration tracking
    configs: HashMap<String, SubagentConfig>,
}

impl Orchestrator {
    /// Create a new orchestrator from a parent agent
    pub fn new(parent: AgentLoop) -> Self {
        Self {
            parent,
            children: HashMap::new(),
            message_bus: AgentMessageBus::new(),
            configs: HashMap::new(),
        }
    }

    /// Spawn a new child agent
    ///
    /// This creates a new AgentLoop instance with its own context but shared
    /// provider and tool registry (optionally filtered).
    pub fn spawn_child(&mut self, config: SubagentConfig) -> Result<()> {
        // Register agent with message bus
        self.message_bus.register(config.agent_id.clone());
        self.configs.insert(config.agent_id.clone(), config.clone());

        // Get reference to parent's tool registry
        let _registry = self.parent.get_tool_registry();

        // Create context manager
        let token_budget = if config.token_budget == 0 {
            self.parent.get_token_budget()
        } else {
            config.token_budget
        };

        let _context = ContextManager::new(token_budget);

        // In a full implementation, we would clone provider and create a new AgentLoop
        // For now, this is a placeholder showing structure
        tracing::info!(
            "Spawning child agent: {} (type: {})",
            config.agent_id,
            config.agent_type
        );

        // TODO: Actually create AgentLoop instance
        // This requires restructuring AgentLoop to support cloning of shared resources

        Ok(())
    }

    /// Execute a task on a child agent
    pub async fn execute_child(&mut self, agent_id: &str, _task: &str) -> Result<AgentExecution> {
        if !self.children.contains_key(agent_id) {
            return Err(FerroError::Tool(format!(
                "Child agent not found: {}",
                agent_id
            )));
        }

        // Get pending messages for this agent
        let messages = self.message_bus.receive(agent_id);
        if !messages.is_empty() {
            tracing::debug!("Agent {} received {} messages", agent_id, messages.len());
        }

        // Execute task
        // TODO: Actually call AgentLoop::run() on the child agent
        // This requires child AgentLoop to be properly initialized

        let response = format!("Task executed on agent {}", agent_id);

        Ok(AgentExecution {
            agent_id: agent_id.to_string(),
            response,
            tool_calls: 0,
            tokens_used: 0,
            messages_received: messages.len(),
            messages_sent: 0,
        })
    }

    /// Send a message from one agent to another
    pub fn send_message(&mut self, msg: AgentMessage) -> Result<()> {
        self.message_bus.send(msg)
    }

    /// Collect results from all child agents
    pub fn collect_results(&self) -> Vec<AgentExecution> {
        // In a full implementation, this would query each child agent's state
        Vec::new()
    }

    /// Get the parent agent
    pub fn parent(&self) -> &AgentLoop {
        &self.parent
    }

    /// Get a reference to the message bus
    pub fn message_bus(&self) -> &AgentMessageBus {
        &self.message_bus
    }

    /// Get a mutable reference to the message bus
    pub fn message_bus_mut(&mut self) -> &mut AgentMessageBus {
        &mut self.message_bus
    }

    /// Check if an agent exists
    pub fn has_agent(&self, agent_id: &str) -> bool {
        self.children.contains_key(agent_id)
    }

    /// Get configuration for an agent
    pub fn get_config(&self, agent_id: &str) -> Option<&SubagentConfig> {
        self.configs.get(agent_id)
    }
}

/// Result from executing an agent
#[derive(Debug, Clone)]
pub struct AgentExecution {
    /// Agent ID
    pub agent_id: String,
    /// Final response text
    pub response: String,
    /// Number of tool calls made
    pub tool_calls: usize,
    /// Tokens used
    pub tokens_used: u64,
    /// Number of messages received during execution
    pub messages_received: usize,
    /// Number of messages sent during execution
    pub messages_sent: usize,
}

impl AgentExecution {
    /// Create a new execution result
    pub fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            response: String::new(),
            tool_calls: 0,
            tokens_used: 0,
            messages_received: 0,
            messages_sent: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_config_builder() {
        let config = SubagentConfig::new("agent_1".to_string(), "coder".to_string())
            .with_prompt("Custom prompt")
            .with_tools(vec!["read_file".to_string(), "write_file".to_string()])
            .with_memory_isolation(false)
            .with_token_budget(10000)
            .with_max_iterations(50);

        assert_eq!(config.agent_id, "agent_1");
        assert_eq!(config.agent_type, "coder");
        assert_eq!(config.system_prompt, Some("Custom prompt".to_string()));
        assert_eq!(config.allowed_tools.len(), 2);
        assert!(!config.memory_isolation);
        assert_eq!(config.token_budget, 10000);
        assert_eq!(config.max_iterations, Some(50));
    }

    #[test]
    fn test_agent_message() {
        let msg = AgentMessage::new("agent_1", "agent_2", "Hello");
        assert_eq!(msg.from_agent_id, "agent_1");
        assert_eq!(msg.to_agent_id, "agent_2");
        assert_eq!(msg.content, "Hello");
        assert!(msg.timestamp <= chrono::Utc::now());
    }

    #[test]
    fn test_message_bus_registration() {
        let mut bus = AgentMessageBus::new();
        bus.register("agent_1".to_string());
        bus.register("agent_2".to_string());

        assert!(bus.queues.contains_key("agent_1"));
        assert!(bus.queues.contains_key("agent_2"));
    }

    #[test]
    fn test_message_bus_send_receive() {
        let mut bus = AgentMessageBus::new();
        bus.register("agent_1".to_string());
        bus.register("agent_2".to_string());

        let msg = AgentMessage::new("agent_1", "agent_2", "Test message");
        bus.send(msg).unwrap();

        let messages = bus.receive("agent_2");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Test message");

        // Messages should be removed after receiving
        assert_eq!(bus.receive("agent_2").len(), 0);
    }

    #[test]
    fn test_message_bus_broadcast() {
        let mut bus = AgentMessageBus::new();
        bus.register("agent_1".to_string());
        bus.register("agent_2".to_string());

        let msg = AgentMessage::new("agent_1", "", "Broadcast message");
        bus.send(msg).unwrap();

        // Both agents should receive broadcast
        let messages_1 = bus.receive("agent_1");
        let messages_2 = bus.receive("agent_2");

        // agent_1 should NOT receive its own broadcast
        assert_eq!(messages_1.len(), 0);
        // agent_2 should receive broadcast
        assert_eq!(messages_2.len(), 1);
        assert_eq!(messages_2[0].content, "Broadcast message");
    }

    #[test]
    fn test_message_bus_error_on_unregistered() {
        let mut bus = AgentMessageBus::new();
        bus.register("agent_1".to_string());

        let msg = AgentMessage::new("agent_1", "agent_2", "Test");
        let result = bus.send(msg);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unregistered agent")
        );
    }

    #[test]
    fn test_message_bus_has_messages() {
        let mut bus = AgentMessageBus::new();
        bus.register("agent_1".to_string());
        bus.register("agent_2".to_string());

        assert!(!bus.has_messages("agent_1"));
        assert!(!bus.has_messages("agent_2"));

        // Send a message
        let msg = AgentMessage::new("agent_2", "agent_1", "Hello");
        bus.send(msg).unwrap();

        assert!(bus.has_messages("agent_1"));
        assert!(!bus.has_messages("agent_2"));

        // Consume of message
        bus.receive("agent_1");

        assert!(!bus.has_messages("agent_1"));
    }

    #[test]
    fn test_message_bus_message_count() {
        let mut bus = AgentMessageBus::new();
        bus.register("agent_1".to_string());
        bus.register("agent_2".to_string());

        assert_eq!(bus.message_count("agent_1"), 0);

        let msg1 = AgentMessage::new("agent_2", "agent_1", "Message 1");
        let msg2 = AgentMessage::new("agent_2", "agent_1", "Message 2");
        let msg3 = AgentMessage::new("agent_2", "agent_1", "Message 3");
        bus.send(msg1).unwrap();
        bus.send(msg2).unwrap();
        bus.send(msg3).unwrap();

        assert_eq!(bus.message_count("agent_1"), 3);

        // Receiving one message should leave 2
        let _ = bus.receive("agent_1");
        assert_eq!(bus.message_count("agent_1"), 2);
    }

    #[test]
    fn test_agent_execution() {
        let execution = AgentExecution::new("agent_1".to_string());

        assert_eq!(execution.agent_id, "agent_1");
        assert_eq!(execution.response, "");
        assert_eq!(execution.tool_calls, 0);
        assert_eq!(execution.tokens_used, 0);
    }
}
