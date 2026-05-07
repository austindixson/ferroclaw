# Ferroclaw TUI Designs Overview

This document describes the different Terminal User Interface (TUI) designs implemented in Ferroclaw.

## Available TUI Variants

### 1. Standard TUI (`src/tui/mod.rs` + `ui.rs`)
**Design Philosophy:** Classic TUI with structured layout

**Features:**
- Top banner bar with model name and token budget
- Scrollable chat history
- Multiline input area with cursor support
- Status bar with connection info
- Borders and padding for structure
- Keyboard shortcuts (Ctrl+C, Ctrl+L, PageUp/PageDown, Shift+Arrow keys)

**Visual Style:** Traditional ratatui style with block borders

---

### 2. Kinetic TUI (`src/tui/kinetic_tui.rs`)
**Design Philosophy:** "Motion is information - the interface breathes with the agent"

**Features:**
- **Kinetic status bar** at top with pulse animation and progress bar
- **No borders** - typography over chrome
- **Glitter verbs** - animated status messages (thinking, contemplating, mulling, etc.)
- **Glitch aesthetic** - raw and alive, not overly polished
- **Real-time feedback** - status changes are immediately visible
- **Hermes-style borders** on chat and input areas
- Direct glitter verb display above input

**Visual Style:**
- Status: `● Thinking… model·3 45% ████████░░ 8/s`
- Chat bubbles with [Ferro] and [You] markers
- Tool calls: `→ tool_name`
- Tool results: `← tool_name [OK/ERR]`

**Key Innovation:** Uses motion and density to encode state (velocity, depth, pressure)

---

### 3. Hermes TUI (`src/tui/hermes_tui.rs` + `hermes_ui.rs`)
**Design Philosophy:** Inspired by Hermes agent TUI - polished chat interface

**Features:**
- **Dark theme** throughout
- **Message bubbles:**
  - Assistant messages: "Ferroclaw:" header + indented text (cyan, bold)
  - User messages: Orange dot (●) + "You:" + text
- **Bottom status bar** with model and process info
- **Left sidebar** with task management (tasks list with status)
- Tool calls and results shown in chat
- Standard keyboard shortcuts

**Visual Style:**
```
[ FERROCLAW ]  ┌─────────────────────┐  ┌─────────┐
               │ Ferroclaw: Hello!  │  │ Tasks   │
               │   How can I help?  │  │         │
               │                     │  │ ● Task1 │
  [You] Hi    │ ● You: Hi          │  │ ○ Task2 │
               │                     │  │         │
  > input_    │ > type message_    │  └─────────┘
               └─────────────────────┘
   model·3     Status: Ready
```

---

### 4. Orchestrator TUI (`src/tui/orchestrator_tui.rs` + `orchestrator_ui.rs`)
**Design Philosophy:** Nyx-inspired transcript - real-time tool visibility

**Features:**
- **Real-time tool lines** - see every tool call as it happens
- **Dark palette** with **teal accents**
- **Verb indicator** - shows current action (Thinking…, Reading…, Writing…, Executing…)
- **Transcript-style display:**
  - `◆ model → tool_name` - LLM tool choice
  - `→ tool_name(args)` - Tool call start
  - `← tool_name` - Tool result
  - `⋯ Parallel tool batch (N calls)` - Batch operations
- **Iteration counter** - shows LLM round number
- **Long wait nudges** - displays messages after 30s if still waiting
- Threaded agent execution with event streaming

**Visual Style:**
```
● Thinking… · 3

[Ferro] Welcome to Ferroclaw!
[You] Read the main.rs file

[Using model: gpt-4]
◆ model → glob
→ glob("*.rs")
← glob
→ read_file("src/main.rs")
← read_file

Ferro: Here's the content of main.rs...
```

**Key Innovation:** Shows the thinking process step-by-step, not just the result

---

### 5. Minimal TUI (`src/tui/minimal_tui.rs`)
**Design Philosophy:** "No borders, no chrome - maximum screen real estate for content"

**Features:**
- **Zero borders** - pure content focus
- **Brutalist terminal aesthetic**
- **Glitter verbs** for status (ready, thinking, contemplating, etc.)
- **Status through typography and position** (not UI widgets)
- **Maximum content area** - no wasted pixels
- Clean minimal markers for different message types

**Visual Style:**
```
● Contemplating… model·3 45%

  Ferro: I can help you build a TUI
  → read_file
  ← ✓ read_file

> Build a TUI interface_
```

**Status line indicators:**
- `●` (filled circle) = Running or Error state
- `○` (empty circle) = Ready state
- Colors: Cyan (running), Red (error), Green (ready)

**Key Innovation:** Pure utilitarian design - every pixel serves the content

---

## How to Launch Each TUI

### Currently Active (Kinetic TUI)
```bash
cd /Users/ghost/Desktop/ferroclaw && ./target/release/ferroclaw run
```
This runs the **Kinetic TUI** (as configured in `src/main.rs` line 62).

### To Launch Other TUIs

To preview different TUI designs, you would need to modify `src/main.rs` line 62:

**Hermes TUI:**
```rust
ferroclaw::tui::hermes_tui::run_hermes_tui(agent_loop, &config).await
```

**Orchestrator TUI:**
```rust
ferroclaw::tui::orchestrator_tui::run_orchestrator_tui(agent_loop, &config).await
```

**Minimal TUI:**
```rust
ferroclaw::tui::minimal_tui::run_minimal_tui(agent_loop, &config).await
```

**Standard TUI:**
```rust
ferroclaw::tui::run_tui(agent_loop, config).await
```

## Comparison Summary

| TUI Variant | Visual Style | Borders | Animation | Best For |
|-------------|--------------|---------|-----------|----------|
| **Standard** | Classic | Yes | No | Traditional users |
| **Kinetic** | Motion-focused | Partial | Yes | Visual feedback |
| **Hermes** | Dark & polished | Yes | No | Task management |
| **Orchestrator** | Transcript-style | Yes | Real-time | Debugging agent flow |
| **Minimal** | Brutalist | No | Glitter verbs | Power users, focus |

## Shared Features Across All TUIs

1. **Keyboard shortcuts:**
   - `Ctrl+C` - Quit
   - `Ctrl+L` - Clear chat
   - `PageUp/PageDown` - Scroll chat by 10 lines
   - `Shift+Up/Shift+Down` - Scroll by 1 line
   - `Shift+Enter` - Newline in input
   - Arrow keys - Navigate within input

2. **Input editing:**
   - Multiline input support
   - Cursor movement with arrows
   - Backspace/Delete support
   - Home/End for line navigation

3. **Chat features:**
   - Scrollable history
   - Auto-scroll to latest messages
   - Tool call/results display
   - Error highlighting

4. **Status tracking:**
   - Token usage (input/output/total)
   - Model name
   - Agent state (thinking/ready/error)

## Glitter Verbs (Kinetic & Minimal TUIs)

Animated status messages that change based on agent activity:

- **Idle:** `ready`, `idle`
- **Thinking:** `thinking`, `contemplating`, `mulling`, `pondering`, `deliberating`
- **Tool active:** `reading`, `writing`, `executing`
- **Error:** `error`, `stuck`
- **Long wait:** `still thinking` (after 10s)

The verbs cycle through to give the interface a "living" feel while the agent works.

---

*Last updated: 2025-01-15*
