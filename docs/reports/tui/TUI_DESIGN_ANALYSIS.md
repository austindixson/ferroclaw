# Ferroclaw TUI - Current State Analysis

## Overview

Ferroclaw has **two TUI implementations** available:

1. **Kinetic TUI (V3)** - Current default, animated, dense
2. **Minimal TUI** - Brutalist, no borders, raw terminal

---

## Current Default: Kinetic TUI (V3)

### Design Philosophy

> "The interface breathes with the agent."

- **Motion is information** — Status changes visible through animation
- **Density encodes state** — Token velocity, iteration depth, memory pressure at a glance
- **Glitch aesthetic** — Raw, alive, not polished
- **No borders** — Typography over chrome

### Visual Layout

```
┌────────────────────────────────────────┐
│ ● Thinking… model·3 45% ████████░░ 8/s │  ← Kinetic status (2 lines)
│ iteration 7 · 12s                       │    (pulsing symbol + progress bar)
│                                         │
│ ┌─ Chat ────────────────────────────┐ │
│ │  Ferroclaw: Response here          │ │  ← Chat history (bordered)
│ │  → tool_call                       │ │     (glitch effects on tool calls)
│ │  ← ✓ tool_result                   │ │
│ └───────────────────────────────────┘ │
│                                         │
│ ● Contemplating…                        │  ← Glitter verb (animated)
│                                         │
│ ┌─ Type your message... ────────────┐ │
│ │ > your input_                      │ │  ← Input (bordered)
│ └───────────────────────────────────┘ │
└────────────────────────────────────────┘
```

### Key Features

#### 1. Two-Line Kinetic Status
- **Line 1**: Symbol + verb + model + iteration + token % + **progress bar** + **velocity**
- **Line 2**: Iteration count + elapsed time (when running)

Example: `● Thinking… model·3 45% ████████░░ 8/s`

#### 2. Progress Bar
```
████████░░ 8.2/s
```
- Visual token usage (█ = used, ░ = remaining)
- Token velocity (tokens/sec) for real-time feedback
- Cyan color for visibility

#### 3. Glitch Effects
- Tool calls flicker with ⚡ symbol for first 2 frames
- Creates sense of activity and motion
- Subtle, not distracting

#### 4. Animated Thinking Indicator
- ● (filled) = running or error
- ○ (hollow) = ready
- Pulses with color: Cyan (running) / Red (error) / Green (ready)

#### 5. Glitter Verbs
Animated status messages:
- `Contemplating…` (initial LLM think)
- `Reading…` (active read_file tools)
- `Writing…` (active write_file tools)
- `Searching…` (active search tools)
- `Executing…` (active bash tools)

### Color Scheme

```rust
Cyan    #06FFFF  → Running state, progress bar
Green   #00FF00  → Ready state
Yellow  #FFFF00  → Iteration count, velocity
Red     #FF0000  → Error state
White   #FFFFFF  → AI responses
DarkGray#444444  → Metadata, model name
```

### Implementation Details

**File**: `src/tui/kinetic_tui.rs` (520 lines)

**Key Functions**:
- `draw_kinetic_status()` — 2-line status with progress bar
- `draw_kinetic()` — Main render with frame_count for animations
- `update_kinetic_state()` — Future: animation state updates
- `draw_chat_history()` — Glitch effects on tool calls

**Animation System**:
- `frame_count` passed to all draw functions
- `GLITCH_FRAMES` constant (currently 2) for glitch duration
- Progress bar updates every tick (100ms intervals via `EventHandler::new(100)`)

---

## Alternative: Minimal TUI

### Design Philosophy

> "Brutalist terminal interface. Raw, alive, utilitarian."

- **No borders, no chrome** — Every pixel serves a purpose
- **Type-driven hierarchy** — Symbols and typography create visual structure
- **Raw aesthetics** — No padding, single-character symbols, monospace rhythm
- **Minimal status** — Symbol-based, not word-based

### Visual Layout

```
┌────────────────────────────────────────┐
│ ●ready model·3                         │  ← Status line (top)
│                                         │
│   Ferroclaw: Response here             │  ← Chat history (borderless)
│   → tool_call                          │     (raw transcript format)
│   ← ✓ tool_result                      │
│                                         │
│   [You] Your prompt here               │
│ > continue typing_                      │  ← Input (minimal prompt)
└────────────────────────────────────────┘
```

### Key Design Decisions

1. **No Borders, No Chrome**
   - Maximum screen real estate for actual work
   - Content breathes, doesn't feel constrained

2. **Type-Driven Hierarchy**
   - `→ tool_name` — tool calls (yellow)
   - `← ✓ tool_name` — successful results (green)
   - `← ✗ tool_name` — errors (red)
   - `> user` — your input (cyan)
   - `  AI response` — assistant (white, indented)
   - `[You]`, `[AI]`, `[ERROR]` — metadata markers

3. **Minimal Status Line**
   - `●` — alive/thinking indicator
   - `○` — idle
   - Symbol-based, not word-based
   - One line, zero wasted space

### Implementation Details

**File**: `src/tui/minimal_tui.rs` (450 lines)

**Key Functions**:
- `draw_minimal()` — Main render function
- `draw_status_line()` — One-line symbolic status
- `draw_content()` — Chat + input, borderless
- `draw_chat_history()` — Raw transcript rendering
- `draw_input()` — Minimal `> ` prompt

---

## Comparison: Kinetic vs Minimal

