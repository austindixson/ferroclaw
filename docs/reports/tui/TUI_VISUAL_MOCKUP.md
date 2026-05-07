# Ferroclaw TUI - Visual Layout Mockup

## Current Interface: Kinetic TUI (V3)

---

## Full Screen Layout (Default Terminal Size: 80x24)

```
╔══════════════════════════════════════════════════════════════════════════════╗
║ ● Thinking… model·3 45% ████████░░ 8/s                    ║  ← Line 1: Kinetic Status
║ iteration 7 · 12s                                             ║  ← Line 2: Iteration Info
║                                                                     ║
║ ┌─ Chat ─────────────────────────────────────────────────────────────┐ ║
║ │                                                                      │ ║
║ │  You: How do I grep for a pattern across all Rust files?           │ ║  ← User message
║ │                                                                      │ ║
║ │  ┌─ Ferroclaw ───────────────────────────────────────────────────┐ │ ║
║ │  │ I'll search for your pattern using grep across the src/        │ │ ║  ← Assistant bubble
║ │  │ directory.                                                      │ │ ║
║ │  └─────────────────────────────────────────────────────────────────┘ │ ║
║ │                                                                      │ ║
║ │  ⚡ grep "pattern" src/**/*.rs -n                         [0.2s]    │ ║  ← Tool call (glitch effect)
║ │                                                                      │ ║
║ │  ← ✓ grep: Found 23 matches in 8 files                               │ ║  ← Success result (green)
║ │                                                                      │ ║
║ │  Here are the top results...                                        │ ║
║ │                                                                      │ ║
║ └──────────────────────────────────────────────────────────────────────┘ ║
║                                                                     ║
║ ● Contemplating…                                                ║  ← Glitter verb
║                                                                     ║
║ ┌─ Type your message... ───────────────────────────────────────────────┐ ║
║ │ > Show me the first 10 results_                                      │ ║  ← Input (bordered)
║ └──────────────────────────────────────────────────────────────────────┘ ║
║                                                                     ║
╚══════════════════════════════════════════════════════════════════════════════╝
  ↑ scroll through history · Ctrl+C to quit · Shift+Enter for newline
```

---

## Component Breakdown

### 1. Kinetic Status Bar (Lines 1-2)

```
╔══════════════════════════════════════════════════════════════════════════════╗
║ ● Thinking… model·3 45% ████████░░ 8/s                    ║
║ iteration 7 · 12s                                             ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

**Elements**:

| Element | Symbol | Color | Meaning |
|---------|--------|-------|---------|
| Thinking indicator | ● | Cyan | Filled circle = running |
| Verb | `Thinking…` | White | Current agent state |
| Model | `model·3` | Dark Gray | LLM model identifier |
| Token % | `45%` | Yellow | Tokens used / budget |
| Progress bar | `████████░░` | Cyan | Visual token usage |
| Velocity | `8/s` | Yellow | Tokens per second |
| Iteration | `iteration 7` | White | Agent loop iteration |
| Elapsed time | `12s` | White | Time since run started |

**Progress Bar Detail**:
```
████████░░
│││││││││││
└─ 8 characters used (cyan)
   └─ 2 characters remaining (gray)
```

**Animation States**:
- `●` + cyan pulse = Running
- `○` + green = Ready/idle
- `●` + red flash = Error

---

### 2. Chat Area (Bordered)

```
┌─ Chat ───────────────────────────────────────────────────────────────┐
│                                                                      │
│  You: How do I grep for a pattern?                                   │
│                                                                      │
│  ┌─ Ferroclaw ───────────────────────────────────────────────────┐ │
│  │ I'll search for your pattern...                                  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│  ⚡ grep "pattern" src/**/*.rs -n                         [0.2s]    │
│                                                                      │
│  ← ✓ grep: Found 23 matches in 8 files                               │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

**Message Types**:

#### User Message
```
  You: How do I grep for a pattern?
  └─ Cyan/White text, left-aligned
```

#### Assistant Message (Bubbles)
```
  ┌─ Ferroclaw ───────────────────────────────────────────────────┐
  │ I'll search for your pattern...                               │
  └─────────────────────────────────────────────────────────────────┘
  └─ White border, white text, indented
```

#### Tool Call (Glitch Effect)
```
  ⚡ grep "pattern" src/**/*.rs -n                         [0.2s]
  ││ └─ Yellow text, tool name + args
  └─ ⚡ symbol flickers for first 2 frames (glitch animation)
```

