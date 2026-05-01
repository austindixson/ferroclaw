//! Event handling for the TUI using crossterm.
//!
//! Spawns a background thread that polls for terminal events and emits
//! `Event::Key`, `Event::Resize`, or `Event::Tick` on a channel.

use crossterm::event::{self, KeyCode, KeyEvent, KeyEventKind, MouseEventKind};
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

/// Events produced by the event handler.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A key was pressed.
    Key(KeyEvent),
    /// Text paste payload (supports drag/drop path paste in many terminals).
    Paste(String),
    /// Mouse wheel scrolled up.
    MouseScrollUp,
    /// Mouse wheel scrolled down.
    MouseScrollDown,
    /// The terminal was resized.
    Resize(u16, u16),
    /// A tick elapsed (for periodic redraws).
    Tick,
}

/// Key bindings for task management.
impl Event {
    /// Check if this event should be handled by the sidebar task module.
    pub fn is_task_shortcut(&self) -> bool {
        matches!(self, Event::Key(key) if matches!(key.code, KeyCode::Char('n' | 'd' | 'c')))
    }

    /// Check if this event is a navigation key that might be used for tasks.
    pub fn is_task_navigation(&self) -> bool {
        if let Event::Key(key) = self {
            matches!(key.code, KeyCode::Up | KeyCode::Down) && key.modifiers.is_empty()
        } else {
            false
        }
    }

    /// Get the task command if this event maps to one.
    pub fn as_task_command(&self) -> Option<TaskCommand> {
        match self {
            Event::Key(key) if key.code == KeyCode::Char('n') => Some(TaskCommand::New),
            Event::Key(key) if key.code == KeyCode::Char('d') => Some(TaskCommand::Delete),
            Event::Key(key) if key.code == KeyCode::Char('c') => Some(TaskCommand::ToggleStatus),
            Event::Key(key) if key.code == KeyCode::Up && key.modifiers.is_empty() => {
                Some(TaskCommand::SelectUp)
            }
            Event::Key(key) if key.code == KeyCode::Down && key.modifiers.is_empty() => {
                Some(TaskCommand::SelectDown)
            }
            _ => None,
        }
    }
}

/// Commands for the task sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCommand {
    /// Add a new task.
    New,
    /// Delete the selected task.
    Delete,
    /// Toggle the status of the selected task.
    ToggleStatus,
    /// Move selection up.
    SelectUp,
    /// Move selection down.
    SelectDown,
}

/// Polls crossterm events on a background thread and sends them through a channel.
pub struct EventHandler {
    rx: mpsc::Receiver<Event>,
    // Keep the handle alive so the thread doesn't get detached unexpectedly.
    _tx: mpsc::Sender<Event>,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate in milliseconds.
    pub fn new(tick_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::channel();
        let event_tx = tx.clone();
        let tick_duration = Duration::from_millis(tick_rate_ms);

        std::thread::spawn(move || {
            loop {
                // Poll with the tick duration as timeout
                if event::poll(tick_duration).unwrap_or(false) {
                    match event::read() {
                        Ok(event::Event::Key(key)) => {
                            // Only send key press events (ignore release/repeat on some platforms)
                            if key.kind == KeyEventKind::Press
                                && event_tx.send(Event::Key(key)).is_err()
                            {
                                return;
                            }
                        }
                        Ok(event::Event::Mouse(mouse)) => {
                            let mapped = match mouse.kind {
                                MouseEventKind::ScrollUp => Some(Event::MouseScrollUp),
                                MouseEventKind::ScrollDown => Some(Event::MouseScrollDown),
                                _ => None,
                            };
                            if let Some(ev) = mapped
                                && event_tx.send(ev).is_err()
                            {
                                return;
                            }
                        }
                        Ok(event::Event::Resize(w, h)) => {
                            if event_tx.send(Event::Resize(w, h)).is_err() {
                                return;
                            }
                        }
                        Ok(event::Event::Paste(data)) => {
                            if event_tx.send(Event::Paste(data)).is_err() {
                                return;
                            }
                        }
                        Ok(_) => {
                            // Ignore focus events, etc.
                        }
                        Err(_) => {
                            return;
                        }
                    }
                } else {
                    // Timeout: emit a tick
                    if event_tx.send(Event::Tick).is_err() {
                        return;
                    }
                }
            }
        });

        Self { rx, _tx: tx }
    }

    /// Block until the next event is available.
    pub fn next(&self) -> anyhow::Result<Event> {
        self.rx
            .recv()
            .map_err(|e| anyhow::anyhow!("Event channel closed: {e}"))
    }

    /// Non-blocking poll for UI events (e.g. while the agent is running).
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Block until the next event or timeout (for interleaving with agent streaming).
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Event, RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }
}
