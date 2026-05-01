//! Kinetic TUI - The interface breathes with the agent.
//!
//! V3 Design Philosophy:
//! - Motion is information (status changes = visible)
//! - Density encodes state (velocity, depth, pressure)
//! - Glitch aesthetic (raw, alive, not polished)
//! - No borders, typography over chrome
//!
//! Layout:
//! ┌────────────────────────────────────────┐
//! │ ● Thinking… model·3 45% ████████░░ 8/s │  ← Kinetic status (pulse+progress)
//! │                                         │
//! │  Ferro: Response here                  │  ← Chat (borderless, wrap)
//! │  → tool_call                           │     Tool calls glitch on entry
//! │  ← ✓ result                            │
//! │                                         │
//! │ > your input_                          │  ← Input (minimal prompt)
//! └────────────────────────────────────────┘

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

/// Kinetic TUI configuration
#[allow(dead_code)]
const PROGRESS_BAR_WIDTH: u16 = 20;
#[allow(dead_code)]
const PROGRESS_UPDATE_INTERVAL_MS: u64 = 100;
#[allow(dead_code)]
const GLITCH_FRAMES: u8 = 2;

/// Run the kinetic TUI.
pub async fn run_kinetic_tui(mut agent_loop: AgentLoop, config: &Config) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let model_name = config.agent.default_model.clone();
    let token_budget = config.agent.token_budget;

    let mut app = App::new(model_name.clone(), token_budget);

    // Welcome message with kinetic intro
    app.chat_history.push(ChatEntry::TranscriptLine(format!(
        "ferroclaw v{} — {} — initialized.",
        env!("CARGO_PKG_VERSION"),
        model_name
    )));
    app.chat_history.push(ChatEntry::TranscriptLine(
        "System ready. Awaiting input.".to_string(),
    ));
    app.chat_history
        .push(ChatEntry::TranscriptLine(String::new()));

    let event_handler = EventHandler::new(100); // Faster tick for animations
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

/// Main event loop with kinetic updates.
async fn run_loop(
    terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_handler: &EventHandler,
    agent_loop: &mut AgentLoop,
    history: &mut Vec<Message>,
) -> anyhow::Result<()> {
    let mut frame_count: u64 = 0;

    loop {
        frame_count += 1;
        terminal.draw(|frame| draw_kinetic(frame, app, frame_count))?;

        match event_handler.next()? {
            Event::Tick => {
                // Update kinetic state on every tick
                if app.is_running {
                    update_kinetic_state(app);
                    terminal.draw(|frame| draw_kinetic(frame, app, frame_count))?;
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

                // Scroll
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

                    app.chat_history
                        .push(ChatEntry::TranscriptLine(format!("[You] {}", input)));
                    app.scroll_to_bottom();
                    app.is_running = true;
                    app.run_started_at = Some(Instant::now());
                    app.iteration = 0;

                    terminal.draw(|frame| draw_kinetic(frame, app, frame_count))?;

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
                        }
                        Err(e) => {
                            app.chat_history
                                .push(ChatEntry::TranscriptLine(format!("[ERROR] {}", e)));
                            app.is_running = false;
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
                // Ignored in kinetic TUI mode.
            }
        }
    }
}

/// Update kinetic state (animations, progress bars)
#[allow(dead_code)]
fn update_kinetic_state(_app: &mut App) {
    // This would be called every tick to update animations
    // For now, placeholder for future animation logic
}

/// Draw the kinetic interface with Hermes-style borders.
fn draw_kinetic(frame: &mut Frame, app: &App, frame_count: u64) {
    let size = frame.area();

    // Split from bottom up:
    // - Status bar (1 line)
    // - Glitter verb (1 line)
    // - Chat history (fills remaining)
    // - Input area (3 lines at bottom)
    let status_height = 1;
    let glitter_height = 1;
    let input_height = 3;

    let input_area = Rect {
        x: 0,
        y: size.height.saturating_sub(input_height),
        width: size.width,
        height: input_height,
    };

    let glitter_area = Rect {
        x: 0,
        y: input_area.y.saturating_sub(glitter_height),
        width: size.width,
        height: glitter_height,
    };

    let chat_area = Rect {
        x: 0,
        y: status_height,
        width: size.width,
        height: glitter_area.y.saturating_sub(status_height),
    };

    let status_area = Rect {
        x: 0,
        y: 0,
        width: size.width,
        height: status_height,
    };

    // Draw status bar at top
    draw_status_bar(frame, app, status_area);

    // Draw chat history with border
    draw_chat_history(frame, app, chat_area, frame_count);

    // Draw glitter verb (directly above input)
    draw_glitter_verb(frame, app, glitter_area, frame_count);

    // Draw input with border
    draw_input(frame, app, input_area);
}

/// Status bar at top (Hermes-style).
fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status_text = format!(
        " {} | {}/{} | [▓ {}%] | {}s ",
        app.model_name,
        app.tokens_used,
        app.token_budget,
        (app.tokens_used as f64 / app.token_budget as f64 * 100.0) as u64,
        app.run_started_at.map_or(0, |s| s.elapsed().as_secs()),
    );

    let status = Paragraph::new(status_text).style(
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(status, area);
}

/// Glitter verb - thinking indicator directly above input.
fn draw_glitter_verb(frame: &mut Frame, app: &App, area: Rect, _frame_count: u64) {
    // Determine state and color
    let status_color = if app.is_running {
        Color::Cyan
    } else if app.is_error {
        Color::Red
    } else {
        Color::Green
    };

    // Calculate glitter verb (animated status message)
    let glitter_verb = get_glitter_verb(
        app.is_running,
        app.iteration,
        &[], // TODO: track active_tools from events
        app.run_started_at,
    );

    // Clean, minimal display: just the verb
    let parts: Vec<Span> = vec![Span::styled(
        &glitter_verb,
        Style::default()
            .fg(status_color)
            .add_modifier(Modifier::BOLD),
    )];

    let glitter_line = Paragraph::new(Line::from(parts)).style(Style::default().bg(Color::Reset));

    frame.render_widget(glitter_line, area);
}

/// Content area: chat history + input.
#[allow(dead_code)]
fn draw_content(frame: &mut Frame, app: &App, area: Rect, frame_count: u64) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let chat_area = chunks[0];
    let input_area = chunks[1];

    draw_chat_history(frame, app, chat_area, frame_count);
    draw_input(frame, app, input_area);
}

