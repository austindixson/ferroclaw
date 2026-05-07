# TUI Designs - Preview & Launch Guide

Welcome to the Ferroclaw TUI Design Preview! This guide shows you all 5 different Terminal User Interface designs available in Ferroclaw.

## What's Included

I've created multiple resources to help you preview and compare all TUI designs:

### 📄 Documentation Files

1. **TUI_DESIGNS.md** - Comprehensive overview of all 5 TUI designs
   - Design philosophy for each TUI
   - Feature breakdowns
   - How to launch each design

2. **TUI_COMPARISON.md** - Side-by-side comparison
   - Visual layouts (ASCII art)
   - Color schemes
   - Feature comparison tables
   - Performance metrics
   - Decision tree for choosing the right TUI

3. **TUI_PREVIEW_GUIDE.md** - Step-by-step preview guide
   - How to switch between TUIs
   - Testing instructions for each design
   - Troubleshooting tips
   - Terminal emulator recommendations

### 🖥️ Terminal Tiles (Canvas)

I've created 4 terminal tiles on the canvas, each showing information about a different TUI:

- **TUI 1: Kinetic (Motion-focused)** - Currently running in main terminal
- **TUI 2: Hermes (Dark & Polished)** - Dark theme with task sidebar
- **TUI 3: Orchestrator (Real-time Transcript)** - Shows every tool call
- **TUI 4: Minimal (Brutalist)** - No borders, maximum content

### 🔧 Utility Scripts

1. **switch_tui.sh** - Quick TUI switcher
   ```bash
   ./switch_tui.sh [kinetic|hermes|orchestrator|minimal|standard]
   ```

2. **demo_all_tuis.sh** - Interactive demo menu
   ```bash
   ./demo_all_tuis.sh
   ```
   Shows all TUIs with descriptions and examples

## Quick Start

### Option 1: View the Comparison (Fastest)

Just read the documentation files to see side-by-side comparisons:

```bash
# Read the comparison (includes visual layouts)
cat TUI_COMPARISON.md

# Or read the comprehensive design overview
cat TUI_DESIGNS.md
```

### Option 2: Use the Demo Script (Interactive)

```bash
# Make it executable (first time only)
chmod +x demo_all_tuis.sh

# Run the interactive demo menu
./demo_all_tuis.sh
```

The demo will show you:
- Description of each TUI
- Key features
- Option to launch each one with example prompts

### Option 3: Switch and Launch TUIs

```bash
# Make switcher executable (first time only)
chmod +x switch_tui.sh

# Switch to a TUI
./switch_tui.sh hermes          # Dark & polished
./switch_tui.sh orchestrator    # Real-time transcript
./switch_tui.sh minimal        # Brutalist, no borders
./switch_tui.sh standard       # Classic with borders
./switch_tui.sh kinetic        # Back to default

# Rebuild
cargo build --release

# Launch
./target/release/ferroclaw run
```

### Option 4: View Terminal Info Tiles

On the canvas, you'll see 4 terminal tiles showing:
- Design philosophy
- How to launch each TUI
- Key visual elements

## The 5 TUI Designs

### 1. Kinetic TUI ⚡ (Currently Active)

**File:** `src/tui/kinetic_tui.rs`

**Tagline:** "Motion is information - the interface breathes with the agent"

**Key Features:**
- Animated glitter verbs (thinking, contemplating, mulling, etc.)
- Kinetic status bar with pulse animation
- Progress bar showing token usage
- Glitch aesthetic - raw and alive
- No borders - typography over chrome

**Best For:** Users who want visual feedback and enjoy animated interfaces

**Visual:**
```
● Thinking… model·3 45% ████████░░ 8/s
→ read_file
← ✓ read_file
```

---

### 2. Hermes TUI 💬

**File:** `src/tui/hermes_tui.rs`

**Tagline:** "Dark theme with polished chat interface"

**Key Features:**
- Dark theme throughout
- Message bubbles with distinct styling
- Left sidebar with task management
- Tool calls shown in chat history
- Bottom status bar

**Best For:** Users who want a familiar chat interface with task management

**Visual:**
```
┌─────────────────────┐  ┌─────────┐
│ Ferroclaw: Hello!    │  │ Tasks   │
│ ● You: Hi           │  │ ● Task1 │
└─────────────────────┘  └─────────┘
```

---

### 3. Orchestrator TUI 📊

**File:** `src/tui/orchestrator_tui.rs`

**Tagline:** "Real-time transcript - see every tool call"

