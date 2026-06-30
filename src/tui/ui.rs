//! UI rendering for the Ferroclaw TUI using ratatui.

use super::app::{App, ChatEntry, TaskStatus};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

/// Draw the entire TUI layout with sidebar on the left.
pub fn draw(frame: &mut Frame, app: &mut App) {
    // Main horizontal layout: sidebar (left) + content (right)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30), // Sidebar width (fixed)
            Constraint::Min(50),    // Main content area
        ])
        .split(frame.area());

    // Sidebar vertical layout
    let sidebar_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Sidebar header
            Constraint::Min(5),    // Task list (scrollable)
            Constraint::Length(1), // Footer
        ])
        .split(main_chunks[0]);

    // Content vertical layout
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Banner bar
            Constraint::Min(5),    // Chat history
            Constraint::Length(5), // Input area
            Constraint::Length(1), // Status bar
        ])
        .split(main_chunks[1]);

    draw_sidebar_header(frame, app, sidebar_chunks[0]);
    draw_sidebar_tasks(frame, app, sidebar_chunks[1]);
    draw_sidebar_footer(frame, app, sidebar_chunks[2]);
    draw_banner(frame, app, content_chunks[0]);
    draw_chat(frame, app, content_chunks[1]);
    draw_input(frame, app, content_chunks[2]);
    draw_status(frame, app, content_chunks[3]);
}

/// Sidebar header with title and task count.
fn draw_sidebar_header(frame: &mut Frame, app: &App, area: Rect) {
    let completed_count = app
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Completed)
        .count();
    let pending_count = app.tasks.len() - completed_count;

    let header_text = format!(
        " Tasks ({}) ✓{} ⏳{} ",
        app.tasks.len(),
        completed_count,
        pending_count
    );

    let header = Paragraph::new(header_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .centered();

    frame.render_widget(header, area);
}

