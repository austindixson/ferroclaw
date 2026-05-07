# TUI Visual Comparison

This document provides side-by-side visual comparisons of all Ferroclaw TUI designs.

## Quick Reference Table

| TUI | Borders | Status Bar | Animation | Task Sidebar | Best Use Case |
|-----|---------|------------|-----------|--------------|---------------|
| **Kinetic** | Minimal | Top (kinetic) | Yes (glitter verbs) | No | Visual feedback |
| **Hermes** | Yes | Bottom | No | Yes | Chat with tasks |
| **Orchestrator** | Yes | Top (verb) | Real-time events | No | Debugging flow |
| **Minimal** | No | Top (minimal) | Glitter verbs | No | Power users |
| **Standard** | Yes | Top | No | No | Traditional users |

## Visual Layouts

### Kinetic TUI
```
● Thinking… model·3 45% ████████░░ 8/s  ← Kinetic status (pulse+progress)
                                         ┌─────────────────────────┐
  Ferro: Response here                  │ ┌─── Chat ─────────────┐ │
  → tool_call                           │ │ Ferro: Response here │ │
  ← ✓ result                            │ │   Tool calls appear  │ │
                                         │ │   as they happen    │ │
                                         │ └─────────────────────┘ │
> your input_                           └─────────────────────────┘
                                         ┌─────────────────────────┐
                                         │ Glitter verb: mulling…  │  ← Animated verb
                                         └─────────────────────────┘
                                         ┌─────────────────────────┐
                                         │ > Type your message...  │  ← Input with border
                                         └─────────────────────────┘
```

### Hermes TUI
```
┌─────────────────────────┐  ┌─────────┐
│ Ferroclaw: Hello!        │  │ Tasks   │  ← Task sidebar
│   How can I help?        │  │         │
│                         │  │ ● Task1 │
│ ● You: Hi               │  │ ○ Task2 │  ← Tasks with status
│                         │  │         │
│ > type message_         │  └─────────┘
└─────────────────────────┘
  Status: Ready  gpt-4       ← Bottom status bar
```

### Orchestrator TUI
```
● Thinking… · 3

[Ferro] Welcome to Ferroclaw!           ← Chat messages
[You] Read the main.rs file

◆ model → glob                         ← Tool choice
→ glob("*.rs")                          ← Tool call start
← glob                                  ← Tool result
→ read_file("src/main.rs")
← read_file

Ferro: Here's the content...           ← Final response
```

### Minimal TUI
```
● Contemplating… model·3 45%           ← Minimal status line

  Ferro: I can help you build a TUI    ← Indented responses
  → read_file                           → Tool call
  ← ✓ read_file                        ← Tool result

> Build a TUI interface_                ← Minimal prompt
```

### Standard TUI
```
┌─────────────────────────────────────┐
│ Ferroclaw v0.1.0  gpt-4  12k/100k   │  ← Top banner
├─────────────────────────────────────┤
│                                     │
│ Ferroclaw: Welcome!                 │  ← Chat with borders
│ > Hello                             │
│ Ferroclaw: Hi! How can I help?      │
│                                     │
├─────────────────────────────────────┤
│ > Type your message...              │  ← Input with border
├─────────────────────────────────────┤
│ Status: Ready  Tokens: 12k/100k     │  ← Status bar
└─────────────────────────────────────┘
```

## Color Schemes

### Kinetic TUI Colors
```
Cyan     - Status, tool calls, thinking indicator
Yellow   - Iteration numbers, some accents
Green    - Ready state, successful tool results
Red      - Error state, failed tool results
DarkGray - Secondary text, inactive elements
White    - Primary text
```

### Hermes TUI Colors
```
Cyan     - Assistant header ("Ferroclaw:")
Orange   - User indicator (●)
Green    - Successful tool results
Red      - Errors, failed tool results
White    - Primary text
DarkGray - Secondary text, borders
```

### Orchestrator TUI Colors
```
Teal     - Accents, thinking indicator
Yellow   - Iteration counters
Green    - Successful tool results
Red      - Errors, failed tool results
Cyan     - Model/tool choice indicators
White    - Primary text
DarkGray - Secondary text
```

### Minimal TUI Colors
```
Cyan     - Thinking state, prompt markers
Green    - Ready state
Red      - Error state
Yellow   - Iteration numbers
DarkGray - Tool results, secondary text
White    - Primary text (Ferro responses)
```

### Standard TUI Colors
```
Blue     - System messages
Cyan     - Some accents
Green    - Ready state
Red      - Errors
Yellow   - Warnings
White    - Primary text
DarkGray - Borders, secondary text
```

## Animation Levels

| TUI | Animation Type | Frequency | Purpose |
|-----|----------------|-----------|---------|
| **Kinetic** | Glitter verbs | Every tick (100ms) | Show agent thinking |
| **Kinetic** | Progress bar | Token updates | Show token usage |
| **Hermes** | None | - | Static interface |
| **Orchestrator** | Real-time events | As they happen | Show tool execution |
| **Minimal** | Glitter verbs | Status changes | Minimal feedback |
| **Standard** | None | - | Static interface |

## Screen Space Utilization

### Content Area Percentage (approximate)

| TUI | Content Area | Chrome (borders/status) | Total |
|-----|--------------|-------------------------|-------|
| **Kinetic** | 85% | 15% | 100% |
| **Hermes** | 75% | 25% (including sidebar) | 100% |
| **Orchestrator** | 80% | 20% | 100% |
| **Minimal** | 95% | 5% | 100% |
| **Standard** | 70% | 30% | 100% |

