# Kinetic TUI — V3 Design

## Philosophy

**The interface breathes with the agent.**

- **Motion is information** — Status changes are visible through animation
- **Density encodes state** — Token velocity, iteration depth, memory pressure at a glance
- **Glitch aesthetic** — Raw, alive, not polished
- **No borders** — Typography over chrome

---

## Visual Design

### Layout

```
┌────────────────────────────────────────┐
│ ● Thinking…  model·3 45% ████████░░ 8/s │  ← Kinetic status (2 lines)
│ iteration 7 · 12s                       │    (pulsing symbol + progress bar)
│                                         │
│  Ferro: Let me analyze...              │  ← Chat history
│  → grep "pattern" src/**/*.rs          │     (glitch effects on tool calls)
│  ← ✓ 23 matches                        │
│                                         │
│ > your input_                          │  ← Input (minimal prompt)
└────────────────────────────────────────┘
```

### Key V3 Features

#### 1. Two-Line Kinetic Status
- **Line 1**: Symbol + verb + model + iteration + token % + **progress bar** + **velocity**
- **Line 2**: Iteration count + elapsed time (when running)

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

---

## Comparison: V2 vs V3

| Feature | V2 Minimal | V3 Kinetic |
|---------|-----------|------------|
| Status height | 1 line | 2 lines |
| Progress | Text percentage ("45%") | Visual bar (████████░░) |
| Velocity | Hidden | Shown ("8.2/s") |
| Animation | None planned | Glitch effects, future pulses |
| Info density | Low | High |
| Timing | Hidden | Line 2 shows elapsed |

---

## Color Scheme

```rust
Cyan    #06FFFF  → Running state, progress bar
Green   #00FF00  → Ready state
Yellow  #FFFF00  → Iteration count, velocity
Red     #FF0000  → Error state
White   #FFFFFF  → AI responses
DarkGray#444444  → Metadata, model name
```

---

## Technical Details

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

## Usage

Run with kinetic TUI (add to `src/main.rs`):

```rust
async fn run_kinetic_tui(config: Config) -> anyhow::Result<()> {
    let (agent_loop, _audit) = build_agent(config.clone()).await?;
    ferroclaw::tui::kinetic_tui::run_kinetic_tui(agent_loop, &config).await
}
```

Or via CLI:
```bash
./target/release/ferroclaw run
```

---

## Future Enhancements

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

**Never**:
- Decorative borders
- Multiple panels
- Task list UI
- Settings screens

---

## Design Principles

1. **Information First** — Every pixel conveys state
2. **Motion = Life** — Static feels dead, kinetic feels alive
3. **Density Over Whitespace** — Maximize information per line
4. **Symbol Over Label** — Progress bar > "45% used"
5. **Glitch Aesthetic** — Embrace raw terminal, don't hide it

---

**Status**: ✅ Implemented
**Build**: Clean (0 errors, 9 warnings - unused constants)
**Ready**: Production testing needed

---

**Built**: 2026-04-13
**Designer**: Mei (hotAsianIntern - Dev track)
