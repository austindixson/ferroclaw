# Hermes-Style Agent Enhancement Research & Planning

## Date: 2025-02-10

## Inspirational Repositories

### 1. gbrain (garrytan/gbrain)
**URL**: https://github.com/garrytan/gbrain/tree/master

**Key Concepts to Study**:
- Graph-based memory organization
- Long-term memory consolidation
- Task decomposition patterns
- Self-reflection mechanisms

**Potential Features**:
- Knowledge graph for project understanding
- Automated task breakdown
- Learning from past actions

### 2. hermes-agent (NousResearch/hermes-agent)
**URL**: https://github.com/NousResearch/hermes-agent

**Key Concepts to Study**:
- Multi-agent orchestration
- Tool composition patterns
- Hierarchical agent structure
- Memory management across agents

**Potential Features**:
- Improved AgentTool with real AgentLoop integration
- Agent-to-agent communication
- Hierarchical task delegation
- Shared memory spaces

### 3. pi-mono Coding Agent (badlogic/pi-mono)
**URL**: https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent

**Key Concepts to Study**:
- Specialized coding agent patterns
- File operation strategies
- Context window optimization
- Testing integration

**Potential Features**:
- Code-aware agent behavior
- Smart file editing strategies
- Test-driven development workflow
- Project structure understanding

---

## Current Ferroclaw Capabilities

### ✅ Already Implemented

1. **Core Agent Loop** (`src/agent/loop.rs`)
   - ReAct (Reason + Act) cycle
   - Streaming responses
   - Token budget management
   - Context pruning
   - WebSocket event broadcasting

2. **AgentTool** (`src/tools/agent.rs`)
   - Six built-in agent types (planner, coder, reviewer, debugger, researcher, generic)
   - AgentDefinition with customizable prompts
   - AgentMemory for isolated conversation history
   - AgentRegistry for managing multiple agents
   - Agent resumption via agent_id

3. **TaskSystem** (`src/tasks/`)
   - SQLite-backed persistent storage
   - Dependency tracking
   - Status workflow (pending → in_progress → completed)
   - Cycle detection
   - CLI integration

4. **Memory Systems**
   - SQLite MemoryStore with FTS5 search
   - MemdirSystem for file-based memory
   - Automatic truncation

5. **PlanMode** (`src/modes/plan.rs`)
   - Four-phase workflow (Research, Planning, Implementation, Verification)
   - Dependency-based wave execution
   - Integration with TaskSystem

6. **FileEditTool**
   - Exact string replacement
   - Uniqueness validation
   - Atomic operations

7. **HookSystem**
   - Six lifecycle hooks
   - Five built-in hooks (Logging, Audit, RateLimit, Security, Metrics)

8. **Git Workflow**
   - Commit command (conventional commits)
   - Review command (automated code review)

---

## Gap Analysis: Current vs Hermes-Style Agents

### Current AgentTool Limitations

1. **Simulation vs Real Execution**
   - Current: Simulated responses without real LLM calls
   - Need: Integration with actual AgentLoop

2. **Shared State**
   - Current: No sharing between agents
   - Need: Memory store integration, shared context

3. **Tool Filtering**
   - Current: `allowed_tools` not enforced
   - Need: Filtered tool registry per agent

4. **Token Budgeting**
   - Current: No per-agent budgeting
   - Need: Independent ContextManager per agent

5. **Agent Coordination**
   - Current: Agents operate independently
   - Need: Agent-to-agent messaging, orchestration patterns

---

## Feature Prioritization

### Phase 1: Core Agent Integration (High Priority)

#### 1.1 Real AgentLoop Integration
**Goal**: Make AgentTool spawn real AgentLoop instances instead of simulating responses

**Implementation Tasks**:
- [ ] Create `SubagentConfig` struct to hold agent-specific configuration
- [ ] Implement `AgentLoop::spawn_subagent()` method
- [ ] Update AgentTool to use real AgentLoop execution
- [ ] Pass shared provider and registry to subagents
- [ ] Implement proper async task spawning

**Files to Modify**:
- `src/tools/agent.rs` - Update AgentTool implementation
- `src/agent/loop.rs` - Add spawn_subagent method
- `src/agent/mod.rs` - Export new types

**Design**:
```rust
pub struct SubagentConfig {
    pub agent_id: String,
    pub agent_type: String,
    pub system_prompt: String,
    pub allowed_tools: Vec<String>,
    pub memory_isolation: bool,
    pub token_budget: u64,
}

impl AgentLoop {
    pub fn spawn_subagent(&self, config: SubagentConfig) -> Result<AgentLoop> {
        // Create new AgentLoop with shared provider, isolated context
    }
}
```