/// Scrollable task list in the sidebar.
fn draw_sidebar_tasks(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.tasks.is_empty() {
        let empty_msg = Paragraph::new("No tasks yet.\nPress 'n' to add a task.")
            .style(Style::default().fg(Color::DarkGray))
            .centered();
        frame.render_widget(empty_msg, area);
        return;
    }

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .enumerate()
        .map(|(idx, task)| {
            let (prefix, style) = match task.status {
                TaskStatus::Pending => ("□ ", Style::default().fg(Color::White)),
                TaskStatus::InProgress => ("◉ ", Style::default().fg(Color::Yellow)),
                TaskStatus::Completed => ("✓ ", Style::default().fg(Color::Green)),
            };

            let task_label = if idx == app.selected_task_index {
                format!("> {}{}", prefix, task.title)
            } else {
                format!("  {}{}", prefix, task.title)
            };

            let task_style = if idx == app.selected_task_index {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            };

            let spans = vec![Span::styled(task_label, task_style)];

            // Show description if this task is selected and there's room
            if idx == app.selected_task_index && !task.description.is_empty() {
                let desc_lines: Vec<Line> = task
                    .description
                    .lines()
                    .map(|line| {
                        Line::from(Span::styled(
                            format!("    │ {}", line),
                            Style::default().fg(Color::DarkGray),
                        ))
                    })
                    .collect();
                return ListItem::new(
                    vec![Line::from(spans)]
                        .into_iter()
                        .chain(desc_lines)
                        .collect::<Vec<Line>>(),
                );
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let task_list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    frame.render_widget(task_list, area);
}

/// Sidebar footer with keyboard shortcuts.
fn draw_sidebar_footer(frame: &mut Frame, _app: &App, area: Rect) {
    let footer_text = " n:new d:del c:cmp ↑↓:nav ";
    let footer =
        Paragraph::new(footer_text).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(footer, area);
}

/// Top banner bar: model name, token usage, iteration count.
fn draw_banner(frame: &mut Frame, app: &App, area: Rect) {
    let budget_pct = if app.token_budget > 0 {
        (app.tokens_used as f64 / app.token_budget as f64 * 100.0) as u64
    } else {
        0
    };

    let banner_text = format!(
        " ferroclaw | model: {} | tokens: {}/{} ({}%) | iter: {}",
        app.model_name, app.tokens_used, app.token_budget, budget_pct, app.iteration,
    );

    let banner = Paragraph::new(banner_text).style(
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(banner, area);
}

/// Scrollable chat history panel with improved visual design.
fn draw_chat(frame: &mut Frame, app: &mut App, area: Rect) {
    let inner_width = area.width.saturating_sub(4) as usize; // Account for borders + padding
    let mut lines: Vec<Line> = Vec::new();

    for entry in &app.chat_history {
        match entry {
            ChatEntry::TranscriptLine(s) => {
                // Trace/planning lines - more subtle
                lines.push(Line::from(Span::styled(
                    format!("  {s}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            ChatEntry::UserMessage(text) => {
                // User message - more prominent
                lines.push(Line::from("")); // Spacing before
                lines.push(Line::from(vec![
                    Span::styled(
                        "You ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("· ", Style::default().fg(Color::Cyan)),
                    Span::raw(text),
                ]));
            }
            ChatEntry::AssistantMessage(text) => {
                // Assistant message - clean and readable
                lines.push(Line::from("")); // Spacing before
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {line}"),
                        Style::default().fg(Color::White),
                    )));
                }
            }
            ChatEntry::ToolCall { name, args: _ } => {
                // Tool call - subtle trace style
                lines.push(Line::from(vec![
                    Span::styled("  → ", Style::default().fg(Color::Yellow)),
                    Span::styled(name, Style::default().fg(Color::Yellow)),
                ]));
            }
            ChatEntry::ToolResult {
                name,
                content,
                is_error,
            } => {
                // Tool result
                let color = if *is_error { Color::Red } else { Color::Green };
                let icon = if *is_error { "✗" } else { "✓" };
                lines.push(Line::from(vec![
                    Span::styled(format!("  ← {name} "), Style::default().fg(color)),
                    Span::styled(icon, Style::default().fg(color)),
                ]));
                // Show first 2 lines of result, truncated
                for (i, line) in content.lines().enumerate() {
                    if i >= 2 {
                        lines.push(Line::from(Span::styled(
                            format!("     ... ({} more lines)", content.lines().count() - 2),
                            Style::default().fg(Color::DarkGray),
                        )));
                        break;
                    }
                    let truncated = if line.len() > inner_width.saturating_sub(5) {
                        format!("{}...", &line[..inner_width.saturating_sub(8)])
                    } else {
                        line.to_string()
                    };
                    lines.push(Line::from(Span::styled(
                        format!("     {truncated}"),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
            ChatEntry::SystemInfo(text) => {
                // System info - subtle italic
                lines.push(Line::from(Span::styled(
                    format!("  {text}"),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            ChatEntry::Error(text) => {
                // Error - prominent
                lines.push(Line::from("")); // Spacing before
                lines.push(Line::from(vec![
                    Span::styled(
                        "[Error] ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text, Style::default().fg(Color::Red)),
                ]));
            }
            ChatEntry::Thought { duration_secs, expanded, .. } => {
                let chevron = if *expanded { "▾" } else { "▸" };
                lines.push(Line::from(Span::styled(
                    format!("{chevron} Thought · {duration_secs}s"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
        lines.push(Line::from("")); // Blank line between entries
    }

    app.total_chat_lines = lines.len() as u16;
    app.visible_chat_height = area.height.saturating_sub(2); // Account for borders

    // Apply scroll offset: 0 = bottom (newest), higher values = scroll up (older)
    let total = lines.len() as u16;
    let visible = app.visible_chat_height;

    // Calculate scroll position from the bottom
    let scroll_offset_from_top = if total > visible {
        // When scroll_offset is 0, show bottom-most content
        // When scroll_offset is max, show top-most content
        let max_scroll = total - visible;
        max_scroll.saturating_sub(app.scroll_offset)
    } else {
        0
    };

    let chat = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Chat "),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset_from_top, 0));

    frame.render_widget(chat, area);
}

/// Multiline input area with cursor.
fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_text: Vec<Line> = app
        .input_lines
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();

    let input = Paragraph::new(Text::from(input_text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Input (Enter to send, Shift+Enter for newline) ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(input, area);

    // Position the cursor
    let cursor_x = area.x + 1 + app.cursor_col as u16;
    let cursor_y = area.y + 1 + app.cursor_line as u16;
    if cursor_x < area.x + area.width - 1 && cursor_y < area.y + area.height - 1 {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Bottom status bar.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let last_usage = if app.last_input_tokens > 0 || app.last_output_tokens > 0 {
        format!(
            " | last: in={} out={}",
            app.last_input_tokens, app.last_output_tokens
        )
    } else {
        String::new()
    };

    let status_text = format!(
        " {} | Ctrl+C: quit | Ctrl+L: clear | PgUp/PgDn: scroll{}",
        app.status, last_usage,
    );

    let status =
        Paragraph::new(status_text).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(status, area);
}