/// Chat history with Hermes-style message bubbles.
fn draw_chat_history(frame: &mut Frame, app: &App, area: Rect, _frame_count: u64) {
    let mut lines: Vec<Line> = Vec::new();

    for entry in &app.chat_history {
        match entry {
            ChatEntry::TranscriptLine(s) => {
                for line in s.lines() {
                    // Style markers
                    if line.starts_with("[Ferro]") {
                        let styled = line.replacen("[Ferro]", "Ferro:", 1);
                        lines.push(Line::from(Span::styled(
                            format!("  {}", styled),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )));
                    } else if line.starts_with("[You]") {
                        let styled = line.replacen("[You]", "You:", 1);
                        lines.push(Line::from(Span::styled(
                            format!("  {}", styled),
                            Style::default().fg(Color::Cyan),
                        )));
                    } else if line.starts_with("[ERROR]") {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        )));
                    } else {
                        lines.push(Line::from(Span::styled(
                            line,
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }
            ChatEntry::UserMessage(text) => {
                // Orange dot + "You:" like Hermes
                lines.push(Line::from(vec![
                    Span::styled(
                        "●",
                        Style::default()
                            .fg(Color::Rgb(255, 107, 53))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " You: ",
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(text),
                ]));
            }
            ChatEntry::AssistantMessage(text) => {
                // "Ferroclaw:" header like Hermes
                lines.push(Line::from(vec![Span::styled(
                    "Ferroclaw: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )]));
                for line in text.lines() {
                    lines.push(Line::from(format!("    {}", line)));
                }
            }
            ChatEntry::ToolCall { name, .. } => {
                // Tool call with arrow
                lines.push(Line::from(vec![
                    Span::styled("  -> ", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            ChatEntry::ToolResult {
                name,
                content,
                is_error,
            } => {
                let color = if *is_error { Color::Red } else { Color::Green };
                let label = if *is_error { "ERR" } else { "OK" };
                lines.push(Line::from(vec![Span::styled(
                    format!("  <- {} [{}] ", name, label),
                    Style::default().fg(color),
                )]));
                // Show first 3 lines of result
                for (i, ln) in content.lines().enumerate() {
                    if i >= 3 {
                        lines.push(Line::from(Span::styled(
                            format!("     ... ({} more lines)", content.lines().count() - 3),
                            Style::default().fg(Color::DarkGray),
                        )));
                        break;
                    }
                    lines.push(Line::from(Span::styled(
                        format!("     {}", ln),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
            ChatEntry::SystemInfo(text) => {
                lines.push(Line::from(Span::styled(
                    text,
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            ChatEntry::Error(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "ERROR: ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text, Style::default().fg(Color::Red)),
                ]));
            }
        }
        lines.push(Line::from(""));
    }

    // Scroll calculation (scroll_offset: 0 = bottom/latest, higher = older)
    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(2); // Account for borders
    let scroll_offset = if total > visible {
        app.scroll_offset
    } else {
        0
    };

    let chat = Paragraph::new(Text::from(lines))
        .block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title(" Chat ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(chat, area);
}

/// Input prompt with Hermes-style border.
fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    // Account for border when positioning cursor
    let inner_area = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .title(" Type your message... ")
        .border_style(Style::default().fg(Color::Cyan))
        .inner(area);

    let input_text: Vec<Line> = app
        .input_lines
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();

    let input = Paragraph::new(Text::from(input_text)).wrap(Wrap { trim: false });

    frame.render_widget(
        ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .title(" Type your message... ")
            .border_style(Style::default().fg(Color::Cyan)),
        area,
    );
    frame.render_widget(input, inner_area);

    // Cursor (account for border offset)
    let cursor_x = inner_area.x + app.cursor_col as u16;
    let cursor_y = inner_area.y + app.cursor_line as u16;
    if cursor_x < inner_area.x + inner_area.width && cursor_y < inner_area.y + inner_area.height {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Process agent events into chat entries.
fn process_events(app: &mut App, events: &[AgentEvent]) {
    for event in events {
        match event {
            AgentEvent::ToolCallStart { name: _, .. } => {
                app.iteration += 1;
            }
            AgentEvent::ToolResult {
                name,
                content,
                is_error,
                ..
            } => {
                let preview = content.lines().next().unwrap_or("");
                let symbol = if *is_error { "✗" } else { "✓" };
                app.chat_history.push(ChatEntry::TranscriptLine(format!(
                    "← {} {} {} {}",
                    symbol,
                    name,
                    if *is_error { "[ERROR]" } else { "" },
                    preview
                )));
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
            }
            AgentEvent::LlmRound { iteration } | AgentEvent::ModelToolChoice { iteration, .. } => {
                app.iteration = *iteration;
            }
            AgentEvent::ParallelToolBatch { .. } => {
                // No iteration field
            }
            AgentEvent::TextDelta(_) | AgentEvent::Done { .. } => {}
        }
    }
}