#### Tool Result (Success)
```
  ← ✓ grep: Found 23 matches in 8 files
  ││ └─ Green text, tool name + summary
```

#### Tool Result (Error)
```
  ← ✗ grep: No matches found
  ││ └─ Red text, error message
```

---

### 3. Glitter Verbs

Animated status line between chat and input:

```
● Contemplating…                         ║
││ └─ Cyan symbol + verb
   └─ Animates when agent is active
```

**Verbs by Context**:

| Verb | Context |
|------|---------|
| `ready` | Idle, waiting for input |
| `Contemplating…` | Initial LLM thinking (first think) |
| `Reading…` | Active `read_file` tools |
| `Writing…` | Active `write_file` tools |
| `Searching…` | Active grep/search tools |
| `Executing…` | Active bash commands |
| `[10s] thinking...` | After 10s of inactivity (nudge) |

---

### 4. Input Area (Bordered)

```
┌─ Type your message... ───────────────────────────────────────────────┐
│ > Show me the first 10 results_                                      │
└──────────────────────────────────────────────────────────────────────┘
```

**Elements**:

| Element | Description |
|---------|-------------|
| Border | Hermes-style single-line border |
| Label | `Type your message...` |
| Prompt | `>` cyan symbol |
| Cursor | `_` blinking underscore |
| Text | Current user input (cyan) |

**Multiline Support**:

```
┌─ Type your message... ───────────────────────────────────────────────┐
│ > First line of input                                                │
│   Second line (Shift+Enter for newline)                              │
│   _
└──────────────────────────────────────────────────────────────────────┘
```

---

## Color Scheme Reference

```rust
Cyan      #06FFFF  → Running state ●, progress bar █, user prompt >
Green     #00FF00  → Ready state ○, success results ← ✓
Yellow    #FFFF00  → Token %, velocity 8/s, tool calls ⚡
Red       #FF0000  → Error state, errors ← ✗
White     #FFFFFF  → Assistant responses, normal text
DarkGray  #444444  → Model name, metadata
```

---

## Alternative States

### Idle State

```
╔══════════════════════════════════════════════════════════════════════════════╗
║ ●ready model·3 0% ░░░░░░░░░░░░░░░ 0/s                        ║
║ iteration 0                                                     ║
║                                                                     ║
║ ┌─ Chat ─────────────────────────────────────────────────────────────┐ ║
║ │                                                                      │ ║
║ │  Ferroclaw is ready. Type a message to begin.                       │ ║
║ │                                                                      │ ║
║ └──────────────────────────────────────────────────────────────────────┘ ║
║                                                                     ║
║ ●ready                                                          ║
║                                                                     ║
║ ┌─ Type your message... ───────────────────────────────────────────────┐ ║
║ │ > _                                                                  │ ║
║ └──────────────────────────────────────────────────────────────────────┘ ║
║                                                                     ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

### Error State

```
╔══════════════════════════════════════════════════════════════════════════════╗
║ ●ERROR! model·3 12% ██░░░░░░░░░░░ 0/s                        ║
║ iteration 2 · 3s                                            ║
║                                                                     ║
║ ┌─ Chat ─────────────────────────────────────────────────────────────┐ ║
║ │                                                                      │ ║
║ │  ← ✗ Failed to read file: No such file or directory                │ ║
║ │                                                                      │ ║
║ └──────────────────────────────────────────────────────────────────────┘ ║
║                                                                     ║
║ ●ERROR!                                                        ║
║                                                                     ║
║ ┌─ Type your message... ───────────────────────────────────────────────┐ ║
║ │ > Try again_                                                         │ ║
║ └──────────────────────────────────────────────────────────────────────┘ ║
║                                                                     ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

---

## Comparison: Minimal TUI Alternative

For reference, here's the Minimal TUI layout:

