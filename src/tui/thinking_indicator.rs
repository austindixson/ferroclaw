//! Thinking Indicator Component
//!
//! A minimal, brutalist visual indicator showing Ferro's activity state.
//!
//! # Design Philosophy
//!
//! - **No chrome**: No boxes, borders, or decoration
//! - **Symbolic clarity**: Single unicode character conveys state
//! - **Color-driven meaning**: Color conveys state at a glance
//! - **Typography over widgets**: Symbol + bold styling = clear feedback
//!
//! # Visual States
//!
//! | State | Symbol | Color | Meaning |
//! |-------|--------|-------|---------|
//! | Running/Thinking | ● | Cyan (bold) | Agent is processing, LLM or tool active |
//! | Idle/Ready | ○ | Green | Waiting for user input |
//! | Error | ● | Red (bold) | Agent encountered an error |
//!
//! # Position
//!
//! The thinking indicator is positioned at the very start of the status line:
//!
//! ```text
//! ● Contemplating… gpt-4o· 3 45%
//! ↑ thinking indicator ↑ verb  ↑model↑iter↑tokens
//! ```
//!
//! # Integration with Glitter Verbs
//!
//! The thinking indicator works in concert with glitter verbs:
//! - When ● is shown, glitter verbs animate (e.g., "Contemplating…", "Reading…")
//! - When ○ is shown, verb shows "ready"
//! - During long waits, elapsed time appears: "Contemplating… · 3s"
//!
//! # Implementation Details
//!
//! ## Symbol Choice
//!
//! - **● U+25CF BLACK CIRCLE**: Represents "filled/active" state
//! - **○ U+25CB WHITE CIRCLE**: Represents "empty/idle" state
//! - These are widely available unicode characters with good terminal rendering
//!
//! ## Bold Styling
//!
//! The ● indicator uses `Modifier::BOLD` for subtle emphasis:
//! - Makes it visually distinct from regular text
//! - Works well with terminal color schemes
//! - Provides a "pulsing" psychological effect when coupled with animation
//!
//! ## Color Coding
//!
//! - **Cyan**: Active, thinking, processing - conveys "in progress"
//! - **Green**: Ready, safe, waiting - conveys "available"
//! - **Red**: Error, failure - conveys "problem"
//! - **Dark Gray**: Metadata, idle text - recedes into background
//!
//! # Example Code
//!
//! ```rust
//! use ratatui::style::{Color, Modifier, Style};
//! use ratatui::text::Span;
//!
//! // Thinking indicator when agent is running
//! let indicator = Span::styled(
//!     "●",
//!     Style::default()
//!         .fg(Color::Cyan)
//!         .add_modifier(Modifier::BOLD),
//! );
//!
//! // Idle indicator
//! let indicator = Span::styled(
//!     "○",
//!     Style::default()
//!         .fg(Color::Green),
//! );
//! ```
//!
//! # Future Enhancements
//!
//! Possible enhancements while maintaining brutalist principles:
//! - Subtle animation (alternating between ● and ○ at 2Hz when running)
//! - Color transitions during long waits (Cyan → Yellow after 30s)
//! - Symbol variations for different states (e.g., ↻ for tool execution)
//!
//! However, the current implementation adheres to the principle of
//! "minimal UI with maximum information through typography and color."
