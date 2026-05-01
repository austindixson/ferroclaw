//! Minimal, bespoke, utilitarian TUI for ferroclaw.
//!
//! Design principles:
//! - No borders, no padding, no chrome
//! - Maximum screen real estate for content
//! - Raw, brutalist terminal aesthetic
//! - Status through typography and position, not UI widgets
//! - Glitter verbs for animated status messages
//!
//! Layout:
//! ┌────────────────────────────────────────┐
//! │ ← Ferro response                       │  ← Output area (auto-scrolling)
//! │   tool results                         │
//! │   more content...                      │
//! │                                        │
//! │ [You] I want to build a TUI_          │  ← Input prompt
//! └────────────────────────────────────────┘
//! ↑ status line: ●thinking model·3·45%    ← Single-line status

use crate::agent::r#loop::AgentEvent;
use crate::agent::r#loop::AgentLoop;
use crate::config::Config;
use crate::types::Message;

use super::app::{App, ChatEntry};
use super::events::{Event, EventHandler};
use super::glitter_verbs::get_glitter_verb;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use std::io;
use std::time::Instant;

/// Run the minimal TUI.
pub async fn run_minimal_tui(mut agent_loop: AgentLoop, config: &Config) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let model_name = config.agent.default_model.clone();
    let token_budget = config.agent.token_budget;

    let mut app = App::new(model_name.clone(), token_budget);

    // Welcome message
    app.chat_history.push(ChatEntry::TranscriptLine(format!(
        "ferroclaw v{} — {} — ",
        env!("CARGO_PKG_VERSION"),
        model_name
    )));
    app.chat_history.push(ChatEntry::TranscriptLine(
        "Type a message and press Enter to send. Ctrl+C to quit.".to_string(),
    ));
    app.chat_history
        .push(ChatEntry::TranscriptLine(String::new()));

    let event_handler = EventHandler::new(250);
    let mut history: Vec<Message> = Vec::new();

    // Main loop
    let result = run_loop(
        &mut terminal,
        &mut app,
        &event_handler,
        &mut agent_loop,
        &mut history,
    )
    .await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Main event loop.
