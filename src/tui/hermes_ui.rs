//! Hermes-style chat TUI implementation for Ferroclaw.

use crate::tui::app::{App, ChatEntry};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

const BANNER_LINES: &[&str] = &[
    "███████╗███████╗██████╗ ██████╗  ██████╗  ██████╗██╗      █████╗ ██╗    ██╗",
    "██╔════╝██╔════╝██╔══██╗██╔══██╗██╔═══██╗██╔════╝██║     ██╔══██╗██║    ██║",
    "█████╗  █████╗  ██████╔╝██████╔╝██║   ██║██║     ██║     ███████║██║ █╗ ██║",
    "██╔══╝  ██╔══╝  ██╔══██╗██╔══██╗██║   ██║██║     ██║     ██╔══██║██║███╗██║",
    "██║     ███████╗██║  ██║██║  ██║╚██████╔╝╚██████╗███████╗██║  ██║╚███╔███╔╝",
    "╚═╝     ╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝  ╚═════╝╚══════╝╚═╝  ╚═╝ ╚══╝╚══╝",
];

const BORDER: Color = Color::Rgb(88, 106, 140);
const TITLE: Color = Color::Rgb(186, 201, 224);
const MUTED: Color = Color::Rgb(120, 138, 168);
const ACCENT: Color = Color::Rgb(112, 188, 255);
const MAX_CHAT_ENTRIES: usize = 1500;

/// Draw the entire Hermes-style TUI layout.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let full = frame.area();

    // Primary-screen rendering guard:
    // avoid writing to the terminal's physical bottom-right cell, which can trigger
    // autowrap/scroll drift and produce repeated "growing" frames in scrollback.
    let viewport = Rect {
        x: full.x,
        y: full.y,
        width: full.width.saturating_sub(1).max(1),
        height: full.height.saturating_sub(1).max(1),
    };

    let input_height = if viewport.height < 20 { 3 } else { 5 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(2), // Status bar (with top breathing room)
            Constraint::Length(input_height),
        ])
        .split(viewport);

    draw_chat(frame, app, chunks[0]);
    draw_status_bar(frame, app, chunks[1]);
    draw_input(frame, app, chunks[2]);

    if app.slash_menu_visible {
        draw_slash_menu_popup(frame, app, chunks[2]);
    }
}

fn clip_preserve_spaces(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    text.chars().take(width).collect()
}

fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    if text.trim().is_empty() {
        return vec![String::new()];
    }

    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();

    for word in text.split_whitespace() {
        let line_len = line.chars().count();
        let word_len = word.chars().count();

        if line.is_empty() {
            if word_len <= width {
                line.push_str(word);
            } else {
                let chars: Vec<char> = word.chars().collect();
                let mut i = 0usize;
                while i < chars.len() {
                    let end = (i + width).min(chars.len());
                    out.push(chars[i..end].iter().collect());
                    i = end;
                }
            }
            continue;
        }

        if line_len + 1 + word_len <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            out.push(line);
            line = String::new();
            if word_len <= width {
                line.push_str(word);
            } else {
                let chars: Vec<char> = word.chars().collect();
                let mut i = 0usize;
                while i < chars.len() {
                    let end = (i + width).min(chars.len());
                    out.push(chars[i..end].iter().collect());
                    i = end;
                }
            }
        }
    }

    if !line.is_empty() {
        out.push(line);
    }

    if out.is_empty() {
        out.push(String::new());
    }

    out
}