#### 1.2 Memory Store Integration for Agents
**Goal**: Allow subagents to use persistent MemoryStore

**Implementation Tasks**:
- [ ] Add `memory_store` field to AgentMemory
- [ ] Implement memory sharing logic based on `memory_isolation` flag
- [ ] Update AgentTool to pass MemoryStore reference
- [ ] Add memory search/store capabilities to subagents

**Files to Modify**:
- `src/tools/agent.rs` - Update AgentMemory struct
- `src/tools/agent.rs` - Update AgentTool initialization

**Design**:
```rust
pub struct AgentMemory {
    pub agent_id: String,
    pub history: Vec<Message>,
    pub store: Option<Arc<Mutex<MemoryStore>>>,  // Shared or isolated
}

impl AgentMemory {
    pub fn with_shared_store(agent_id: String, store: Arc<Mutex<MemoryStore>>) -> Self {
        Self {
            agent_id,
            history: Vec::new(),
            store: Some(store),
        }
    }

    pub fn with_isolated_store(agent_id: String) -> Self {
        Self {
            agent_id,
            history: Vec::new(),
            store: Some(Arc::new(Mutex::new(MemoryStore::new(None).unwrap()))),
        }
    }
}
```

#### 1.3 Tool Filtering Enforcement
**Goal**: Respect `allowed_tools` parameter in subagent tool calls

**Implementation Tasks**:
- [ ] Create `FilteredToolRegistry` wrapper
- [ ] Implement tool filtering logic
- [ ] Update AgentLoop to use filtered registry for subagents
- [ ] Add tests for tool filtering

**Files to Modify**:
- `src/tool.rs` - Add FilteredToolRegistry
- `src/tools/agent.rs` - Use filtered registry
- `src/agent/loop.rs` - Accept filtered registry

**Design**:
```rust
pub struct FilteredToolRegistry {
    inner: ToolRegistry,
    allowed: Option<HashSet<String>>,
}

impl FilteredToolRegistry {
    pub fn new(registry: ToolRegistry, allowed_tools: Option<Vec<String>>) -> Self {
        Self {
            inner: registry,
            allowed: allowed_tools.map(|t| t.into_iter().collect()),
        }
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        if let Some(ref allowed) = self.allowed {
            self.inner
                .definitions()
                .into_iter()
                .filter(|t| allowed.contains(&t.name))
                .collect()
        } else {
            self.inner.definitions()
        }
    }
}
```

#### 1.4 Per-Agent Context Management
**Goal**: Independent token budgeting per subagent

**Implementation Tasks**:
- [ ] Add `context_manager` to AgentMemory
- [ ] Implement per-agent token budgeting
- [ ] Add token usage tracking to AgentExecution result
- [ ] Enforce token limits per agent

**Files to Modify**:
- `src/tools/agent.rs` - Add context to AgentMemory
- `src/tools/agent.rs` - Track token usage in AgentExecution

**Design**:
```rust
pub struct AgentMemory {
    pub agent_id: String,
    pub history: Vec<Message>,
    pub store: Option<Arc<Mutex<MemoryStore>>>,
    pub context: ContextManager,  // New field
}

pub struct AgentExecution {
    pub agent_id: String,
    pub response: String,
    pub tool_calls: usize,
    pub tokens_used: u64,
    pub token_budget_remaining: u64,  // New field
}
```

---

### Phase 2: Advanced Orchestration (Medium Priority)

#### 2.1 Agent-to-Agent Communication
**Goal**: Enable agents to send messages to each other

**Implementation Tasks**:
- [ ] Design message passing protocol
- [ ] Implement agent message queue
- [ ] Add `send_message()` and `receive_messages()` to AgentTool
- [ ] Create agent discovery mechanism

**Files to Create**:
- `src/agent/messaging.rs` - Message passing infrastructure

**Design**:
```rust
pub struct AgentMessage {
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct AgentMessageBus {
    queues: HashMap<String, VecDeque<AgentMessage>>,
}

impl AgentMessageBus {
    pub fn send(&mut self, msg: AgentMessage) -> Result<()>;
    pub fn receive(&mut self, agent_id: &str) -> Vec<AgentMessage>;
}
```