**Key Features:**
- Real-time tool line visibility
- Iteration counter showing LLM round
- Verbs for different actions (Reading…, Writing…, Executing…)
- Parallel tool batch notifications
- Long wait nudges (after 30s)

**Best For:** Debugging and understanding agent decision-making

**Visual:**
```
◆ model → glob
→ glob("*.rs")
← glob
→ read_file("src/main.rs")
← read_file
```

---

### 4. Minimal TUI 🎯

**File:** `src/tui/minimal_tui.rs`

**Tagline:** "No borders, no chrome - maximum content"

**Key Features:**
- Zero borders - pure content focus
- Brutalist terminal aesthetic
- Glitter verbs for status
- Status through typography
- Maximum usable content area (95%)

**Best For:** Power users who want maximum content and minimal chrome

**Visual:**
```
● Contemplating… model·3 45%

  Ferro: I can help you
  → read_file
  ← ✓ read_file

> Type your message_
```

---

### 5. Standard TUI 📋

**File:** `src/tui/mod.rs` + `ui.rs`

**Tagline:** "Classic ratatui TUI with structured layout"

**Key Features:**
- Traditional block borders
- Top banner bar (model, tokens)
- Scrollable chat history
- Multiline input with cursor support
- Status bar with connection info

**Best For:** Traditional users who like structured layouts

**Visual:**
```
┌─────────────────────────────────┐
│ Ferroclaw v0.1.0  gpt-4         │
├─────────────────────────────────┤
│ Ferroclaw: Welcome!            │
│ > Hello                         │
├─────────────────────────────────┤
│ > Type your message...         │
├─────────────────────────────────┤
│ Status: Ready                   │
└─────────────────────────────────┘
```

## Comparison at a Glance

| TUI | Borders | Animation | Task Sidebar | Screen Space | Best For |
|-----|---------|-----------|--------------|--------------|----------|
| **Kinetic** | Minimal | Yes (glitter verbs) | No | 85% | Visual feedback |
| **Hermes** | Full | No | Yes | 75% | Chat + tasks |
| **Orchestrator** | Full | Real-time events | No | 80% | Debugging |
| **Minimal** | None | Glitter verbs | No | 95% | Power users |
| **Standard** | Full | No | No | 70% | Traditional |

## Decision Tree

```
Do you want animated feedback?
├─ Yes → Kinetic TUI
└─ No
    └─ Do you want task management?
        ├─ Yes → Hermes TUI
        └─ No
            └─ Do you want real-time tool visibility?
                ├─ Yes → Orchestrator TUI
                └─ No
                    └─ Do you prefer minimal chrome?
                        ├─ Yes → Minimal TUI
                        └─ No → Standard TUI
```

## File Reference

- **TUI source code:** `src/tui/`
- **Main entry point:** `src/main.rs` (line 62)
- **Switcher script:** `switch_tui.sh`
- **Demo script:** `demo_all_tuis.sh`

## Resources Created

1. **TUI_DESIGNS.md** - Full design documentation
2. **TUI_COMPARISON.md** - Side-by-side comparisons
3. **TUI_PREVIEW_GUIDE.md** - Step-by-step preview guide
4. **switch_tui.sh** - Quick TUI switcher
5. **demo_all_tuis.sh** - Interactive demo menu
6. **4 Terminal tiles** on canvas showing each TUI's info

## Next Steps

1. **Read the docs:** Start with `TUI_COMPARISON.md` for quick overview
2. **Run the demo:** `./demo_all_tuis.sh` for interactive showcase
3. **Try them out:** Use `switch_tui.sh` to switch between designs
4. **Choose your favorite:** Based on your workflow and preferences

## Troubleshooting

**Binary not found:**
```bash
cargo build --release
```

**Permission denied on scripts:**
```bash
chmod +x switch_tui.sh demo_all_tuis.sh
```

**TUI not switching:**
1. Check you edited `src/main.rs` line 62
2. Rebuild with `cargo build --release`
3. Check for compilation errors

**Colors look wrong:**
- Try a better terminal (iTerm2, WezTerm, Alacritty)
- Some terminals don't support 24-bit colors

## Keyboard Shortcuts (All TUIs)

- `Ctrl+C` - Quit
- `Ctrl+L` - Clear chat
- `PageUp/PageDown` - Scroll 10 lines
- `Shift+Up/Down` - Scroll 1 line (most TUIs)
- `Shift+Enter` - Newline in input
- Arrow keys - Navigate input

## Enjoy! 🎉

Explore all 5 TUI designs and find the one that fits your style. Each design has a unique philosophy and approach to terminal-based interaction.

---

*Need help? Check the detailed guides or run the demo script!*