#[allow(dead_code)]
fn draw_header(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new(Text::from(vec![Line::from(Span::styled(
        "Ferroclaw",
        Style::default().fg(TITLE).add_modifier(Modifier::BOLD),
    ))]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title, area);
}

/// Bottom status bar with model/process info.
fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let verb = if app.verb.is_empty() {
        "Ready".to_string()
    } else {
        app.verb.clone()
    };

    let elapsed_secs = app
        .run_started_at
        .map(|started| started.elapsed().as_secs())
        .unwrap_or(0);

    let status_text = format!(
        " {} | {}/{} | {} | {}s",
        app.model_name, app.tokens_used, app.token_budget, verb, elapsed_secs,
    );

    // Keep a blank row above the indicator for visual breathing room.
    let status_row = Rect {
        x: area.x,
        y: area.y.saturating_add(1),
        width: area.width,
        height: 1,
    };

    let status = Paragraph::new(status_text).style(
        Style::default()
            .bg(Color::Rgb(36, 44, 61))
            .fg(TITLE)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(status, status_row);
}

/// Hermes-style chat history with message bubbles.
fn draw_chat(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.chat_history.len() > MAX_CHAT_ENTRIES {
        let drop_n = app.chat_history.len() - MAX_CHAT_ENTRIES;
        app.chat_history.drain(0..drop_n);
    }

    let inner_width = area.width.saturating_sub(4) as usize;
    let mut lines: Vec<Line> = Vec::new();

    let banner_width = area.width as usize;
    for banner in BANNER_LINES {
        lines.push(Line::from(Span::styled(
            clip_preserve_spaces(banner, banner_width.max(1)),
            Style::default().fg(TITLE),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!("Ferroclaw v{} · /help · /model", env!("CARGO_PKG_VERSION")),
        Style::default().fg(MUTED),
    )));
    lines.push(Line::from(""));

    for entry in &app.chat_history {
        match entry {
            ChatEntry::TranscriptLine(s) => {
                lines.push(Line::from(Span::styled(
                    s.as_str(),
                    Style::default().fg(MUTED),
                )));
            }
            ChatEntry::UserMessage(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "●",
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " You: ",
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        text,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                lines.push(Line::from(""));
            }
            ChatEntry::AssistantMessage(text) => {
                let mut normalized: Vec<String> = Vec::new();
                let mut prev_blank = false;
                for raw in text.lines() {
                    let line = raw.trim_end();
                    let is_blank = line.trim().is_empty();
                    if is_blank {
                        if prev_blank {
                            continue;
                        }
                        prev_blank = true;
                        normalized.push(String::new());
                    } else {
                        prev_blank = false;
                        normalized.push(line.to_string());
                    }
                }

                while normalized
                    .first()
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(false)
                {
                    normalized.remove(0);
                }
                while normalized
                    .last()
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(false)
                {
                    normalized.pop();
                }
                if normalized.is_empty() {
                    continue;
                }

                // Never force a minimum width larger than viewport.
                let box_width = inner_width;
                if box_width < 8 {
                    for raw in &normalized {
                        for segment in wrap_to_width(raw, inner_width.max(1)) {
                            lines.push(Line::from(segment));
                        }
                    }
                    lines.push(Line::from(""));
                    continue;
                }

                let inner = box_width.saturating_sub(2);
                let body_width = inner.saturating_sub(2).max(1);
                let title = "─ Ferroclaw ";
                let title_pad = inner.saturating_sub(title.chars().count());
                lines.push(Line::from(Span::styled(
                    format!("╭{title}{}╮", "─".repeat(title_pad)),
                    Style::default()
                        .fg(Color::Rgb(172, 160, 255))
                        .add_modifier(Modifier::BOLD),
                )));

                for raw in &normalized {
                    for seg in wrap_to_width(raw, body_width) {
                        let pad = body_width.saturating_sub(seg.chars().count());
                        lines.push(Line::from(Span::raw(format!(
                            "│ {seg}{} │",
                            " ".repeat(pad)
                        ))));
                    }
                }

                lines.push(Line::from(Span::styled(
                    format!("╰{}╯", "─".repeat(inner)),
                    Style::default().fg(Color::Rgb(172, 160, 255)),
                )));
                lines.push(Line::from(""));
            }
            ChatEntry::ToolCall { name, args: _ } => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "◆ tool call: ",
                        Style::default().fg(Color::Rgb(202, 178, 116)),
                    ),
                    Span::styled(
                        name,
                        Style::default()
                            .fg(Color::Rgb(202, 178, 116))
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            ChatEntry::ToolResult {
                name,
                content,
                is_error,
            } => {
                let color = if *is_error {
                    Color::Rgb(255, 130, 130)
                } else {
                    Color::Rgb(120, 190, 155)
                };
                let label = if *is_error {
                    "tool error"
                } else {
                    "tool result"
                };
                lines.push(Line::from(vec![
                    Span::styled("↳ ", Style::default().fg(color)),
                    Span::styled(
                        format!("{label}: "),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(name, Style::default().fg(color)),
                ]));
                let max_source_lines = 8usize;
                let mut shown = 0usize;
                let mut capped = false;
                for (i, ln) in content.lines().enumerate() {
                    if i >= max_source_lines {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "    … ({} more lines)",
                                content.lines().count() - max_source_lines
                            ),
                            Style::default().fg(MUTED),
                        )));
                        break;
                    }

                    let wrap_width = inner_width.saturating_sub(4).max(1);
                    for seg in wrap_to_width(ln, wrap_width) {
                        lines.push(Line::from(Span::styled(
                            format!("    {seg}"),
                            Style::default().fg(color),
                        )));
                        shown += 1;
                        if shown >= 24 {
                            lines.push(Line::from(Span::styled(
                                "    … (output trimmed for display)",
                                Style::default().fg(MUTED),
                            )));
                            capped = true;
                            break;
                        }
                    }
                    if capped {
                        break;
                    }
                }
            }
            ChatEntry::SystemInfo(text) => {
                lines.push(Line::from(Span::styled(text, Style::default().fg(MUTED))));
            }
            ChatEntry::Error(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "ERROR: ",
                        Style::default()
                            .fg(Color::Rgb(255, 130, 130))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text, Style::default().fg(Color::Rgb(255, 130, 130))),
                ]));
            }
        }
    }

    // Bottom-anchor short transcripts so new chat appears near the composer.
    let visible = area.height as usize;
    if visible > 0 && lines.len() < visible {
        let mut padded = Vec::with_capacity(visible);
        padded.extend(std::iter::repeat_n(Line::from(""), visible - lines.len()));
        padded.extend(lines);
        lines = padded;
    }

    app.total_chat_lines = lines.len() as u16;
    app.visible_chat_height = area.height;

    let total = lines.len() as u16;
    let visible_u16 = app.visible_chat_height;
    let scroll = if total > visible_u16 {
        let max_offset = total - visible_u16;
        let from_bottom = app.scroll_offset.min(max_offset);
        max_offset - from_bottom
    } else {
        0
    };

    let chat = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(Clear, area);
    frame.render_widget(chat, area);
}