#### 2.2 Hierarchical Task Delegation
**Goal**: Allow planner agents to spawn coder agents for specific tasks

**Implementation Tasks**:
- [ ] Create `Orchestrator` struct for managing agent hierarchy
- [ ] Implement parent-child agent relationships
- [ ] Add task delegation patterns
- [ ] Implement result aggregation from child agents

**Files to Create**:
- `src/agent/orchestrator.rs` - Agent orchestration

**Design**:
```rust
pub struct Orchestrator {
    parent_agent: AgentLoop,
    child_agents: HashMap<String, AgentLoop>,
    message_bus: AgentMessageBus,
}

impl Orchestrator {
    pub fn spawn_child(&mut self, config: SubagentConfig) -> Result<String>;
    pub fn delegate_task(&mut self, child_id: String, task: String) -> Result<AgentExecution>;
    pub fn collect_results(&self) -> HashMap<String, AgentExecution>;
}
```

#### 2.3 Collaboration Patterns
**Goal**: Implement common multi-agent patterns

**Implementation Tasks**:
- [ ] **Reviewer-Coder Loop**: Coder writes code, reviewer provides feedback
- [ ] **Planner-Executor Chain**: Planner breaks down tasks, executors implement
- [ ] **Research-Implement Pipeline**: Researcher gathers info, implementer builds
- [ ] **Parallel Agents**: Multiple coders work on independent tasks

**Files to Create**:
- `src/agent/patterns/` - Predefined collaboration patterns

**Design**:
```rust
pub enum AgentPattern {
    ReviewerCoderLoop {
        coder_id: String,
        reviewer_id: String,
        max_iterations: usize,
    },
    PlannerExecutorChain {
        planner_id: String,
        executor_ids: Vec<String>,
    },
    ParallelExecution {
        agent_ids: Vec<String>,
    },
}

impl AgentPattern {
    pub async fn execute(&mut self, task: &str) -> Result<Vec<AgentExecution>>;
}
```

---

### Phase 3: Memory & Learning (Medium Priority)

#### 3.1 Knowledge Graph Memory
**Goal**: Store project structure as a graph (inspired by gbrain)

**Implementation Tasks**:
- [ ] Design knowledge graph schema (nodes: files, functions, concepts; edges: calls, imports, depends_on)
- [ ] Implement graph storage (SQLite or separate database)
- [ ] Add graph query capabilities
- [ ] Create graph-building tools for agents

**Files to Create**:
- `src/memory/graph.rs` - Knowledge graph implementation
- `src/tools/graph_query.rs` - Graph query tool

**Design**:
```rust
pub struct KnowledgeGraph {
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl KnowledgeGraph {
    pub fn add_node(&self, id: String, type: String, attributes: HashMap<String, Value>) -> Result<()>;
    pub fn add_edge(&self, from: String, to: String, type: String) -> Result<()>;
    pub fn query(&self, query: GraphQuery) -> Result<Vec<GraphNode>>;
}

pub struct GraphQuery {
    pub node_type: Option<String>,
    pub edges: Vec<EdgeFilter>,
    pub attributes: HashMap<String, Value>,
}
```

#### 3.2 Task Decomposition Learning
**Goal**: Learn from past tasks to improve planning

**Implementation Tasks**:
- [ ] Store completed task patterns
- [ ] Implement pattern matching for new tasks
- [ ] Add task template system
- [ ] Create similarity scoring for tasks

**Files to Create**:
- `src/tasks/learning.rs` - Task pattern learning

**Design**:
```rust
pub struct TaskPattern {
    pub description: String,
    pub steps: Vec<TaskStep>,
    pub dependencies: Vec<String>,
    pub tools_used: Vec<String>,
    pub success_rate: f64,
}

pub struct TaskPatternStore {
    patterns: Vec<TaskPattern>,
}

impl TaskPatternStore {
    pub fn add_pattern(&mut self, pattern: TaskPattern);
    pub fn find_similar(&self, description: &str) -> Vec<TaskPattern>;
    pub fn update_success_rate(&mut self, pattern_id: &str, success: bool);
}
```

#### 3.3 Self-Reflection Mechanism
**Goal**: Agents review their own performance

**Implementation Tasks**:
- [ ] Add reflection phase after task completion
- [ ] Store reflection notes in memory
- [ ] Implement "what went wrong/well" analysis
- [ ] Create improvement suggestions

**Files to Create**:
- `src/agent/reflection.rs` - Self-reflection system

