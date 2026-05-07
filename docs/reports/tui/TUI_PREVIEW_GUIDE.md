# TUI Preview Guide

This guide helps you preview all the different TUI designs in Ferroclaw across multiple terminal emulators.

## Quick Start

The current active TUI is **Kinetic** (motion-focused). It's running in the terminal tile labeled "Ferroclaw TUI - Terminal 1".

To see the other TUI designs, you have two options:

### Option 1: Use the Switcher Script (Recommended)

```bash
# Make it executable first
chmod +x switch_tui.sh

# Switch to a different TUI
./switch_tui.sh hermes          # Dark & polished
./switch_tui.sh orchestrator    # Real-time transcript
./switch_tui.sh minimal        # Brutalist, no borders
./switch_tui.sh standard       # Classic with borders
./switch_tui.sh kinetic        # Back to default (motion-focused)

# After switching, rebuild
cargo build --release

# Launch the new TUI
./target/release/ferroclaw run
```

### Option 2: Manual Switch

Edit `src/main.rs` line 62, then rebuild:

```rust
// Change this line to the desired TUI:
ferroclaw::tui::kinetic_tui::run_kinetic_tui(agent_loop, &config).await

// Options:
ferroclaw::tui::hermes_tui::run_hermes_tui(agent_loop, &config).await
ferroclaw::tui::orchestrator_tui::run_orchestrator_tui(agent_loop, &config).await
ferroclaw::tui::minimal_tui::run_minimal_tui(agent_loop, &config).await
ferroclaw::tui::run_tui(agent_loop, config).await  // Standard
```

## TUI Design Comparison

| Terminal Tile | TUI Variant | Key Features | Visual Style |
|--------------|-------------|--------------|--------------|
| **Main Terminal** | Kinetic | Glitter verbs, animation, progress bar | Motion-focused, no borders |
| **TUI 2** | Hermes | Message bubbles, task sidebar | Dark, polished chat interface |
| **TUI 3** | Orchestrator | Real-time tool visibility | Transcript-style, teal accents |
| **TUI 4** | Minimal | Brutalist, no chrome | Maximum content area |

## Detailed TUI Descriptions

### 1. Kinetic TUI (Currently Running)

**File:** `src/tui/kinetic_tui.rs`

**Design Philosophy:** "Motion is information - the interface breathes with the agent"

**Unique Features:**
- Animated glitter verbs (thinking, contemplating, mulling, etc.)
- Kinetic status bar with pulse animation
- Progress bar showing token usage
- Glitch aesthetic - raw and alive
- No borders - typography over chrome

**Status Line Example:**
```
● Thinking… model·3 45% ████████░░ 8/s
```

**Chat Display:**
```
→ read_file("src/main.rs")
← ✓ read_file
Ferro: Here's the main.rs content...
```

**Best For:** Users who want visual feedback and animated status updates

---

### 2. Hermes TUI

**File:** `src/tui/hermes_tui.rs`

**Design Philosophy:** Inspired by Hermes agent - polished, familiar chat UI

**Unique Features:**
- Dark theme throughout
- Message bubbles with distinct styling:
  - Assistant: "Ferroclaw:" header (cyan, bold) + indented text
  - User: Orange dot (●) + "You:" + text
- Left sidebar with task management
- Tool calls shown in chat history
- Bottom status bar

**Layout:**
```
┌─────────────────────────┐  ┌─────────┐
│ Ferroclaw: Hello!       │  │ Tasks   │
│   How can I help?       │  │         │
│                         │  │ ● Task1 │
│ ● You: Hi               │  │ ○ Task2 │
│                         │  │         │
│ > type your message_    │  └─────────┘
└─────────────────────────┘
  Status: Ready  gpt-4
```

**Best For:** Users who want a familiar chat interface with task management

---

### 3. Orchestrator TUI

**File:** `src/tui/orchestrator_tui.rs`

**Design Philosophy:** Nyx-inspired transcript - see every tool call in real-time

**Unique Features:**
- Real-time tool line visibility
- Iteration counter showing LLM round
- Verbs for different actions (Reading…, Writing…, Executing…)
- Parallel tool batch notifications
- Long wait nudges (after 30s)
- Dark palette with teal accents

**Transcript Display:**
```
● Thinking… · 3

[Ferro] Welcome to Ferroclaw!
[You] Read the main.rs file

◆ model → glob
→ glob("*.rs")
← glob
→ read_file("src/main.rs")
← read_file

Ferro: Here's the content of main.rs...
```

**Key Symbols:**
- `◆ model → tool_name` - LLM tool choice
- `→ tool_name(args)` - Tool call start
- `← tool_name [OK/ERR]` - Tool result
- `⋯ Parallel tool batch (N calls)` - Batch operations

**Best For:** Debugging and understanding agent decision-making

---

### 4. Minimal TUI

**File:** `src/tui/minimal_tui.rs`

**Design Philosophy:** "No borders, no chrome - maximum screen real estate"