```
╔══════════════════════════════════════════════════════════════════════════════╗
║ ●ready model·3                                                    ║
║                                                                     ║
║   You: How do I grep for a pattern?                                   ║
║                                                                      ║
║   Ferroclaw: I'll search for your pattern across the src/ directory.   ║
║                                                                      ║
║   → grep "pattern" src/**/*.rs -n                                     ║
║   ← ✓ grep: Found 23 matches in 8 files                                ║
║                                                                      ║
║   [You] Show me the first 10 results                                  ║
║ > _                                                                  ║
║                                                                     ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

**Key Differences**:
- No borders
- One-line status (`●ready model·3`)
- Transcript format (no bubbles)
- Raw transcript markers (`→`, `← ✓`, `← ✗`)
- Maximum screen space for content

---

## Responsive Behavior

### Small Terminal (60x15)

```
╔══════════════════════════════════════════╗
║ ● Thinking… 45% ████░░ 8/s       ║
║ iteration 7 · 12s                ║
║ ┌─ Chat ───────────────────┐    ║
║ │  You: How do I grep?     │    ║
║ │  ┌─ Ferroclaw ────────┐  │    ║
║ │  │ I'll search...      │  │    ║
║ │  └─────────────────────┘  │    ║
║ │  ← ✓ grep: 23 matches     │    ║
║ └────────────────────────────┘    ║
║ ● Contemplating…             ║
║ ┌─ Type your message... ────┐    ║
║ │ > Show results_            │    ║
║ └────────────────────────────┘    ║
╚══════════════════════════════════════════╝
```

- Progress bar compresses
- Bubbles wrap text
- Input area remains visible

### Large Terminal (120x40)

```
╔══════════════════════════════════════════════════════════════════════════════════════════════════╗
║ ● Thinking… model·3 45% ████████████░░░░░░░░ 8/s                                ║
║ iteration 7 · 12s                                                                  ║
║                                                                                    ║
║ ┌─ Chat ──────────────────────────────────────────────────────────────────────────┐ ║
║ │                                                                                  │ ║
║ │  You: How do I grep for a specific pattern across all Rust files in src/?       │ ║
║ │                                                                                  │ ║
║ │  ┌─ Ferroclaw ───────────────────────────────────────────────────────────────┐ │ ║
║ │  │ I'll help you search for that pattern. I'll use grep to scan the src/     │ │ ║
║ │  │ directory and return all matching lines with line numbers.                 │ │ ║
║ │  └───────────────────────────────────────────────────────────────────────────┘ │ ║
║ │                                                                                  │ ║
║ │  ⚡ grep "pattern" src/**/*.rs -n -i --color=always                     [0.2s] │ ║
║ │                                                                                  │ ║
║ │  ← ✓ grep: Found 23 matches in 8 files                                           │ ║
║ │                                                                                  │ ║
║ │  Here are the first 10 results:                                                 │ ║
║ │    src/main.rs:42: fn grep_pattern(&self, pattern: &str) {                     │ ║
║ │    src/utils.rs:15: fn search_files(dir: &Path, pattern: &str) {               │ ║
║ │    ...                                                                           │ ║
║ │                                                                                  │ ║
║ └──────────────────────────────────────────────────────────────────────────────────┘ ║
║ ● Contemplating…                                                            ║
║ ┌─ Type your message... ────────────────────────────────────────────────────────────┐ ║
║ │ > Show me the next 10 results_                                                    │ ║
║ └───────────────────────────────────────────────────────────────────────────────────┘ ║
║                                                                                    ║
╚══════════════════════════════════════════════════════════════════════════════════════════════════╝
```

- Full progress bar
- Long tool calls visible
- More chat history visible
- Wrapped text in bubbles

---

## Key Interactions

### Keyboard Controls

| Key | Action |
|-----|--------|
| `Ctrl+C` | Quit TUI |
| `Enter` | Send message |
| `Shift+Enter` | Newline in input |
| `↑` | Scroll chat up |
| `↓` | Scroll chat down |
| `PageUp` | Scroll up by page |
| `PageDown` | Scroll down by page |
| `Home` | Jump to start of line |
| `End` | Jump to end of line |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character at cursor |
| `←` / `→` | Move cursor left/right |

### Mouse Controls

| Action | Behavior |
|--------|----------|
| Scroll wheel up | Scroll chat up |
| Scroll wheel down | Scroll chat down |

---

## Animation Timings

| Animation | Duration | Trigger |
|-----------|----------|---------|
| Glitch effect (⚡) | 2 frames (200ms) | New tool call appears |
| Thinking indicator | Continuous pulse | Agent running |
| Glitter verb | Updates every 100ms | State change |
| Progress bar | Updates every tick | Token usage changes |
| Cursor blink | Continuous | Default terminal behavior |

---

## Summary

The current **Kinetic TUI (V3)** features:
- Dense, information-rich interface
- Animated status with progress bar and velocity
- Hermes-style borders
- Chat bubble format
- Glitch effects on tool calls
- Glitter verbs for contextual status

This document provides a complete visual reference for the current TUI layout to guide the redesign process.

---

**Created**: 2026-04-13
**Based On**: `src/tui/kinetic_tui.rs` implementation
