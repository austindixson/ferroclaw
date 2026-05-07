# Thinking Indicator Integration - Subtask 3/6

## Current State

The thinking indicator is **already connected** to the processing state in `src/tui/minimal_tui.rs`:

### 1. State Management in `App`

```rust
pub struct App {
    /// True while the agent is running a turn
    pub is_running: bool,
    /// When the current agent run started (for long-wait nudges)
    pub run_started_at: Option<Instant>,
    /// Short status verb for the orchestrator bar
    pub verb: String,
    // ... other fields
}
```

### 2. Status Line Rendering

```rust
fn draw_status_line(frame: &mut Frame, app: &App, area: Rect) {
    let status_color = if app.is_running {
        Color::Cyan           // Running = Cyan
    } else if app.verb == "error" {
        Color::Red            // Error = Red
    } else {
        Color::Green          // Ready = Green
    };

    let parts: Vec<Span> = vec![
        // Thinking/ready indicator (pulsing ● when running)
        Span::styled(
            if app.is_running { "●" } else { "○" },  // ● when running, ○ when idle
            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
        ),
        // ... rest of status line
    ];
}
```

### 3. State Transitions

#### User Sends Message (Start Processing)
```rust
// In run_loop() when Enter is pressed:
app.verb = get_glitter_verb(true, 0, &[], Some(Instant::now()));
app.is_running = true;
app.run_started_at = Some(Instant::now());
app.iteration = 0;
```

#### Agent Events (Processing Updates)
```rust
// In process_events():
fn process_events(app: &mut App, events: &[AgentEvent]) {
    let mut active_tools: Vec<String> = Vec::new();

    for event in events {
        match event {
            AgentEvent::ToolCallStart { name, .. } => {
                active_tools.push(name.clone());
                app.iteration += 1;
                app.verb = get_glitter_verb(true, app.iteration, &active_tools, app.run_started_at);
            }
            AgentEvent::ToolResult { name, .. } => {
                active_tools.retain(|n| n != name);
                app.verb = get_glitter_verb(true, app.iteration, &active_tools, app.run_started_at);
            }
            AgentEvent::LlmRound { iteration } | AgentEvent::ModelToolChoice { iteration, .. } => {
                app.iteration = *iteration;
                app.verb = get_glitter_verb(true, app.iteration, &active_tools, app.run_started_at);
            }
            AgentEvent::Error(msg) => {
                app.verb = "error".to_string();
            }
            AgentEvent::Done { .. } => {
                // Handled below
            }
        }
    }
}
```

#### Agent Finishes (End Processing)
```rust
// In run_loop() after agent_loop.run() completes:
match agent_loop.run(&input, history).await {
    Ok((response, events)) => {
        process_events(app, &events);
        if !response.is_empty() {
            app.chat_history.push(ChatEntry::TranscriptLine(
                format!("[Ferro] {}", response.replace('\n', "\n     "))
            ));
        }
        app.is_running = false;        // ← Set to false
        app.run_started_at = None;     // ← Clear timestamp
        app.verb = "ready".to_string(); // ← Update verb
    }
    Err(e) => {
        app.chat_history.push(ChatEntry::TranscriptLine(
            format!("[ERROR] {}", e)
        ));
        app.is_running = false;        // ← Set to false on error
        app.verb = "error".to_string(); // ← Set error verb
    }
}
```

## Analysis

### ✅ What Works

1. **Indicator Symbol**: Correctly shows ● when `is_running=true`, ○ when `is_running=false`
2. **Indicator Color**: Cyan (running), Green (ready), Red (error)
3. **State Transitions**: Set to true on message send, cleared on completion/error
4. **Glitter Verbs**: Updated with `get_glitter_verb()` during processing
5. **Error Handling**: Sets verb to "error" and color to Red on errors

### 🔍 What Could Be Enhanced

1. **Error State Indicator**: Currently shows ● with Red when error occurs, but clears to ○ Green when `is_running=false`. Should show ● Red during error state.

2. **Explicit Error State**: The error state is only indicated by `app.verb == "error"`, but the indicator symbol still depends on `app.is_running`. After error, `is_running` is set to false, so indicator becomes ○ Green.

3. **State Consistency**: When error occurs, both the verb and indicator should reflect the error state consistently.

## Proposed Enhancements

### 1. Add Explicit Error State to App

```rust
pub struct App {
    pub is_running: bool,
    pub is_error: bool,      // ← NEW: Track error state
    pub run_started_at: Option<Instant>,
    pub verb: String,
    // ...
}
```

### 2. Update Status Line Rendering

```rust
fn draw_status_line(frame: &mut Frame, app: &App, area: Rect) {
    let status_color = if app.is_error {
        Color::Red            // Error = Red (highest priority)
    } else if app.is_running {
        Color::Cyan           // Running = Cyan
    } else {
        Color::Green          // Ready = Green
    };

    let parts: Vec<Span> = vec![
        // Thinking/ready indicator (pulsing ● when running or error)
        Span::styled(
            if app.is_running || app.is_error { "●" } else { "○" },
            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
        ),
        // ... rest of status line
    ];
}
```

### 3. Update State Transitions

#### On Error
```rust
Err(e) => {
    app.chat_history.push(ChatEntry::TranscriptLine(
        format!("[ERROR] {}", e)
    ));
    app.is_running = false;      // Processing stopped
    app.is_error = true;         // ← NEW: Set error state
    app.verb = "error".to_string();
}
```

#### Clear Error on New Message
```rust
// When user sends a new message:
app.is_running = true;
app.is_error = false;           // ← NEW: Clear error state
app.run_started_at = Some(Instant::now());
```

## Implementation Plan

1. ✅ **Verify current implementation** - Already connected and working
2. ⏭️ **Add `is_error` field** to `App` struct
3. ⏭️ **Update `draw_status_line`** to handle error state in indicator
4. ⏭️ **Update error handling** to set `is_error = true`
5. ⏭️ **Update new message handling** to clear `is_error = false`
6. ⏭️ **Add unit tests** for state transitions
7. ⏭️ **Test with demo** to verify all states

## Testing Strategy

### Test Cases

1. **Initial State**: App starts with `is_running=false`, `is_error=false`, indicator should show ○ Green

2. **User Sends Message**: `is_running=true`, `is_error=false`, indicator should show ● Cyan

3. **Processing Events**: During tool calls and LLM rounds, indicator should remain ● Cyan

4. **Agent Completes**: `is_running=false`, `is_error=false`, indicator should show ○ Green

5. **Agent Errors**: `is_running=false`, `is_error=true`, indicator should show ● Red

6. **New Message After Error**: `is_running=true`, `is_error=false`, indicator should show ● Cyan

### Manual Testing

Run the TUI and observe:
- ● appears when agent is processing
- ○ appears when agent is idle
- ● appears with Red color when error occurs
- ○ appears with Green color after error is cleared

## Conclusion

The thinking indicator is **already connected** to the processing state and works correctly for normal operation. The enhancement will add explicit error state handling to make the error indication more consistent and visible.
