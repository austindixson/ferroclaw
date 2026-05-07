# Minimal TUI — Brutalist Terminal Interface

## Design Philosophy

**Purpose**: Clean, distraction-free AI agent interaction  
**Aesthetic**: Raw terminal, brutalist, utilitarian  
**Goal**: The conversation IS the interface

---

## Visual Design

### Layout
```
┌────────────────────────────────────────┐
│ ← AI response text                     │  ← Chat history
│   → tool_name (tool call)              │     (borderless, full-width)
│   ✓ tool_name (tool result)            │
│   [You] Your prompt here                │
│ > continue typing_                       │  ← Input
└────────────────────────────────────────┘
●thinking model·3 45%                       ← Status line
```

### Key Design Decisions

1. **No Borders, No Chrome**
   - Every pixel serves a purpose
   - Content breathes, doesn't feel constrained
   - Maximum screen real estate for actual work

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

4. **Color Palette**
   ```
   Cyan    #06FFFF  → User input, status
   Green   #00FF00  → Success
   Yellow  #FFFF00  → Tools, warnings
   Red     #FF0000  → Errors
   White   #FFFFFF  → AI text
   DarkGray#444444  → Metadata, muted text
   Black   #000000  → Background
   ```

5. **Raw Aesthetics**
   - No padding except 2-space indentation for AI responses
   - Single-character symbols (→ ← ✓ ✗ ● ○)
   - Punctuation over decoration (· instead of |)
   - Monospace font rhythm

---

## How It Differs

### From Orchestrator TUI

| Feature | Orchestrator | Minimal |
|---------|-------------|---------|
| Sidebar with tasks | ✓ | ✗ removed |
| Multiple panels | 4+ | 1 (content only) |
| Borders | Heavy | None |
| Status bar | Full line with text | 1-line symbols |
| Chat style | Bubbles | Transcript format |
| Scroll indicator | Yes | Implicit position |

### Why This Approach

**Utilitarian**: 
- 100% of screen space for conversation
- No UI chrome to maintain or learn
- Works like a raw terminal

**Brutalist**:
- No decoration for decoration's sake
- Honors the medium (terminal) rather than hiding it
- Functional beauty over polished ugliness

**Bespoke**:
- Designed specifically for agent interaction
- Not a generic chat UI adapted for agents
- Symbols map to agent actions (→ tool calls, ← results)

---

## Implementation

Located in: `src/tui/minimal_tui.rs`

### Core Functions

- `draw_minimal()` — Main render function
- `draw_status_line()` — One-line symbolic status
- `draw_content()` — Chat + input, borderless
- `draw_chat_history()` — Raw transcript rendering
- `draw_input()` — Minimal "> " prompt

### Event Loop

Minimal keyboard handling:
- `Ctrl+C` — quit
- `Enter` — send message
- `Shift+Enter` — newline in input
- `↑/↓` — scroll chat
- `PageUp/PageDown` — scroll by page
- All character keys — normal typing (no shortcuts hijacked)

---

## Usage

Run with:
```bash
./target/release/ferroclaw
```

The minimal TUI is now the default interface. To use the old orchestrator TUI, modify `src/main.rs` line 43 to call:
```rust
ferroclaw::tui::orchestrator_tui::run_orchestrator_tui(agent_loop, &config).await?;
```

---

## Design Principles Applied

1. **Form Follows Function** — Layout shaped by user's goal (converse with AI)
2. **Less is More** — Removed all non-essential UI elements
3. **Typography Over Decoration** — Symbols and spacing create hierarchy, not borders
4. **Raw Over Polished** — Embraces terminal nature instead of hiding it
5. **Bold Choices** — No semi-committal design (full brutalist or nothing)

---

## Future Enhancements

If extending this design:

**Maintain the brutalist philosophy:**
- Add features only if they serve core function
- Prefer symbols over labels
- Keep single-line status minimal
- Never add borders or padding unless critical

**Possible additions:**
- Thread context display (if multi-turn conversations)
- Session memory indicator
- Progress bars for long-running tools (inline, not modal)

**Never add:**
- Decorative borders
- Multiple panels (split views)
- Task lists (use external tools instead)
- Settings panels (use config file)
- Help screens (learn by doing)
