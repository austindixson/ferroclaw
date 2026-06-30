//! Terminal UI module for Ferroclaw.
//!
//! Provides a rich TUI using ratatui + crossterm with:
//! - Top banner bar (model, tokens)
//! - Scrollable chat history
//! - Multiline input area
//! - Status bar with connection info
//!
//! Also includes a Hermes-style TUI with:
//! - Dark theme
//! - Message bubbles (assistant: "Ferroclaw" header + text; user: orange dot + text)
//! - Bottom status bar with model/process info

pub mod app;
pub mod colors;
pub mod composer;
pub mod events;
pub mod glitter_verbs;
pub mod hermes_tui;
pub mod hermes_ui;
pub mod kinetic_tui;
pub mod live_panels;
pub mod minimal_tui;
pub mod model_select;
pub mod orchestrator_tui;
pub mod orchestrator_ui;
pub mod thinking_indicator;
pub mod thinking_indicator_demo;
pub mod ui;

use crate::agent::AgentLoop;
use crate::agent::r#loop::AgentEvent;
use crate::config::Config;
use crate::types::Message;

use app::{App, ChatEntry};
use events::{Event, EventHandler};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;

/// Run the full TUI REPL. Takes ownership of the agent loop and config.
pub async fn run_tui(mut agent_loop: AgentLoop, config: &Config) -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let model_name = config.agent.default_model.clone();
    let token_budget = config.agent.token_budget;

    let mut app = App::new(model_name, token_budget);
    let event_handler = EventHandler::new(250);
    let mut history: Vec<Message> = Vec::new();

    app.chat_history.push(ChatEntry::SystemInfo(format!(
        "Ferroclaw v{} -- Security-first AI agent. Type a message and press Enter to send.",
        env!("CARGO_PKG_VERSION"),
    )));

    // Main loop
    let result = run_loop(
        &mut terminal,
        &mut app,
        &event_handler,
        &mut agent_loop,
        &mut history,
    )
    .await;

    // Restore terminal (always, even on error)
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_handler: &EventHandler,
    agent_loop: &mut AgentLoop,
    history: &mut Vec<Message>,
) -> anyhow::Result<()> {
    loop {
        // Draw UI
        terminal.draw(|frame| ui::draw(frame, app))?;

        // Handle events
        match event_handler.next()? {
            Event::Tick => {
                // Nothing to do on tick, just redraw
            }
            Event::MouseScrollUp => {
                app.scroll_up(3);
                continue;
            }
            Event::MouseClick(_, _) => {}
            Event::MouseScrollDown => {
                app.scroll_down(3);
                continue;
            }
            Event::Key(key_event) => {
                use crossterm::event::KeyCode;
                use crossterm::event::KeyModifiers;

                let code = key_event.code;
                let modifiers = key_event.modifiers;

                // Ctrl+C: quit
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    return Ok(());
                }

                // Ctrl+L: clear chat
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('l') {
                    app.clear_chat();
                    continue;
                }

                // PageUp / PageDown: scroll chat
                if code == KeyCode::PageUp {
                    app.scroll_up(10);
                    continue;
                }
                if code == KeyCode::PageDown {
                    app.scroll_down(10);
                    continue;
                }

                // Shift+Up / Shift+Down: scroll by 1
                if modifiers.contains(KeyModifiers::SHIFT) && code == KeyCode::Up {
                    app.scroll_up(1);
                    continue;
                }
                if modifiers.contains(KeyModifiers::SHIFT) && code == KeyCode::Down {
                    app.scroll_down(1);
                    continue;
                }

                // Character input (exclude task shortcuts)
                if let KeyCode::Char(c) = code {
                    // Skip task management shortcuts: n, d, c
                    if c == 'n' || c == 'd' {
                        continue; // Ignore these keys
                    }
                    app.input_char(c);
                    continue;
                }

                // Enter: send message (Shift+Enter for newline)
                if code == KeyCode::Enter && !modifiers.contains(KeyModifiers::SHIFT) {
                    let input = app.take_input();
                    if input.is_empty() {
                        continue;
                    }

                    app.chat_history.push(ChatEntry::UserMessage(input.clone()));
                    app.scroll_to_bottom();
                    app.set_status("Thinking...");
                    app.iteration = 0;

                    // Redraw with the user message visible
                    terminal.draw(|frame| ui::draw(frame, app))?;

                    // Run the agent
                    match agent_loop.run(&input, history).await {
                        Ok((outcome, events)) => {
                            process_agent_events(app, &events);
                            app.chat_history
                                .push(ChatEntry::AssistantMessage(outcome.text));
                            app.set_status("Ready");
                        }
                        Err(e) => {
                            app.chat_history.push(ChatEntry::Error(format!("{e}")));
                            app.set_status("Error");
                        }
                    }
                    app.scroll_to_bottom();
                    continue;
                }

                // Shift+Enter or Alt+Enter: newline in input
                if code == KeyCode::Enter && modifiers.contains(KeyModifiers::SHIFT) {
                    app.input_newline();
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

                // Arrow keys for cursor movement in input
                if code == KeyCode::Left {
                    app.input_move_left();
                    continue;
                }
                if code == KeyCode::Right {
                    app.input_move_right();
                    continue;
                }
                if code == KeyCode::Up && !modifiers.contains(KeyModifiers::SHIFT) {
                    app.input_move_up();
                    continue;
                }
                if code == KeyCode::Down && !modifiers.contains(KeyModifiers::SHIFT) {
                    app.input_move_down();
                    continue;
                }

                // Home / End
                if code == KeyCode::Home {
                    app.input_home();
                    continue;
                }
                if code == KeyCode::End {
                    app.input_end();
                    continue;
                }

                // Tab -> 4 spaces
                if code == KeyCode::Tab {
                    for _ in 0..4 {
                        app.input_char(' ');
                    }
                    continue;
                }

                // Character input (exclude task shortcuts: n, d)
                if let KeyCode::Char(c) = code {
                    // Skip task management shortcuts: n, d
                    if c == 'n' || c == 'd' {
                        continue; // Ignore these keys
                    }
                    app.input_char(c);
                    continue;
                }
            }
            Event::Resize(_, _) => {
                // Terminal will redraw on next iteration
            }
            Event::Paste(_) => {
                // Base TUI mode ignores paste payloads.
            }
        }
    }
}

/// Process AgentEvents into ChatEntry items for the TUI.
fn process_agent_events(app: &mut App, events: &[AgentEvent]) {
    for event in events {
        match event {
            AgentEvent::ToolCallStart { name, .. } => {
                app.chat_history.push(ChatEntry::ToolCall {
                    name: name.clone(),
                    args: String::new(),
                });
                app.iteration += 1;
                app.auto_scroll_if_sticky();
            }
            AgentEvent::LlmRound { .. }
            | AgentEvent::ModelToolChoice { .. }
            | AgentEvent::ParallelToolBatch { .. } => {}
            AgentEvent::ToolResult {
                name,
                content,
                is_error,
                ..
            } => {
                app.chat_history.push(ChatEntry::ToolResult {
                    name: name.clone(),
                    content: content.clone(),
                    is_error: *is_error,
                });
                app.auto_scroll_if_sticky();
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
                app.chat_history.push(ChatEntry::Error(msg.clone()));
                app.auto_scroll_if_sticky();
            }
            AgentEvent::TextDelta(_) | AgentEvent::Done { .. } => {
                // Text deltas are already captured in the final response
            }
        }
    }
}