fn draw_slash_menu_popup(frame: &mut Frame, app: &App, input_area: Rect) {
    if app.slash_menu_items.is_empty() {
        return;
    }

    let popup_height = (app.slash_menu_items.len().min(8) as u16).saturating_add(2);
    let popup_width = input_area.width.clamp(28, 80);
    let popup_x = input_area.x;
    let popup_y = input_area.y.saturating_sub(popup_height);
    let popup = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    let start = app.slash_menu_scroll.min(app.slash_menu_items.len());
    let end = (start + popup_height.saturating_sub(2) as usize).min(app.slash_menu_items.len());
    let mut lines = Vec::new();
    for (idx, item) in app.slash_menu_items[start..end].iter().enumerate() {
        let absolute_idx = start + idx;
        let style = if absolute_idx == app.slash_menu_selected {
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TITLE)
        };
        lines.push(Line::from(Span::styled(item.clone(), style)));
    }

    let menu = Paragraph::new(Text::from(lines)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" / Commands ")
            .border_style(Style::default().fg(BORDER)),
    );

    frame.render_widget(Clear, popup);
    frame.render_widget(menu, popup);
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let visible_rows = area.height.saturating_sub(2) as usize;
    let max_cols = area.width.saturating_sub(2) as usize;
    let total_rows = app.input_lines.len();

    let start_row = if visible_rows == 0 {
        0
    } else {
        app.cursor_line
            .saturating_add(1)
            .saturating_sub(visible_rows)
    }
    .min(total_rows.saturating_sub(1));
    let end_row = (start_row + visible_rows).min(total_rows);

    // Horizontal viewport: keep cursor visible for very long single-line requests.
    let start_col = if max_cols == 0 {
        0
    } else {
        app.cursor_col.saturating_sub(max_cols.saturating_sub(1))
    };

    let input_text: Vec<Line> = app.input_lines[start_row..end_row]
        .iter()
        .map(|line: &String| {
            let visible = line
                .chars()
                .skip(start_col)
                .take(max_cols)
                .collect::<String>();
            Line::from(visible)
        })
        .collect();

    let input = Paragraph::new(Text::from(input_text)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Type your message... ")
            .border_style(Style::default().fg(BORDER)),
    );

    frame.render_widget(input, area);

    let cursor_line_in_view = app.cursor_line.saturating_sub(start_row) as u16;
    let cursor_col_in_view = app
        .cursor_col
        .saturating_sub(start_col)
        .min(max_cols.saturating_sub(1)) as u16;
    let cursor_x = area.x + 1 + cursor_col_in_view;
    let cursor_y = area.y + 1 + cursor_line_in_view;

    frame.set_cursor_position((
        cursor_x.min(area.x + area.width.saturating_sub(2)),
        cursor_y.min(area.y + area.height.saturating_sub(2)),
    ));
}