## Message Formatting Comparison

### Assistant Messages

| TUI | Format | Example |
|-----|--------|---------|
| Kinetic | `[Ferro] text` | `[Ferro] Here's the code...` |
| Hermes | `Ferroclaw:` (bold) | `Ferroclaw: Here's the code...` |
| Orchestrator | `Ferro: text` | `Ferro: Here's the code...` |
| Minimal | `Ferro: text` (indented) | `  Ferro: Here's the code...` |
| Standard | `Ferroclaw: text` | `Ferroclaw: Here's the code...` |

### User Messages

| TUI | Format | Example |
|-----|--------|---------|
| Kinetic | `[You] text` | `[You] Read main.rs` |
| Hermes | `● You:` (orange dot) | `● You: Read main.rs` |
| Orchestrator | `[You] text` | `[You] Read main.rs` |
| Minimal | `> text` (no prefix) | `> Read main.rs` |
| Standard | `> text` | `> Read main.rs` |

### Tool Calls

| TUI | Format | Example |
|-----|--------|---------|
| Kinetic | `→ tool_name` | `→ read_file` |
| Hermes | `→ tool_name` | `→ read_file` |
| Orchestrator | `→ tool_name(args)` | `→ read_file("src/main.rs")` |
| Minimal | `→ tool_name` | `→ read_file` |
| Standard | `→ tool_name` | `→ read_file` |

### Tool Results

| TUI | Format | Example |
|-----|--------|---------|
| Kinetic | `← ✓ tool_name` / `← ✗ tool_name [ERR]` | `← ✓ read_file` |
| Hermes | `← tool_name` (green/red) | `← read_file` |
| Orchestrator | `← tool_name` / `← tool_name [ERR]` | `← read_file` |
| Minimal | `← ✓ tool_name` / `← ✗ tool_name` | `← ✓ read_file` |
| Standard | `← tool_name` | `← read_file` |

## Keyboard Shortcut Support

| Shortcut | Kinetic | Hermes | Orchestrator | Minimal | Standard |
|----------|---------|--------|--------------|---------|----------|
| `Ctrl+C` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `Ctrl+L` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `PageUp/Down` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `Shift+Up/Down` | ✅ | ✅ | ✅ | ❌ | ✅ |
| `Shift+Enter` | ✅ | ✅ | ✅ | ✅ | ✅ |
| Arrow keys | ✅ | ✅ | ✅ | ✅ | ✅ |
| Home/End | ❌ | ✅ | ✅ | ✅ | ✅ |
| Tab | ❌ | ✅ | ✅ | ❌ | ✅ |
| Backspace/Delete | ✅ | ✅ | ✅ | ✅ | ✅ |

## Feature Checklist

| Feature | Kinetic | Hermes | Orchestrator | Minimal | Standard |
|---------|---------|--------|--------------|---------|----------|
| Chat history | ✅ | ✅ | ✅ | ✅ | ✅ |
| Scrollable | ✅ | ✅ | ✅ | ✅ | ✅ |
| Multiline input | ✅ | ✅ | ✅ | ✅ | ✅ |
| Cursor movement | ✅ | ✅ | ✅ | ✅ | ✅ |
| Tool call display | ✅ | ✅ | ✅ | ✅ | ✅ |
| Tool result display | ✅ | ✅ | ✅ | ✅ | ✅ |
| Token tracking | ✅ | ✅ | ✅ | ✅ | ✅ |
| Model display | ✅ | ✅ | ✅ | ✅ | ✅ |
| Status bar | ✅ | ✅ | ✅ | ✅ | ✅ |
| Animation | ✅ | ❌ | ✅ (events) | ✅ | ❌ |
| Task sidebar | ❌ | ✅ | ❌ | ❌ | ❌ |
| Glitter verbs | ✅ | ❌ | ❌ | ✅ | ❌ |
| Progress bar | ✅ | ❌ | ❌ | ❌ | ❌ |
| Iteration counter | ✅ | ❌ | ✅ | ✅ | ❌ |
| Real-time events | ❌ | ❌ | ✅ | ❌ | ❌ |
| Borders | Partial | Full | Full | None | Full |

## Performance Metrics

| Metric | Kinetic | Hermes | Orchestrator | Minimal | Standard |
|--------|---------|--------|--------------|---------|----------|
| CPU idle | <1% | <1% | <1% | <1% | <1% |
| CPU active | 2-5% | 1-2% | 2-4% | 1-2% | <1% |
| Memory | ~15MB | ~20MB | ~18MB | ~12MB | ~15MB |
| Tick rate | 100ms | 250ms | 250ms | 250ms | 250ms |
| Draw calls | 10/s | 4/s | 5/s | 4/s | 4/s |

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

## User Persona Mapping

| Persona | Preferred TUI | Why |
|---------|---------------|-----|
| **Visual Thinker** | Kinetic | Animated feedback helps thinking process |
| **Chat Enthusiast** | Hermes | Familiar chat UI with modern design |
| **Debugger** | Orchestrator | See every tool call in real-time |
| **Power User** | Minimal | Maximum content, no distractions |
| **Traditionalist** | Standard | Classic, structured layout |
| **Task Manager** | Hermes | Built-in task sidebar |
| **Developer** | Orchestrator | Transparency into agent decisions |

## Summary

- **Most Animated:** Kinetic
- **Most Polished:** Hermes
- **Most Transparent:** Orchestrator
- **Most Minimal:** Minimal
- **Most Traditional:** Standard

Choose based on your workflow and preferences!

---

*Last updated: 2025-01-15*