async fn run_loop(
    terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_handler: &EventHandler,
    agent_loop: &mut AgentLoop,
    history: &mut Vec<Message>,
) -> anyhow::Result<()> {
    loop {
        // Draw
        terminal.draw(|frame| draw_minimal(frame, app))?;

        match event_handler.next()? {
            Event::Tick => {
                // Timeout-based redraw for agent activity
                if app.is_running || app.is_error {
                    maybe_nudge_if_slow(app);
                    terminal.draw(|frame| draw_minimal(frame, app))?;
                }
            }

            Event::MouseScrollUp => {
                app.scroll_up(3);
                continue;
            }

            Event::MouseScrollDown => {
                app.scroll_down(3);
                continue;
            }

            Event::Key(key_event) => {
                use crossterm::event::{KeyCode, KeyModifiers};

                let code = key_event.code;
                let modifiers = key_event.modifiers;

                // Quit
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    return Ok(());
                }

                // Scroll (no modifiers on arrow keys)
                if code == KeyCode::Up && modifiers.is_empty() {
                    app.scroll_up(3);
                    continue;
                }
                if code == KeyCode::Down && modifiers.is_empty() {
                    app.scroll_down(3);
                    continue;
                }
                if code == KeyCode::PageUp {
                    app.scroll_up(20);
                    continue;
                }
                if code == KeyCode::PageDown {
                    app.scroll_down(20);
                    continue;
                }

                // Send message
                if code == KeyCode::Enter && !modifiers.contains(KeyModifiers::SHIFT) {
                    let input = app.take_input();
                    if input.is_empty() {
                        continue;
                    }

                    // Clear error state and start running
                    app.is_error = false;
                    app.verb = get_glitter_verb(true, 0, &[], Some(Instant::now()));
                    app.is_running = true;
                    app.run_started_at = Some(Instant::now());
                    app.iteration = 0;

                    // User message
                    app.chat_history
                        .push(ChatEntry::TranscriptLine(format!("[You] {}", input)));
                    app.scroll_to_bottom();

                    terminal.draw(|frame| draw_minimal(frame, app))?;

                    // Run agent
                    match agent_loop.run(&input, history).await {
                        Ok((outcome, events)) => {
                            process_events(app, &events);
                            if !outcome.text.is_empty() {
                                app.chat_history.push(ChatEntry::TranscriptLine(format!(
                                    "[Ferro] {}",
                                    outcome.text.replace('\n', "\n     ")
                                )));
                            }
                            app.is_running = false;
                            app.run_started_at = None;
                            app.verb = "ready".to_string();
                        }
                        Err(e) => {
                            app.chat_history
                                .push(ChatEntry::TranscriptLine(format!("[ERROR] {}", e)));
                            app.is_running = false;
                            app.is_error = true;
                            app.verb = "error".to_string();
                        }
                    }
                    app.scroll_to_bottom();
                    continue;
                }

                // Newline in input
                if code == KeyCode::Enter && modifiers.contains(KeyModifiers::SHIFT) {
                    app.input_newline();
                    continue;
                }

                // Character input
                if let KeyCode::Char(c) = code {
                    app.input_char(c);
                    continue;
                }

                // Backspace
                if code == KeyCode::Backspace {
                    app.input_backspace();
                    continue;
                }

                // Delete
                if code == KeyCode::Delete {
                    app.input_delete();
                    continue;
                }

                // Cursor movement
                if code == KeyCode::Left {
                    app.input_move_left();
                    continue;
                }
                if code == KeyCode::Right {
                    app.input_move_right();
                    continue;
                }
                if code == KeyCode::Home {
                    app.input_home();
                    continue;
                }
                if code == KeyCode::End {
                    app.input_end();
                    continue;
                }
            }

            Event::Resize(_, _) => {
                // Terminal will redraw on next loop
            }
            Event::Paste(_) => {
                // Ignored in minimal TUI mode.
            }
        }
    }
}

/// Draw the minimal interface.
fn draw_minimal(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Explicitly calculate areas (status at TOP, content below)
    let status_height = 1;

    // Status line at the very top
    let status_area = Rect {
        x: 0,
        y: 0,
        width: size.width,
        height: status_height,
    };

    // Content area below status line
    let content_area = Rect {
        x: 0,
        y: status_height,
        width: size.width,
        height: size.height.saturating_sub(status_height),
    };

    // Draw content first (background)
    draw_content(frame, app, content_area);

    // Draw status line on top (at the top of screen)
    draw_status_line(frame, app, status_area);
}

