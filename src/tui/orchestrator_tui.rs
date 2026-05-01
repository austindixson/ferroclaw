//! Orchestrator-style chat TUI module for Ferroclaw.
//!
//! Nyx-inspired transcript: real-time tool lines, dark palette, teal accents.

use super::app::{App, ChatEntry};
use super::events::{Event, EventHandler};
use crate::agent::AgentLoop;
use crate::agent::r#loop::AgentEvent;
use crate::config::Config;
use crate::types::Message;

use super::orchestrator_ui::draw as draw_orchestrator;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Run the Orchestrator-style TUI REPL. Takes ownership of the agent loop and config.
pub async fn run_orchestrator_tui(agent_loop: AgentLoop, config: &Config) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let model_name = config.agent.default_model.clone();
    let token_budget = config.agent.token_budget;

    let mut app = App::new(model_name, token_budget);
    app.verb = "Ready".into();
    let event_handler = EventHandler::new(250);
    let mut history: Vec<Message> = Vec::new();
    let mut agent_slot = Some(agent_loop);

    app.chat_history.push(ChatEntry::AssistantMessage(
        "Welcome to Ferroclaw! I'm your security-first AI assistant. How can I help you today?"
            .into(),
    ));

    // Add some sample tasks to demonstrate the sidebar
    app.add_task(
        "Review security logs".to_string(),
        "Check for unusual access patterns".to_string(),
    );
    app.add_task(
        "Update dependencies".to_string(),
        "Run cargo update and review changes".to_string(),
    );
    app.add_task(
        "Write documentation".to_string(),
        "Document the new API endpoints".to_string(),
    );

    let result = run_loop(
        &mut terminal,
        &mut app,
        &event_handler,
        &mut agent_slot,
        &mut history,
    );

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_handler: &EventHandler,
    agent_slot: &mut Option<AgentLoop>,
    history: &mut Vec<Message>,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| draw_orchestrator(frame, app))?;

        match event_handler.next()? {
            Event::Tick => {
                if app.is_running {
                    maybe_long_wait_nudge(app);
                    terminal.draw(|frame| draw_orchestrator(frame, app))?;
                }
            }
            Event::MouseScrollUp => {
                app.scroll_up(3);
            }
            Event::MouseScrollDown => {
                app.scroll_down(3);
            }
            Event::Key(key_event) => {
                use crossterm::event::KeyCode;
                use crossterm::event::KeyModifiers;

                let code = key_event.code;
                let modifiers = key_event.modifiers;

                // Task management shortcuts disabled - 'c' key now works for typing
                // if let Some(task_cmd) = Event::Key(key_event).as_task_command() {
                //     handle_task_command(app, task_cmd);
                //     continue;
                // }

                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    return Ok(());
                }

                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('l') {
                    app.clear_chat();
                    app.verb = "Ready".into();
                    continue;
                }

                if code == KeyCode::PageUp {
                    app.scroll_up(10);
                    continue;
                }
                if code == KeyCode::PageDown {
                    app.scroll_down(10);
                    continue;
                }

                if modifiers.contains(KeyModifiers::SHIFT) && code == KeyCode::Up {
                    app.scroll_up(1);
                    continue;
                }
                if modifiers.contains(KeyModifiers::SHIFT) && code == KeyCode::Down {
                    app.scroll_down(1);
                    continue;
                }

                if code == KeyCode::Enter && !modifiers.contains(KeyModifiers::SHIFT) {
                    let input = app.take_input();
                    if input.is_empty() {
                        continue;
                    }

                    let Some(mut al) = agent_slot.take() else {
                        continue;
                    };

                    app.chat_history.push(ChatEntry::TranscriptLine(format!(
                        "[Using model: {}]",
                        app.model_name
                    )));
                    app.chat_history.push(ChatEntry::UserMessage(input.clone()));
                    app.scroll_to_bottom();
                    app.is_running = true;
                    app.run_started_at = Some(Instant::now());
                    app.last_nudge_sec = 0;
                    app.verb = "Thinking…".into();

                    terminal.draw(|frame| draw_orchestrator(frame, app))?;

                    let mut hist = std::mem::take(history);
                    let (tx, rx) = mpsc::channel::<AgentEvent>();
                    let input_for_agent = input;
                    let th = std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new()
                            .expect("failed to build tokio runtime for agent thread");
                        let fut = al.run_with_callback(&input_for_agent, &mut hist, |e| {
                            let _ = tx.send(e.clone());
                        });
                        let res = rt.block_on(fut);
                        (res, hist, al)
                    });

                    loop {
                        while let Ok(ev) = rx.try_recv() {
                            apply_orchestrator_agent_event(app, &ev);
                            terminal.draw(|frame| draw_orchestrator(frame, app))?;
                        }

                        if th.is_finished() {
                            while let Ok(ev) = rx.try_recv() {
                                apply_orchestrator_agent_event(app, &ev);
                                terminal.draw(|frame| draw_orchestrator(frame, app))?;
                            }
                            break;
                        }

                        match event_handler.recv_timeout(Duration::from_millis(50)) {
                            Ok(Event::Tick) => {
                                maybe_long_wait_nudge(app);
                                terminal.draw(|frame| draw_orchestrator(frame, app))?;
                            }
                            Ok(Event::MouseScrollUp) => {
                                app.scroll_up(3);
                                terminal.draw(|frame| draw_orchestrator(frame, app))?;
                                continue;
                            }
                            Ok(Event::MouseScrollDown) => {
                                app.scroll_down(3);
                                terminal.draw(|frame| draw_orchestrator(frame, app))?;
                                continue;
                            }
                            Ok(Event::Resize(_, _)) => {}
                            Ok(Event::Paste(_)) => {}
                            Ok(Event::Key(key_event)) => {
                                use crossterm::event::KeyCode;
                                use crossterm::event::KeyModifiers;
                                let code = key_event.code;
                                let modifiers = key_event.modifiers;

                                // Task shortcuts disabled during agent execution too
                                // if let Some(task_cmd) = Event::Key(key_event).as_task_command() {
                                //     handle_task_command(app, task_cmd);
                                //     terminal.draw(|frame| draw_orchestrator(frame, app))?;
                                //     continue;
                                // }

                                if modifiers.contains(KeyModifiers::CONTROL)
                                    && code == KeyCode::Char('c')
                                {
                                    return Ok(());
                                }
                                if modifiers.contains(KeyModifiers::CONTROL)
                                    && code == KeyCode::Char('l')
                                {
                                    app.clear_chat();
                                    app.verb = "Ready".into();
                                    terminal.draw(|frame| draw_orchestrator(frame, app))?;
                                    continue;
                                }
                                if code == KeyCode::PageUp {
                                    app.scroll_up(10);
                                    terminal.draw(|frame| draw_orchestrator(frame, app))?;
                                    continue;
                                }
                                if code == KeyCode::PageDown {
                                    app.scroll_down(10);
                                    terminal.draw(|frame| draw_orchestrator(frame, app))?;
                                    continue;
                                }
                                if modifiers.contains(KeyModifiers::SHIFT) && code == KeyCode::Up {
                                    app.scroll_up(1);
                                    terminal.draw(|frame| draw_orchestrator(frame, app))?;
                                    continue;
                                }
                                if modifiers.contains(KeyModifiers::SHIFT) && code == KeyCode::Down
                                {
                                    app.scroll_down(1);
                                    terminal.draw(|frame| draw_orchestrator(frame, app))?;
                                    continue;
                                }
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                maybe_long_wait_nudge(app);
                                terminal.draw(|frame| draw_orchestrator(frame, app))?;
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    }

                    let (run_result, hist_back, al_back) = th
                        .join()
                        .map_err(|_| anyhow::anyhow!("agent thread panicked"))?;
                    *history = hist_back;
                    *agent_slot = Some(al_back);

                    app.is_running = false;
                    app.run_started_at = None;
                    app.verb = "Ready".into();
                    app.set_status("Ready");

                    match run_result {
                        Ok(_) => {}
                        Err(e) => {
                            app.chat_history
                                .push(ChatEntry::TranscriptLine(format!("[Error] {e}")));
                            app.set_status("Error");
                        }
                    }

                    app.auto_scroll_if_sticky();
                    continue;
                }

                if code == KeyCode::Enter && modifiers.contains(KeyModifiers::SHIFT) {
                    app.input_newline();
                    continue;
                }

                if code == KeyCode::Backspace {
                    app.input_backspace();
                    continue;
                }

                if code == KeyCode::Delete {
                    app.input_delete();
                    continue;
                }

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

                if code == KeyCode::Home {
                    app.input_home();
                    continue;
                }
                if code == KeyCode::End {
                    app.input_end();
                    continue;
                }

                if let KeyCode::Char(c) = code {
                    app.input_char(c);
                }

                if code == KeyCode::Tab {
                    for _ in 0..4 {
                        app.input_char(' ');
                    }
                }
            }
            Event::Resize(_, _) => {}
            Event::Paste(_) => {}
        }
    }
}

