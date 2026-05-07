# Subtask 4/6: Glitter Verbs Code Analysis

## Source Location

**Path:** `/Users/ghost/Desktop/nyx clone/packages/client/src/lib/orchestrator/orchestratorGlitterVerbs.ts`

## Overview

The glitter verbs system provides human-readable, Claude Code-style short labels for orchestrator phases. These are displayed while the model or tools are active, giving users clear, engaging feedback about what the agent is doing.

## Core Functions

### 1. `glitterVerbForPrepare()`
```typescript
export function glitterVerbForPrepare(): string {
  return 'Preparing session…'
}
```
- **When:** Before the workspace prompt is built
- **Purpose:** Avoids a dead "Planning" screen during `getWorkspace`
- **Output:** Static text "Preparing session…"

### 2. `glitterVerbForLlm(iteration: number)`
```typescript
export function glitterVerbForLlm(iteration: number): string {
  if (iteration <= 1) return 'Round 1 — calling model…'
  return 'Thinking…'
}
```
- **When:** Before each LLM round
- **Purpose:** Shows which iteration the agent is on
- **Avoids:** "Planning…" alone (users read it as a frozen spinner before model responds)
- **Output:**
  - Round 1: "Round 1 — calling model…"
  - Round 2+: "Thinking…"

### 3. `glitterVerbForLlmPending(elapsedMs: number, iteration: number = 1)`
```typescript
export function glitterVerbForLlmPending(elapsedMs: number, iteration: number = 1): string {
  const s = Math.max(0, Math.floor(elapsedMs / 1000))
  const verb = llmPendingVerbAt(elapsedMs, iteration)
  if (s <= 0) return verb
  if (s < 45) return `${verb} · ${s}s`
  return `${verb} · ${s}s · tool rounds can be slow`
}
```
- **When:** Fires ~2×/s while HTTP chat request is in flight
- **Purpose:** So UI does not look frozen during long model latency
- **Output:** Dynamically changing verb with elapsed time
- **Examples:**
  - "Contemplating…"
  - "Contemplating… · 2s"
  - "Contemplating… · 60s · tool rounds can be slow"

### 4. `glitterVerbForTools(names: string[])`
```typescript
export function glitterVerbForTools(names: string[]): string {
  const uniq = [...new Set(names)].filter(Boolean).sort()
  if (uniq.length === 0) return 'Working…'
  if (uniq.length === 1) {
    const n = uniq[0]
    return VERB_BY_TOOL[n] ?? `Running ${n}…`
  }
  const allSame = uniq.every((x) => x === uniq[0])
  if (allSame) {
    const base = VERB_BY_TOOL[uniq[0]] ?? `Running ${uniq[0]}`
    const stripped = base.replace(/…$/, '')
    return `${stripped} (×${uniq.length})…`
  }
  return `Running ${uniq.length} tools…`
}
```
- **When:** During tool execution
- **Purpose:** Shows which tools are running
- **Output:**
  - No tools: "Working…"
  - Single tool: "Reading…" (or tool-specific verb)
  - Multiple same tools: "Reading (×3)…"
  - Multiple different tools: "Running 3 tools…"

## LLM Pending Verbs List

The system uses a pseudo-random walk through 20 engaging verbs:

```typescript
const LLM_PENDING_VERBS = [
  'Contemplating…',
  'Kerplunking through context…',
  'Cannonballing into the prompt…',
  'Synthesizing…',
  'Pondering…',
  'Interrogating the vibes…',
  'Triangulating an answer…',
  'Mulling with intent…',
  'Orbiting the problem…',
  'Cross-referencing the universe…',
  'Concocting a response…',
  'Massaging the latent space…',
  'Scheming productively…',
  'Doing a little epistemology…',
  'Nibbling on possibilities…',
  'Calibrating brilliance…',
  'Juggling hypotheses…',
  'Whispering to the tensors…',
  'Assembling cleverness…',
  'Reasoning with panache…',
]
```

### Verb Selection Algorithm

```typescript
const LLM_PENDING_BUCKET_MS = 2200

function llmPendingVerbAt(elapsedMs: number, iteration: number): string {
  const bucket = Math.max(0, Math.floor(elapsedMs / LLM_PENDING_BUCKET_MS))
  const roundOffset = Math.max(0, iteration - 1) * 5
  const idx = (bucket * 7 + 3 + roundOffset) % LLM_PENDING_VERBS.length
  return LLM_PENDING_VERBS[idx]!
}
```

**Key Design Decisions:**
- **Bucket size:** 2.2 seconds per verb change
- **Deterministic:** Same time → same verb (no flickering)
- **Iteration offset:** Each tool round shifts the verb sequence
- **Algorithm:** `(bucket * 7 + 3 + roundOffset) % 20`

**Why this matters:**
- Prevents verb flickering during rapid re-renders
- Provides variety without chaos
- Changes predictably over time
- Differentiates between tool rounds