/// Status line: minimal, symbolic, type-driven with glitter verbs.
///
/// ● Contemplating… model·3 45%
/// ○ ready
/// ● Reading… model·3 45%
/// ● error model·3 45%
///
/// Uses:
/// - Symbols (●○) for thinking/ready/error state
/// - Glitter verbs for animated status messages
/// - Minimal punctuation (· instead of | or —)
/// - Color coding for quick recognition
fn draw_status_line(frame: &mut Frame, app: &App, area: Rect) {
    // Determine status color and indicator state
    // Priority: Error > Running > Ready
    let (status_color, show_filled) = if app.is_error {
        (Color::Red, true) // Error: ● Red
    } else if app.is_running {
        (Color::Cyan, true) // Running: ● Cyan
    } else {
        (Color::Green, false) // Ready: ○ Green
    };

    // Create longer-lived bindings for format! strings
    let model_label = format!("{}·", app.model_name);
    let iteration_label = if app.iteration > 0 {
        format!("{} ", app.iteration)
    } else {
        String::new()
    };

    // Build minimal status line with thinking indicator
    let parts: Vec<Span> = vec![
        // Thinking/ready/error indicator (pulsing ● when running or error)
        Span::styled(
            if show_filled { "●" } else { "○" },
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        // Space separator
        Span::styled(" ", Style::default()),
        // Glitter verb (animated status message)
        Span::styled(
            &app.verb,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        // Space separator
        Span::styled(" ", Style::default()),
        // Model name
        Span::styled(&model_label, Style::default().fg(Color::DarkGray)),
        // Iteration
        Span::styled(&iteration_label, Style::default().fg(Color::Yellow)),
        // Tokens
        if app.token_budget > 0 {
            let pct = (app.tokens_used as f64 / app.token_budget as f64 * 100.0) as u64;
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

    let status_line = Paragraph::new(Line::from(parts)).style(Style::default().bg(Color::Reset));

    frame.render_widget(status_line, area);
}

/// Content area: chat history + input, no borders.
fn draw_content(frame: &mut Frame, app: &App, area: Rect) {
    // Clear the content area with a background
    let clear_block = ratatui::widgets::Block::default().style(Style::default().bg(Color::Reset));
    frame.render_widget(clear_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Chat history (fills available space)
            Constraint::Length(3), // Input area (fixed 3 lines)
        ])
        .split(area);

    let chat_area = chunks[0];
    let input_area = chunks[1];

    // Draw chat history (raw, borderless, wrap text)
    draw_chat_history(frame, app, chat_area);

    // Draw input (minimal prompt)
    draw_input(frame, app, input_area);
}

/// Draw chat history - brutalist, no chrome.
fn draw_chat_history(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for entry in &app.chat_history {
        match entry {
            ChatEntry::TranscriptLine(s) => {
                // Raw transcript lines - typeface distinguishes them
                for line in s.lines() {
                    // Highlight [Ferro] and [You] markers
                    if line.starts_with("[Ferro]") {
                        let styled_line = line.replacen("[Ferro]", "Ferro:", 1);
                        lines.push(Line::from(Span::styled(
                            format!("  {}", styled_line),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )));
                    } else if line.starts_with("[You]") {
                        let styled_line = line.replacen("[You]", "You:", 1);
                        lines.push(Line::from(Span::styled(
                            format!("  {}", styled_line),
                            Style::default().fg(Color::Cyan),
                        )));
                    } else if line.starts_with("[ERROR]") {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        )));
                    } else if line.contains("thinking...") || line.contains("Still waiting") {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::Yellow),
                        )));
                    } else {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }
            ChatEntry::UserMessage(text) => {
                // User messages: stark, clear marker
                lines.push(Line::from(vec![
                    Span::styled("> ", Style::default().fg(Color::Cyan)),
                    Span::raw(text),
                ]));
            }
            ChatEntry::AssistantMessage(text) => {
                // Ferro responses: indented, with "Ferro:" prefix (clean)
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  Ferro: {}", line),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    )));
                }
            }
            ChatEntry::ToolCall { name, .. } => {
                // Tool calls: symbolic, minimal
                lines.push(Line::from(Span::styled(
                    format!("→ {}", name),
                    Style::default().fg(Color::Yellow),
                )));
            }
            ChatEntry::ToolResult { name, is_error, .. } => {
                // Tool results: symbol + status
                let symbol = if *is_error { "✗" } else { "✓" };
                lines.push(Line::from(Span::styled(
                    format!("← {} {}", symbol, name),
                    Style::default().fg(if *is_error { Color::Red } else { Color::Green }),
                )));
            }
            ChatEntry::SystemInfo(text) => {
                // System info: subtle
                lines.push(Line::from(Span::styled(
                    format!("  {}", text),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            ChatEntry::Error(text) => {
                // Errors: stark, visible
                lines.push(Line::from(vec![
                    Span::styled("[", Style::default().fg(Color::Red)),
                    Span::styled(
                        "ERROR",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("]", Style::default().fg(Color::Red)),
                    Span::styled(format!(" {}", text), Style::default().fg(Color::Red)),
                ]));
            }
        }
        lines.push(Line::from(""));
    }

    // Calculate scroll position
    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(1);
    let scroll_offset = if total > visible {
        let max_scroll = total - visible;
        max_scroll.saturating_sub(app.scroll_offset)
    } else {
        0
    };

    let chat = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(chat, area);
}

/// Draw input - minimal prompt, no border.
fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    // Split: prompt marker | input field
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2), // "> " prompt
            Constraint::Min(0),    // Input text
        ])
        .split(area);

    let prompt_area = chunks[0];
    let input_area = chunks[1];

    // Prompt marker (minimal)
    let prompt = Paragraph::new("> ").style(Style::default().fg(Color::Cyan));
    frame.render_widget(prompt, prompt_area);

    // Input text
    let input_text: Vec<Line> = app
        .input_lines
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();

    let input = Paragraph::new(Text::from(input_text)).wrap(Wrap { trim: false });
    frame.render_widget(input, input_area);

    // Cursor
    let cursor_x = input_area.x + app.cursor_col as u16;
    let cursor_y = input_area.y + app.cursor_line as u16;
    if cursor_x < input_area.x + input_area.width && cursor_y < input_area.y + input_area.height {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Process agent events into chat entries.
fn process_events(app: &mut App, events: &[AgentEvent]) {
    // Track active tools for glitter verbs
    let mut active_tools: Vec<String> = Vec::new();

    for event in events {
        match event {
            AgentEvent::ToolCallStart { name, .. } => {
                active_tools.push(name.clone());
                app.iteration += 1;
                // Update verb with glitter verb for tools
                app.verb = get_glitter_verb(true, app.iteration, &active_tools, app.run_started_at);
            }
            AgentEvent::ToolResult {
                name,
                content,
                is_error,
                ..
            } => {
                // Remove tool from active list
                active_tools.retain(|n| n != name);

                // Show first line of result only (brutalist: no noise)
                let preview = content.lines().next().unwrap_or("");
                let symbol = if *is_error { "✗" } else { "✓" };
                app.chat_history.push(ChatEntry::TranscriptLine(format!(
                    "← {} {} {} {}",
                    symbol,
                    name,
                    if *is_error { "[ERROR]" } else { "" },
                    preview
                )));

                // Update verb
                if app.is_running {
                    app.verb =
                        get_glitter_verb(true, app.iteration, &active_tools, app.run_started_at);
                }
            }
            AgentEvent::TokenUsage {
                input,
                output,
                total_used,
            } => {
                app.tokens_used = *total_used;
                app.last_input_tokens = *input;
                app.last_output_tokens = *output;
            }
            AgentEvent::Error(msg) => {
                app.chat_history
                    .push(ChatEntry::TranscriptLine(format!("[ERROR] {}", msg)));
                app.is_error = true;
                app.verb = "error".to_string();
            }
            AgentEvent::LlmRound { iteration } | AgentEvent::ModelToolChoice { iteration, .. } => {
                app.iteration = *iteration;
                // Update verb with glitter verb for LLM
                app.verb = get_glitter_verb(true, app.iteration, &active_tools, app.run_started_at);
            }
            AgentEvent::ParallelToolBatch { .. } => {
                // No iteration field in this variant
            }
            AgentEvent::TextDelta(_) | AgentEvent::Done { .. } => {
                // Text captured in final response
            }
        }
    }
}

/// Show "still waiting" nudge if agent takes too long.
fn maybe_nudge_if_slow(app: &mut App) {
    let Some(started) = app.run_started_at else {
        return;
    };

    let elapsed = started.elapsed().as_secs();
    if elapsed < 10 {
        return;
    }

    // Only nudge once per 10-second bucket
    if app.last_nudge_sec == elapsed as u32 {
        return;
    }
    app.last_nudge_sec = elapsed as u32;

    // Update verb with glitter verb for long wait
    app.verb = get_glitter_verb(true, app.iteration, &[], app.run_started_at);

    app.chat_history.push(ChatEntry::TranscriptLine(format!(
        "[{}s] thinking...",
        elapsed
    )));
}