fn verb_for_tool(name: &str) -> &'static str {
    match name {
        n if n.contains("read")
            || n == "glob"
            || n == "grep"
            || n.contains("list")
            || n.contains("Glob")
            || n.contains("Grep") =>
        {
            "Reading…"
        }
        n if n.contains("write")
            || n.contains("str_replace")
            || n.contains("edit")
            || n.contains("Edit") =>
        {
            "Writing…"
        }
        n if n.contains("shell") || n.contains("exec") || n.contains("bash") || n == "run" => {
            "Executing…"
        }
        _ => "Thinking…",
    }
}

fn apply_orchestrator_agent_event(app: &mut App, ev: &AgentEvent) {
    match ev {
        AgentEvent::LlmRound { iteration } => {
            app.iteration = *iteration;
            app.verb = format!("Thinking… · {iteration}");
            app.auto_scroll_if_sticky();
        }
        AgentEvent::ModelToolChoice { iteration, names } => {
            app.iteration = *iteration;
            app.chat_history.push(ChatEntry::TranscriptLine(format!(
                "◆ model → {}",
                names.join(", ")
            )));
            app.verb = format!("Tools… · {iteration}");
            app.auto_scroll_if_sticky();
        }
        AgentEvent::ParallelToolBatch { count } => {
            app.chat_history.push(ChatEntry::TranscriptLine(format!(
                "⋯ Parallel tool batch ({count} calls)"
            )));
            app.auto_scroll_if_sticky();
        }
        AgentEvent::ToolCallStart {
            name, arguments, ..
        } => {
            app.verb = verb_for_tool(name).to_string();
            let preview = args_preview(arguments, 120);
            app.chat_history
                .push(ChatEntry::TranscriptLine(format!("→ {name}({preview})")));
            app.auto_scroll_if_sticky();
        }
        AgentEvent::ToolResult {
            name,
            content,
            is_error,
            ..
        } => {
            if *is_error {
                app.chat_history
                    .push(ChatEntry::TranscriptLine(format!("← {name} [ERR]")));
                let snippet: String = content.chars().take(400).collect();
                if !snippet.is_empty() {
                    app.chat_history.push(ChatEntry::TranscriptLine(snippet));
                }
            } else {
                app.chat_history
                    .push(ChatEntry::TranscriptLine(format!("← {name}")));
            }
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
            app.chat_history
                .push(ChatEntry::TranscriptLine(format!("[Error] {msg}")));
            app.auto_scroll_if_sticky();
        }
        AgentEvent::Done { text } => {
            if !text.trim().is_empty() {
                app.chat_history
                    .push(ChatEntry::AssistantMessage(text.clone()));
            }
            app.verb = "Ready".into();
            app.auto_scroll_if_sticky();
        }
        AgentEvent::TextDelta(_) => {}
    }
}

fn args_preview(raw: &str, max: usize) -> String {
    let raw = raw.trim();
    if raw.len() <= max {
        raw.to_string()
    } else {
        format!("{}…", raw.chars().take(max).collect::<String>())
    }
}

fn maybe_long_wait_nudge(app: &mut App) {
    let Some(start) = app.run_started_at else {
        return;
    };
    let sec = start.elapsed().as_secs() as u32;
    if sec < 30 {
        return;
    }
    if !sec.is_multiple_of(30) {
        return;
    }
    if app.last_nudge_sec == sec {
        return;
    }
    app.last_nudge_sec = sec;
    app.chat_history.push(ChatEntry::TranscriptLine(format!(
        "[{sec}s] Still waiting — Large tool+model responses can take a while. Use Ctrl+C to quit."
    )));
    app.auto_scroll_if_sticky();
}