## Tool-Specific Verbs

```typescript
const VERB_BY_TOOL: Record<string, string> = {
  read_file: 'Reading…',
  write_file: 'Writing…',
  delete_file: 'Deleting…',
  list_directory: 'Listing…',
  open_workspace: 'Opening workspace…',
  canvas_list_modules: 'Scanning canvas…',
  canvas_create_tile: 'Adding tiles…',
  canvas_update_tile: 'Updating canvas…',
}
```

**Design principles:**
- Present continuous tense (ing)
- Ends with ellipsis (…)
- Describes action, not result
- Short and punchy (1-3 words)

## Integration Points

### Activity Payload Type

```typescript
export type OrchestratorActivityPayload =
  | { kind: 'prepare' }
  | { kind: 'llm'; iteration: number }
  | { kind: 'llm_pending'; iteration: number; elapsedMs: number }
  | { kind: 'tools'; iteration: number; toolNames: string[] }
```

### Usage in Orchestrator Loop

From `runOrchestrator.ts`:

```typescript
export async function runOrchestratorAgent(
  options: RunOrchestratorOptions
): Promise<{ assistantText: string; messages: ChatMessage[] }> {
  const { onActivity, ... } = options

  // 1. Prepare phase
  resetOrchestratorPaceClock()
  onActivity?.({ kind: 'prepare' })  // → "Preparing session…"

  // 2. LLM round start
  onActivity?.({ kind: 'llm', iteration: iterations })  // → "Round 1 — calling model…" / "Thinking…"

  // 3. LLM pending (heartbeat)
  const hb = window.setInterval(() => {
    const elapsedMs = Date.now() - pendingSince
    onActivity?.({
      kind: 'llm_pending',
      iteration: iterations,
      elapsedMs,
    })  // → "Contemplating… · 2s"
  }, 500)

  // 4. Tool execution
  const toolNames = effectiveToolCalls.map((tc) => tc.function?.name).filter(Boolean) as string[]
  onActivity?.({ kind: 'tools', iteration: iterations, toolNames })  // → "Reading…"
}
```

### UI Integration Pattern

The UI receives activity payloads and maps them to glitter verbs:

```typescript
function mapActivityToVerb(payload: OrchestratorActivityPayload): string {
  switch (payload.kind) {
    case 'prepare':
      return glitterVerbForPrepare()
    case 'llm':
      return glitterVerbForLlm(payload.iteration)
    case 'llm_pending':
      return glitterVerbForLlmPending(payload.elapsedMs, payload.iteration)
    case 'tools':
      return glitterVerbForTools(payload.toolNames)
  }
}
```

## Rust TUI Adaptation Strategy

### 1. Translate Types to Rust

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ActivityKind {
    Prepare,
    Llm { iteration: u32 },
    LlmPending { iteration: u32, elapsed_ms: u64 },
    Tools { iteration: u32, tool_names: Vec<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityPayload {
    pub kind: ActivityKind,
}
```

### 2. Port Verb Lists

```rust
const LLM_PENDING_VERBS: &[&str] = &[
    "Contemplating…",
    "Kerplunking through context…",
    "Cannonballing into the prompt…",
    "Synthesizing…",
    "Pondering…",
    "Interrogating the vibes…",
    "Triangulating an answer…",
    "Mulling with intent…",
    "Orbiting the problem…",
    "Cross-referencing the universe…",
    "Concocting a response…",
    "Massaging the latent space…",
    "Scheming productively…",
    "Doing a little epistemology…",
    "Nibbling on possibilities…",
    "Calibrating brilliance…",
    "Juggling hypotheses…",
    "Whispering to the tensors…",
    "Assembling cleverness…",
    "Reasoning with panache…",
];

const LLM_PENDING_BUCKET_MS: u64 = 2200;

fn llm_pending_verb_at(elapsed_ms: u64, iteration: u32) -> &'static str {
    let bucket = elapsed_ms.saturating_div(LLM_PENDING_BUCKET_MS);
    let round_offset = iteration.saturating_sub(1) * 5;
    let idx = (bucket * 7 + 3 + round_offset as u64) % LLM_PENDING_VERBS.len() as u64;
    LLM_PENDING_VERBS[idx as usize]
}
```

### 3. Port Verb Functions

```rust
use std::collections::HashMap;

pub fn glitter_verb_for_prepare() -> &'static str {
    "Preparing session…"
}

pub fn glitter_verb_for_llm(iteration: u32) -> String {
    if iteration <= 1 {
        "Round 1 — calling model…".to_string()
    } else {
        "Thinking…".to_string()
    }
}