**Unique Features:**
- Zero borders - pure content focus
- Brutalist terminal aesthetic
- Glitter verbs for status
- Status through typography (not UI widgets)
- Maximum usable content area

**Status Line:**
```
● Contemplating… model·3 45%

  Ferro: I can help you
  → read_file
  ← ✓ read_file

> Type your message_
```

**Status Indicators:**
- `●` (filled) = Running or Error state
- `○` (empty) = Ready state
- Colors: Cyan (running), Red (error), Green (ready)

**Best For:** Power users who want maximum content and minimal chrome

---

### 5. Standard TUI (Classic)

**File:** `src/tui/mod.rs` + `ui.rs`

**Design Philosophy:** Classic ratatui TUI with structured layout

**Unique Features:**
- Traditional block borders
- Top banner bar (model, tokens)
- Scrollable chat history
- Multiline input with cursor support
- Status bar with connection info
- Full keyboard shortcut support

**Best For:** Traditional users who like structured layouts

---

## Keyboard Shortcuts (All TUIs)

| Shortcut | Action |
|----------|--------|
| `Ctrl+C` | Quit |
| `Ctrl+L` | Clear chat |
| `PageUp` | Scroll up 10 lines |
| `PageDown` | Scroll down 10 lines |
| `Shift+Up` | Scroll up 1 line |
| `Shift+Down` | Scroll down 1 line |
| `Shift+Enter` | Newline in input |
| `Arrow keys` | Move cursor in input |
| `Home/End` | Jump to start/end of line |
| `Backspace/Delete` | Delete characters |

## Testing Each TUI

### 1. Test Kinetic (Default)
Already running in main terminal. Try:
- Type: "Hello"
- Watch the animated status bar
- Observe glitter verbs changing
- See progress bar animate

### 2. Test Hermes
```bash
./switch_tui.sh hermes
cargo build --release
./target/release/ferroclaw run
```
Look for:
- Dark theme with cyan accents
- Message bubbles with headers
- Task sidebar (add tasks with UI if implemented)

### 3. Test Orchestrator
```bash
./switch_tui.sh orchestrator
cargo build --release
./target/release/ferroclaw run
```
Look for:
- Real-time tool calls as they happen
- Iteration counter
- Verbs changing based on action
- Transcript-style display

### 4. Test Minimal
```bash
./switch_tui.sh minimal
cargo build --release
./target/release/ferroclaw run
```
Look for:
- Zero borders
- Brutalist aesthetic
- Maximum content area
- Glitter verbs in status line

## Performance Comparison

| TUI | Animation | Complexity | Overhead | Screen Usage |
|-----|-----------|------------|----------|--------------|
| Kinetic | High | Medium | Medium | Medium |
| Hermes | Low | High | High | Medium |
| Orchestrator | Medium | Medium | Medium | Medium |
| Minimal | Low | Low | Low | High (content) |
| Standard | None | Low | Low | Low (chrome) |

## Which TUI Should You Choose?

**Choose Kinetic if:** You want visual feedback and enjoy animated interfaces

**Choose Hermes if:** You prefer a familiar chat interface with task management

**Choose Orchestrator if:** You want to debug or understand agent behavior in detail

**Choose Minimal if:** You're a power user who wants maximum content area

**Choose Standard if:** You like traditional, structured interfaces

## Customization Tips

### Modify Glitter Verbs
Edit `src/tui/glitter_verbs.rs` to add or change animated status messages.

### Change Color Schemes
Edit the color definitions in each TUI's draw functions:
- `kinetic_tui.rs`: Search for `Color::`
- `hermes_ui.rs`: Search for `Color::`
- `orchestrator_ui.rs`: Search for `Color::`
- `minimal_tui.rs`: Search for `Color::`

### Adjust Layout
Modify the `Layout` constraints in each TUI's draw function to change proportions.

## Troubleshooting

**TUI not switching:**
- Make sure you edited line 62 of `src/main.rs`
- Rebuild with `cargo build --release`
- Check for compilation errors

**Colors look wrong:**
- Some terminals don't support 24-bit colors
- Try a different terminal emulator (iTerm2, WezTerm, Alacritty)

**Animations too slow/fast:**
- Adjust `tick` rate in `EventHandler::new()` in each TUI
- Default is 100-250ms

## Terminal Emulator Recommendations

For the best TUI experience:

1. **iTerm2** (macOS) - Excellent color support, smooth rendering
2. **WezTerm** (Cross-platform) - Fast, GPU-accelerated
3. **Alacritty** (Cross-platform) - Minimal, fast
4. **Terminal.app** (macOS) - Built-in, decent support
5. **kitty** (Cross-platform) - Fast, supports graphics

## Additional Resources

- **TUI Designs Summary:** `TUI_DESIGNS.md`
- **Source Code:** `src/tui/`
- **Main Entry Point:** `src/main.rs` (line 62)

---

*Last updated: 2025-01-15*
