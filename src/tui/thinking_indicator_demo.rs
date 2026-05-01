//! Interactive demo for the thinking indicator component.
//!
//! This module provides a self-contained demo that showcases:
//! - The three indicator states (Running, Ready, Error)
//! - Visual differences between ● and ○ symbols
//! - Color coding (Cyan, Green, Red)
//! - Bold vs normal styling
//! - Real-time state switching via keyboard
//! - Status line rendering with glitter verbs
//!
//! Run with: cargo run --example thinking_indicator_demo
//!
//! Controls:
//! - R: Set to Running state (● Cyan)
//! - E: Set to Ready state (○ Green)
//! - X: Set to Error state (● Red)
//! - Q: Quit

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::Instant;

#[cfg(feature = "demo")]
use ratatui::backend::CrosstermBackend;
#[cfg(feature = "demo")]
use std::io;

/// Represents the thinking indicator state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorState {
    /// Agent is actively processing
    Running,
    /// Agent is ready for input
    Ready,
    /// Agent encountered an error
    Error,
}

impl IndicatorState {
    /// Get the symbol for this state (● for running/error, ○ for ready)
    pub fn symbol(self) -> &'static str {
        match self {
            IndicatorState::Running => "●",
            IndicatorState::Ready => "○",
            IndicatorState::Error => "●",
        }
    }

    /// Get the color for this state
    pub fn color(self) -> Color {
        match self {
            IndicatorState::Running => Color::Cyan,
            IndicatorState::Ready => Color::Green,
            IndicatorState::Error => Color::Red,
        }
    }

    /// Check if the indicator should be bold (always true for emphasis)
    pub fn is_bold(self) -> bool {
        true
    }

    /// Get the status verb for this state
    pub fn verb(self) -> &'static str {
        match self {
            IndicatorState::Running => "Contemplating…",
            IndicatorState::Ready => "Ready",
            IndicatorState::Error => "Error",
        }
    }
}

/// Create a styled Span for the thinking indicator.
pub fn create_indicator_span(state: IndicatorState) -> Span<'static> {
    let style = Style::default()
        .fg(state.color())
        .add_modifier(Modifier::BOLD);

    Span::styled(state.symbol(), style)
}

/// Demo application state.
pub struct DemoApp {
    /// Current indicator state
    state: IndicatorState,
    /// Model name for display
    model_name: String,
    /// Token budget
    token_budget: u64,
    /// Tokens used
    tokens_used: u64,
    /// Current iteration
    iteration: u32,
    /// When the current run started
    run_started_at: Option<Instant>,
    /// Elapsed time string
    elapsed_str: String,
}

impl DemoApp {
    /// Create a new demo application.
    pub fn new() -> Self {
        Self {
            state: IndicatorState::Ready,
            model_name: "gpt-4o".to_string(),
            token_budget: 100_000,
            tokens_used: 45_000,
            iteration: 0,
            run_started_at: None,
            elapsed_str: String::new(),
        }
    }

    /// Set the indicator state.
    pub fn set_state(&mut self, state: IndicatorState) {
        self.state = state;

        // Update timestamp for running state
        if state == IndicatorState::Running {
            if self.run_started_at.is_none() {
                self.run_started_at = Some(Instant::now());
            }
        } else {
            self.run_started_at = None;
        }
    }

    /// Get the current state.
    pub fn state(&self) -> IndicatorState {
        self.state
    }

    /// Update elapsed time string.
    pub fn update_elapsed(&mut self) {
        if let Some(started) = self.run_started_at {
            let elapsed = started.elapsed();
            let secs = elapsed.as_secs();
            let ms = elapsed.subsec_millis();
            self.elapsed_str = format!("{}.{:03}s", secs, ms);
        } else {
            self.elapsed_str.clear();
        }
    }

