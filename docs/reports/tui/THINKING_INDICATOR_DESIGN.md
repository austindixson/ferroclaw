# Thinking Indicator Component - Design Document

## Overview

The **Thinking Indicator** is a minimal, brutalist UI component that provides visual feedback about Ferro's activity state in the ferroclaw TUI. It consists of a single Unicode character that changes based on the agent's state, combined with color coding for quick visual recognition.

## Design Philosophy

The thinking indicator adheres to ferroclaw's core design principles:

1. **Zero Chrome**: No boxes, borders, padding, or decoration
2. **Maximum Information, Minimal Pixels**: One character conveys state
3. **Color-Driven Meaning**: Color provides immediate semantic information
4. **Typography Over Widgets**: Symbol + bold styling = clear feedback
5. **Brutalist Aesthetic**: Raw, functional, intentionally stark

## Visual States

| State | Symbol | Color | Bold | Meaning |
|-------|--------|-------|------|---------|
| **Running** | ● (U+25CF) | Cyan | Yes | Agent is processing, LLM or tool active |
| **Ready** | ○ (U+25CB) | Green | No | Waiting for user input |
| **Error** | ● (U+25CF) | Red | Yes | Agent encountered an error |

## Layout & Positioning

The thinking indicator is positioned at the very start of the status line, immediately visible to the user:

```
● Contemplating… gpt-4o· 3 45%
↑ thinking indicator ↑ verb  ↑model↑iter↑tokens
```

**Status Line Structure:**
1. Thinking indicator (● or ○)
2. Space separator
3. Glitter verb (animated status message)
4. Space separator
5. Model name (with · delimiter)
6. Iteration count
7. Token usage percentage

## Implementation Details

### Symbol Choice

The Unicode characters were selected for their:

- **Universal Availability**: Work across all modern terminals
- **Clear Visual Distinction**: Filled vs empty circle is unmistakable
- **Render Quality**: Display consistently at all font sizes
- **Minimal Width**: Single character preserves screen real estate

```
● U+25CF BLACK CIRCLE  → "filled/active" state
○ U+25CB WHITE CIRCLE  → "empty/idle" state
```

### Color Coding

Colors follow semantic conventions familiar to developers:

- **Cyan**: Processing, in-progress, computing (conveys "active thinking")
- **Green**: Ready, safe, success (conveys "available")
- **Red**: Error, failure, problem (conveys "attention needed")
- **Dark Gray**: Metadata, idle text (recedes into background)

### Bold Styling

The bold modifier is applied strategically:

- **Running/Error states**: Bold creates emphasis and draws attention
- **Ready state**: Not bold, maintaining subtle presence
- **Pulsing effect**: Combined with glitter verbs, creates psychological "animation"

## Integration with Glitter Verbs

The thinking indicator works in concert with the glitter verbs system:

### Normal Operation
```
● Contemplating… gpt-4o· 3 45%
```
- Active indicator (●) with cyan
- Glitter verb animates every 2.2 seconds
- Shows agent is actively processing

### Idle State
```
○ ready gpt-4o· 3 45%
```
- Empty indicator (○) with green
- Static verb "ready"
- Agent waiting for user input

### During Tool Execution
```
● Reading… gpt-4o· 4 45%
```
- Active indicator (●) with cyan
- Tool-specific verb ("Reading…", "Writing…", etc.)
- Shows specific activity type

### Long Wait with Elapsed Time
```
● Contemplating… · 7s gpt-4o· 5 45%
```
- Active indicator remains
- Verb shows elapsed time
- Helpful context for slow operations

### Error State
```
● error gpt-4o· 3 45%
```
- Bold indicator with red
- Static verb "error"
- Immediate visual feedback of problem

## Code Structure

### Module Files

```
src/tui/
├── thinking_indicator.rs       # Documentation and design specs
├── thinking_indicator_demo.rs  # Demo and tests
├── minimal_tui.rs              # Integration in main TUI
└── mod.rs                      # Module declarations
```

### Key Functions

```rust
// Create styled span for indicator
pub fn create_indicator_span(state: IndicatorState) -> Span

// Get symbol for state
pub fn symbol(&self) -> &str

// Get color for state
pub fn color(&self) -> Color

// Check if bold styling
pub fn is_bold(&self) -> bool
```

## Testing

The component includes comprehensive tests:

- **Symbol validation**: Ensures correct Unicode characters
- **Color mapping**: Verifies semantic color assignments
- **Bold styling**: Confirms visual emphasis rules
- **Span creation**: Tests styled span generation
- **Demo integration**: Interactive testing harness

Run tests:
```bash
cargo test thinking_indicator
```

Run demo:
```bash
cargo run --example thinking_indicator_demo
```

## Usage Example

```rust
use ratatui::style::{Color, Modifier, Style};

// In status line rendering
let indicator_span = if app.is_running {
    Span::styled(
        "●",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
} else {
    Span::styled(
        "○",
        Style::default()
            .fg(Color::Green),
    )
};

// Build status line
let status_line = vec![
    indicator_span,
    Span::raw(" "),
    Span::styled(&app.verb, Style::default().fg(status_color)),
    // ... more status elements
];
```

## Future Enhancements

While maintaining brutalist principles, possible enhancements:

1. **Subtle Animation**
   - Alternate between ● and ○ at 2Hz when running
   - Creates pulsing effect without chrome

2. **Color Transitions**
   - Cyan → Yellow after 30 seconds of processing
   - Visual cue for long-running operations

3. **State-Specific Symbols**
   - Use ↻ (clockwise arrow) for tool execution
   - Use ⏳ (hourglass) for waiting state

4. **Accessibility**
   - Terminal bell on state changes (optional)
   - Screen reader announcements for accessibility

5. **Performance Metrics**
   - Change color based on token usage thresholds
   - Yellow at 80%, red at 95%

However, any enhancements must adhere to:
- Zero additional chrome
- Single-character indicator
- Minimal visual noise
- Consistent brutalist aesthetic

## Design Trade-offs

### Chosen: Single Symbol vs Multi-Symbol
**Decision**: Single symbol (●/○)
**Rationale**: Minimal visual footprint, instant recognition
**Trade-off**: Less granular state information

### Chosen: Color + Bold vs Animation
**Decision**: Color + bold only
**Rationale**: Works on all terminals, no performance cost
**Trade-off**: No actual animation, relies on glitter verbs for motion

### Chosen: Position at Start vs End
**Decision**: At start of status line
**Rationale**: First thing user sees, immediate feedback
**Trade-off**: May obscure when typing (mitigated by height)

## Accessibility Considerations

- **High Contrast**: Cyan/Green/Red on dark backgrounds
- **Symbol Distinction**: Filled vs empty circle works with color blindness
- **Screen Readers**: Single character is announced clearly
- **No Flashing**: No strobing or rapid color changes
- **Font Independence**: Unicode characters work across fonts

## Performance Impact

- **Zero Overhead**: Single Unicode character render
- **No Computation**: State is boolean + enum
- **No Memory**: No additional data structures
- **Fast**: Render time is negligible (< 1μs)

## Conclusion

The thinking indicator achieves its design goals:

✓ **Minimal**: Single character, zero chrome
✓ **Clear**: State instantly recognizable
✓ **Brutalist**: Raw, functional aesthetic
✓ **Integrated**: Works seamlessly with glitter verbs
✓ **Performant**: No computational overhead
✓ **Accessible**: High contrast, color-blind friendly

It embodies the ferroclaw philosophy of "maximum information, minimum pixels" while providing users with immediate, at-a-glance feedback about Ferro's activity state.