| Feature | Kinetic (Current) | Minimal (Alt) |
|---------|------------------|---------------|
| Status height | 2 lines | 1 line |
| Progress | Visual bar (`████████░░`) | Text percentage (`45%`) |
| Velocity | Shown (`8.2/s`) | Hidden |
| Animation | Glitch effects, future pulses | None planned |
| Info density | High | Low |
| Timing | Line 2 shows elapsed | Hidden |
| Borders | Yes (Hermes-style) | No |
| Chat format | Bubbles | Transcript |
| Glitter verbs | Yes | Yes |
| Aesthetic | Kinetic, alive | Brutalist, raw |

---

## Shared Components

### App State (`src/tui/app.rs`)

Both TUIs share the same `App` struct:
- `chat_history: Vec<ChatEntry>` — Transcript lines, messages, tools
- `input_lines: Vec<String>` — Multiline input buffer
- `cursor_line`, `cursor_col` — Cursor position
- `scroll_offset` — Chat scroll position
- `model_name`, `token_budget`, `tokens_used` — Token tracking
- `iteration` — Agent loop iteration
- `verb` — Current glitter verb
- `is_running`, `is_error` — Agent state
- `run_started_at` — For timing calculations
- `active_tools: Vec<String>` — For glitter verbs

### Chat Entry Types

```rust
pub enum ChatEntry {
    UserMessage(String),
    AssistantMessage(String),
    TranscriptLine(String),      // Orchestrator-style markers
    ToolCall { name: String, args: String },
    ToolResult { name: String, content: String, is_error: bool },
    SystemInfo(String),
    Error(String),
}
```

### Glitter Verbs (`src/tui/glitter_verbs.rs`)

Animated status messages based on:
- Agent state (running vs idle)
- Iteration count
- Active tools
- Elapsed time

Examples:
- `ready`
- `Contemplating…`
- `Reading…` (read_file active)
- `Writing…` (write_file active)
- `Searching…` (grep/search tools active)
- `Executing…` (bash active)
- `[10s] thinking...` (nudge after 10s)

---

## Event Handling (`src/tui/events.rs`)

### Event Types
- `Tick` — Timer-based redraw (100ms for kinetic, 250ms for minimal)
- `Key` — Keyboard input
- `MouseScrollUp`, `MouseScrollDown` — Mouse scrolling
- `Resize` — Terminal resize

### Key Bindings (Both TUIs)
- `Ctrl+C` — Quit
- `Enter` — Send message
- `Shift+Enter` — Newline in input
- `↑/↓` — Scroll chat
- `PageUp/PageDown` — Scroll by page
- Arrow keys — Cursor movement in input
- `Home/End` — Jump to start/end of line
- `Backspace/Delete` — Delete characters

---

## Current Status

### Kinetic TUI (Default)
- ✅ Implemented
- ✅ Active default (see `src/main.rs:43`)
- ✅ Clean build (0 errors, 9 warnings - unused constants)
- ⚠️ Production testing needed

### Minimal TUI (Alternative)
- ✅ Implemented
- ✅ Available as alternative
- ⚠️ Not currently default

### Planned Enhancements (Kinetic V3)

**Near-term**:
- Real token velocity calculation (currently placeholder "12.3/s")
- Pulse animation on ● symbol (scale/brightness)
- Iteration counter increment animation
- Error state shaking effect

**Mid-term**:
- Memory pressure indicator (heap usage bar)
- Tool timing breakdown (hover or status line)
- Session statistics (total tokens, time, cost)
- Kinetic transitions between states (fade, slide)

**Never** (Design Principles):
- Decorative borders (minimal TUI)
- Multiple panels (orchestrator TUI)
- Task list UI
- Settings screens

---

## Code Locations

| Component | Path | Lines |
|-----------|------|-------|
| Kinetic TUI | `src/tui/kinetic_tui.rs` | 520 |
| Minimal TUI | `src/tui/minimal_tui.rs` | 450 |
| App State | `src/tui/app.rs` | 350 |
| Events | `src/tui/events.rs` | 120 |
| Glitter Verbs | `src/tui/glitter_verbs.rs` | 180 |
| Thinking Indicator | `src/tui/thinking_indicator.rs` | 200 |

---

## Design Principles Summary

### Kinetic TUI
1. **Information First** — Every pixel conveys state
2. **Motion = Life** — Static feels dead, kinetic feels alive
3. **Density Over Whitespace** — Maximize information per line
4. **Symbol Over Label** — Progress bar > "45% used"
5. **Glitch Aesthetic** — Embrace raw terminal, don't hide it

### Minimal TUI
1. **Form Follows Function** — Layout shaped by user's goal
2. **Less is More** — Removed all non-essential UI elements
3. **Typography Over Decoration** — Symbols and spacing create hierarchy
4. **Raw Over Polished** — Embraces terminal nature
5. **Bold Choices** — Full brutalist or nothing

---

## Next Steps for Redesign

Questions to consider:
1. **Direction**: Kinetic (more animated) or Minimal (more brutalist)?
2. **Borders**: Keep Hermes-style or remove entirely?
3. **Layout**: Chat bubbles or transcript format?
4. **Status**: 2-line detailed or 1-line symbolic?
5. **Information Density**: High (Kinetic) or Low (Minimal)?
6. **Animation**: Glitch effects, pulses, or none?
7. **Typography**: What fonts/symbols work best?

---

**Generated**: 2026-04-13
**Analysis Based On**: Code inspection of `src/tui/` module