    /// Draw the status line with thinking indicator.
    pub fn draw_status_line(&self, frame: &mut Frame, area: Rect) {
        let state = self.state;
        let (status_color, show_filled) = match state {
            IndicatorState::Error => (Color::Red, true),
            IndicatorState::Running => (Color::Cyan, true),
            IndicatorState::Ready => (Color::Green, false),
        };

        // Create bindings for format! strings
        let model_label = format!("{}·", self.model_name);
        let iteration_label = if self.iteration > 0 {
            format!("{} ", self.iteration)
        } else {
            String::new()
        };

        let parts: Vec<Span> = vec![
            // Thinking/ready/error indicator
            Span::styled(
                if show_filled { "●" } else { "○" },
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(
                state.verb(),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(&model_label, Style::default().fg(Color::DarkGray)),
            Span::styled(&iteration_label, Style::default().fg(Color::Yellow)),
            // Tokens
            if self.token_budget > 0 {
                let pct = (self.tokens_used as f64 / self.token_budget as f64 * 100.0) as u64;
                Span::styled(
                    format!("{}%", pct),
                    Style::default().fg(if pct > 80 {
                        Color::Red
                    } else {
                        Color::DarkGray
                    }),
                )
            } else {
                Span::styled("", Style::default())
            },
        ];

        let status_line =
            Paragraph::new(Line::from(parts)).style(Style::default().bg(Color::Reset));
        frame.render_widget(status_line, area);
    }

    /// Draw the help panel.
    pub fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from("Thinking Indicator Demo"),
            Line::from(""),
            Line::from("Controls:"),
            Line::from("  R - Set Running state (● Cyan)"),
            Line::from("  E - Set Ready state (○ Green)"),
            Line::from("  X - Set Error state (● Red)"),
            Line::from("  Q - Quit"),
            Line::from(""),
            Line::from("Current State:"),
            Line::from(format!("  State: {:?} ({})", self.state, self.state.verb())),
            Line::from(format!(
                "  Symbol: {} (color: {:?}, bold: {})",
                self.state.symbol(),
                self.state.color(),
                self.state.is_bold()
            )),
            if !self.elapsed_str.is_empty() {
                Line::from(format!("  Elapsed: {}", self.elapsed_str))
            } else {
                Line::from("  Elapsed: -")
            },
            Line::from(""),
            Line::from("Visual Reference:"),
            Line::from("  ● - Filled circle (running/error)"),
            Line::from("  ○ - Empty circle (ready)"),
            Line::from("  Cyan - Agent processing"),
            Line::from("  Green - Agent ready"),
            Line::from("  Red - Agent error"),
        ];

        let help = Paragraph::new(lines).style(Style::default());
        frame.render_widget(help, area);
    }
}

impl Default for DemoApp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indicator_symbols() {
        assert_eq!(IndicatorState::Running.symbol(), "●");
        assert_eq!(IndicatorState::Ready.symbol(), "○");
        assert_eq!(IndicatorState::Error.symbol(), "●");
    }

    #[test]
    fn test_indicator_colors() {
        assert_eq!(IndicatorState::Running.color(), Color::Cyan);
        assert_eq!(IndicatorState::Ready.color(), Color::Green);
        assert_eq!(IndicatorState::Error.color(), Color::Red);
    }

    #[test]
    fn test_bold_states() {
        assert!(IndicatorState::Running.is_bold());
        assert!(IndicatorState::Ready.is_bold());
        assert!(IndicatorState::Error.is_bold());
    }

    #[test]
    fn test_create_indicator_span() {
        let span = create_indicator_span(IndicatorState::Running);
        assert_eq!(span.content, "●");
        assert_eq!(span.style.fg, Some(Color::Cyan));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_state_transitions() {
        let mut app = DemoApp::new();

        // Start with ready state
        assert_eq!(app.state(), IndicatorState::Ready);

        // Transition to running
        app.set_state(IndicatorState::Running);
        assert_eq!(app.state(), IndicatorState::Running);
        assert!(app.run_started_at.is_some());

        // Transition to error
        app.set_state(IndicatorState::Error);
        assert_eq!(app.state(), IndicatorState::Error);
        assert!(app.run_started_at.is_none()); // Should clear timestamp

        // Transition to ready
        app.set_state(IndicatorState::Ready);
        assert_eq!(app.state(), IndicatorState::Ready);
        assert!(app.run_started_at.is_none());
    }

    #[test]
    fn test_state_verbs() {
        assert_eq!(IndicatorState::Running.verb(), "Contemplating…");
        assert_eq!(IndicatorState::Ready.verb(), "Ready");
        assert_eq!(IndicatorState::Error.verb(), "Error");
    }

    #[test]
    fn test_display_state_transitions() {
        let mut app = DemoApp::new();

        // Ready state
        assert_eq!(app.state.symbol(), "○");
        assert_eq!(app.state.color(), Color::Green);

        // Running state
        app.set_state(IndicatorState::Running);
        assert_eq!(app.state.symbol(), "●");
        assert_eq!(app.state.color(), Color::Cyan);

        // Error state
        app.set_state(IndicatorState::Error);
        assert_eq!(app.state.symbol(), "●");
        assert_eq!(app.state.color(), Color::Red);

        // Back to ready
        app.set_state(IndicatorState::Ready);
        assert_eq!(app.state.symbol(), "○");
        assert_eq!(app.state.color(), Color::Green);
    }
}

/// Main demo entry point.
#[cfg(feature = "demo")]
pub fn run_demo() -> anyhow::Result<()> {
    use crossterm::event::{self, KeyCode};
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut app = DemoApp::new();

    loop {
        terminal.draw(|frame| {
            let size = frame.area();

            // Split into status line (top) and help panel (rest)
            let status_height = 1;
            let status_area = Rect {
                x: 0,
                y: 0,
                width: size.width,
                height: status_height,
            };
            let help_area = Rect {
                x: 0,
                y: status_height,
                width: size.width,
                height: size.height.saturating_sub(status_height),
            };

            app.update_elapsed();
            app.draw_status_line(frame, status_area);
            app.draw_help(frame, help_area);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let event::Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        app.set_state(IndicatorState::Running);
                        app.iteration += 1;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') => {
                        app.set_state(IndicatorState::Ready);
                        app.iteration = 0;
                    }
                    KeyCode::Char('x') | KeyCode::Char('X') => {
                        app.set_state(IndicatorState::Error);
                    }
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