**Design**:
```rust
pub struct Reflection {
    pub task: String,
    pub execution: AgentExecution,
    pub what_worked: Vec<String>,
    pub what_failed: Vec<String>,
    pub improvements: Vec<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Reflection {
    pub async fn generate(task: &str, execution: &AgentExecution) -> Self;
    pub fn store(&self, memory: &MemoryStore) -> Result<()>;
}
```

---

### Phase 4: Testing & Validation (High Priority)

#### 4.1 Multi-Agent Test Suite
**Goal**: Comprehensive tests for agent orchestration

**Implementation Tasks**:
- [ ] Unit tests for AgentLoop spawning
- [ ] Integration tests for tool filtering
- [ ] Memory sharing tests
- [ ] Agent-to-agent communication tests

**Files to Create**:
- `src/agent/orchestration_test.rs`

#### 4.2 Performance Benchmarks
**Goal**: Measure multi-agent system performance

**Implementation Tasks**:
- [ ] Benchmark subagent spawn time
- [ ] Benchmark memory access patterns
- [ ] Benchmark message passing overhead
- [ ] Compare single-agent vs multi-agent performance

**Files to Create**:
- `benches/multi_agent.rs`

#### 4.3 Integration with Existing Features
**Goal**: Ensure new features work with TaskSystem, PlanMode, etc.

**Implementation Tasks**:
- [ ] Test AgentTool with TaskSystem integration
- [ ] Test PlanMode spawning subagents for phases
- [ ] Test Commit command with reviewer agent
- [ ] Test Review command with automated subagent review

---

## Technical Debt & Cleanup

### Code Organization
- [ ] Consolidate agent-related modules under `src/agent/`
- [ ] Separate orchestration concerns from core AgentLoop
- [ ] Improve documentation for AgentTool usage

### Error Handling
- [ ] Add specific error types for agent operations
- [ ] Improve error messages for agent failures
- [ ] Add timeout handling for agent execution

### Testing
- [ ] Increase test coverage for agent-related code
- [ ] Add property-based tests for tool filtering
- [ ] Add fuzz tests for message passing

---

## Success Metrics

### Phase 1 Metrics
- ✅ Subagents execute real LLM calls (not simulated)
- ✅ Each subagent has independent token budget
- ✅ Tool filtering is enforced correctly
- ✅ Memory can be shared or isolated per agent

### Phase 2 Metrics
- ✅ Agents can send messages to each other
- ✅ Planner can spawn multiple coder agents
- ✅ Results from child agents can be aggregated
- ✅ Predefined patterns work correctly

### Phase 3 Metrics
- ✅ Knowledge graph stores project structure
- ✅ Task patterns improve over time
- ✅ Self-reflection generates actionable insights

### Phase 4 Metrics
- ✅ All new features have comprehensive tests
- ✅ Performance benchmarks show acceptable overhead
- ✅ Integration with existing features works

---

## Dependencies & Prerequisites

### Required
- Current Ferroclaw codebase (✅ already in place)
- Cargo workspace structure
- SQLite for knowledge graph storage
- Test infrastructure

### Nice to Have
- Visualization tools for agent graphs
- Performance profiling tools
- Agent tracing/debugging tools

---

## Timeline Estimate

- **Phase 1**: 2-3 weeks (core integration)
- **Phase 2**: 2-3 weeks (orchestration)
- **Phase 3**: 3-4 weeks (memory & learning)
- **Phase 4**: Ongoing (testing & validation)

**Total**: 9-10 weeks for full implementation

---

## Open Questions

1. **Concurrency Model**: Should subagents run in parallel tokio tasks or sequentially?
   - *Proposal*: Parallel with configurable max concurrency

2. **Memory Sharing**: Should all subagents share memory by default?
   - *Proposal*: Opt-in sharing via `memory_isolation: false`

3. **Agent Discovery**: How should agents find each other?
   - *Proposal*: Centralized registry with message bus

4. **Error Propagation**: How should child agent errors affect parent?
   - *Proposal*: Graceful degradation with error reporting

5. **Resource Limits**: How to prevent runaway subagent spawning?
   - *Proposal*: Configurable max_agents and max_depth limits

---

## Next Steps

1. **Immediate**: Start with Phase 1.1 (Real AgentLoop Integration)
2. **Research**: Study gbrain's knowledge graph implementation
3. **Testing**: Set up multi-agent test infrastructure
4. **Documentation**: Update docs as features are implemented

---

*Last updated: 2025-02-10*