pub fn glitter_verb_for_llm_pending(elapsed_ms: u64, iteration: u32) -> String {
    let s = elapsed_ms / 1000;
    let verb = llm_pending_verb_at(elapsed_ms, iteration);
    if s == 0 {
        verb.to_string()
    } else if s < 45 {
        format!("{} · {}s", verb, s)
    } else {
        format!("{} · {}s · tool rounds can be slow", verb, s)
    }
}

fn tool_verb_for_name(name: &str) -> Option<&'static str> {
    match name {
        "read_file" => Some("Reading…"),
        "write_file" => Some("Writing…"),
        "delete_file" => Some("Deleting…"),
        "list_directory" => Some("Listing…"),
        "open_workspace" => Some("Opening workspace…"),
        "canvas_list_modules" => Some("Scanning canvas…"),
        "canvas_create_tile" => Some("Adding tiles…"),
        "canvas_update_tile" => Some("Updating canvas…"),
        _ => None,
    }
}

pub fn glitter_verb_for_tools(names: &[String]) -> String {
    let mut uniq: Vec<_> = names.iter().filter(|s| !s.is_empty()).cloned().collect();
    uniq.sort();
    uniq.dedup();

    if uniq.is_empty() {
        return "Working…".to_string();
    }

    if uniq.len() == 1 {
        let name = &uniq[0];
        return tool_verb_for_name(name)
            .unwrap_or(&format!("Running {}…", name))
            .to_string();
    }

    let all_same = uniq.iter().all(|x| x == &uniq[0]);
    if all_same {
        let base = tool_verb_for_name(&uniq[0])
            .unwrap_or(&format!("Running {}", uniq[0]));
        let stripped = base.trim_end_matches('…');
        return format!("{} (×{})…", stripped, uniq.len());
    }

    format!("Running {} tools…", uniq.len())
}
```

### 4. Integrate with App State

```rust
pub struct App {
    // ... existing fields
    pub activity_kind: ActivityKind,
    pub verb: String,
    pub activity_started_at: Option<Instant>,
}
```

### 5. Update Event Processing

```rust
impl App {
    pub fn update_activity(&mut self, payload: ActivityPayload) {
        self.activity_kind = payload.kind;
        self.activity_started_at = Some(Instant::now());
        self.verb = self.calculate_verb();
    }

    fn calculate_verb(&self) -> String {
        match &self.activity_kind {
            ActivityKind::Prepare => glitter_verb_for_prepare().to_string(),
            ActivityKind::Llm { iteration } => glitter_verb_for_llm(*iteration),
            ActivityKind::LlmPending { iteration, .. } => {
                let elapsed = self.activity_started_at
                    .map(|t| t.elapsed().as_millis() as u64)
                    .unwrap_or(0);
                glitter_verb_for_llm_pending(elapsed, *iteration)
            }
            ActivityKind::Tools { tool_names, .. } => glitter_verb_for_tools(tool_names),
        }
    }

    pub fn update_pending_verb(&mut self) {
        if let ActivityKind::LlmPending { iteration, .. } = self.activity_kind {
            let elapsed = self.activity_started_at
                .map(|t| t.elapsed().as_millis() as u64)
                .unwrap_or(0);
            self.verb = glitter_verb_for_llm_pending(elapsed, *iteration);
        }
    }
}
```

## Key Design Patterns

### 1. Deterministic Verb Selection
- Uses elapsed time + iteration to calculate verb index
- Prevents flickering during rapid re-renders
- Consistent behavior for same conditions

### 2. Time-Based Progression
- Verbs change every 2.2 seconds
- Adds elapsed time to verb after 1 second
- Adds "tool rounds can be slow" after 45 seconds

### 3. Tool Consolidation
- Deduplicates tool names
- Shows count for repeated tools
- Shows total count for mixed tools

### 4. Engagement Without Distraction
- Short, punchy verbs (1-3 words)
- Present continuous tense (active)
- Ends with ellipsis (in progress)
- Changes predictably over time

## Next Steps for Integration

### Phase 1: Port glitter verbs module
- Create `src/tui/glitter_verbs.rs` with all verb functions
- Add tests for verb selection algorithm
- Verify deterministic behavior

### Phase 2: Integrate with event system
- Add `ActivityKind` enum to app state
- Update event processing to track activity
- Add heartbeat timer for LLM pending state

### Phase 3: Update rendering
- Display verb in status line
- Handle verb updates during LLM pending
- Test with long-running operations

### Phase 4: Polish
- Adjust timing if needed
- Add custom tool verbs for ferroclaw tools
- Ensure smooth transitions between states

## Success Criteria

- [x] Identified glitter verbs code in external orchestrator
- [x] Analyzed verb selection algorithm and design patterns
- [x] Created Rust port strategy
- [ ] Port glitter verbs module to Rust
- [ ] Integrate with app state and event system
- [ ] Update rendering to display verbs
- [ ] Test with long-running operations
- [ ] Ensure deterministic verb selection

**Status: Analysis Complete ✅**
**Next:** Port glitter verbs module to Rust
