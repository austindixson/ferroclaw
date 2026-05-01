//! Orchestrator TUI: Hermes-style layout (3 panels) + nyx palette for tool transcript lines.
//!
//! Chat uses **pre-wrapped** one-row `Line`s (no `Paragraph::wrap`) so `scroll()` matches
//! visible rows — same pattern as [`super::hermes_ui`].

use crate::tui::app::{App, ChatEntry, TaskStatus};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

const TEAL: Color = Color::Rgb(0, 212, 170);
const AMBER: Color = Color::Rgb(251, 191, 36);
const EMERALD: Color = Color::Rgb(52, 211, 153);
const CYAN: Color = Color::Rgb(103, 232, 249);
const GRAY_300: Color = Color::Rgb(209, 213, 219);
const GRAY_500: Color = Color::Rgb(107, 114, 128);
const RED_LIGHT: Color = Color::Rgb(252, 165, 165);
const HERMES_ORANGE: Color = Color::Rgb(255, 107, 53);
const HERMES_ASSIST: Color = Color::Rgb(74, 222, 128); // green-400-ish
const TILE_BG: Color = Color::Rgb(19, 19, 26);
const TILE_BORDER: Color = Color::Rgb(30, 30, 42);
const TILE_HEADER: Color = Color::Rgb(22, 22, 31);
const CODE_FG: Color = Color::Rgb(207, 250, 254);

#[derive(Debug, Clone)]
enum ParsedSeg {
    Text(String),
    Code { lang: String, body: String },
}

fn parse_fenced_segments(input: &str) -> Vec<ParsedSeg> {
    let src = input.replace("\r\n", "\n");
    let lines: Vec<&str> = src.lines().collect();
    let mut out: Vec<ParsedSeg> = Vec::new();
    let mut in_code = false;
    let mut lang = String::new();
    let mut buf: Vec<String> = Vec::new();

    for line in lines {
        if let Some(stripped) = line.strip_prefix("```") {
            let rest = stripped.trim();
            if in_code {
                if !buf.is_empty() {
                    let body = buf.join("\n");
                    buf.clear();
                    out.push(ParsedSeg::Code {
                        lang: lang.clone(),
                        body,
                    });
                }
                in_code = false;
                lang.clear();
            } else {
                if !buf.is_empty() {
                    let content = buf.join("\n");
                    buf.clear();
                    out.push(ParsedSeg::Text(content));
                }
                in_code = true;
                lang = rest.to_string();
            }
            continue;
        }
        buf.push(line.to_string());
    }
    if in_code {
        if !buf.is_empty() {
            let body = buf.join("\n");
            out.push(ParsedSeg::Code {
                lang: lang.clone(),
                body,
            });
        }
    } else if !buf.is_empty() {
        out.push(ParsedSeg::Text(buf.join("\n")));
    }
    out
}

/// Split `s` into chunks of at most `max_chars` Unicode scalars (one terminal row each).
fn chunk_string_chars(s: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut n = 0usize;
    for ch in s.chars() {
        if n >= max_chars && !buf.is_empty() {
            out.push(buf);
            buf = String::new();
            n = 0;
        }
        buf.push(ch);
        n += 1;
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn push_wrapped_styled(lines: &mut Vec<Line>, s: &str, width: usize, style: Style) {
    for part in chunk_string_chars(s, width.max(1)) {
        lines.push(Line::from(Span::styled(part, style)));
    }
}

fn transcript_style(line: &str) -> Style {
    let t = line.trim_start();
    let base = Style::default().fg(GRAY_300);
    if t.starts_with("[Error]") || t.starts_with("ERROR") {
        return base.fg(RED_LIGHT);
    }
    if t.starts_with('→') {
        return base.fg(AMBER);
    }
    if t.starts_with('←') {
        return base.fg(EMERALD);
    }
    if t.starts_with('◆') {
        return base.fg(CYAN);
    }
    if t.starts_with('⋯') {
        return base.fg(GRAY_500);
    }
    if t.starts_with('>') {
        return base.fg(GRAY_300);
    }
    if t.starts_with('[') && t.contains("s]") && t.contains("Still waiting") {
        return base.fg(GRAY_500);
    }
    base
}

/// Hermes-style layout: sidebar | (chat + status above input + input). Status sits directly above the text box.
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

    // Content: chat fills space; status bar sits directly above the input (like Claude Code)
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Chat area
            Constraint::Length(1), // Status bar — directly above input
            Constraint::Length(5), // Input area
        ])
        .split(main_chunks[1]);

    draw_sidebar_header(frame, app, sidebar_chunks[0]);
    draw_sidebar_tasks(frame, app, sidebar_chunks[1]);
    draw_sidebar_footer(frame, app, sidebar_chunks[2]);
    draw_chat(frame, app, content_chunks[0]);
    draw_status_bar(frame, app, content_chunks[1]);
    draw_input(frame, app, content_chunks[2]);
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
                .border_style(Style::default().fg(TEAL)),
        )
        .style(Style::default().bg(TILE_HEADER).fg(GRAY_300))
        .centered();

    frame.render_widget(header, area);
}

/// Scrollable task list in the sidebar.
fn draw_sidebar_tasks(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.tasks.is_empty() {
        let empty_msg = Paragraph::new("No tasks yet.\nPress 'n' to add a task.")
            .style(Style::default().fg(GRAY_500))
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
                TaskStatus::Pending => ("□ ", Style::default().fg(GRAY_300)),
                TaskStatus::InProgress => ("◉ ", Style::default().fg(AMBER)),
                TaskStatus::Completed => ("✓ ", Style::default().fg(EMERALD)),
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

            // Show description if this task is selected
            if idx == app.selected_task_index && !task.description.is_empty() {
                let desc_lines: Vec<Line> = task
                    .description
                    .lines()
                    .map(|line| {
                        Line::from(Span::styled(
                            format!("    │ {}", line),
                            Style::default().fg(GRAY_500),
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(TILE_BORDER))
                .bg(TILE_BG),
        )
        .style(Style::default().bg(TILE_BG))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    frame.render_widget(task_list, area);
}

/// Sidebar footer with keyboard shortcuts.
fn draw_sidebar_footer(frame: &mut Frame, _app: &App, area: Rect) {
    let footer_text = " n:new d:del c:cmp ↑↓:nav ";
    let footer = Paragraph::new(footer_text).style(Style::default().bg(TILE_HEADER).fg(GRAY_500));
    frame.render_widget(footer, area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let indicator = if app.is_running { "●" } else { "○" };
    let ind_color = if app.is_running { TEAL } else { GRAY_500 };

    let last_usage = if app.last_input_tokens > 0 || app.last_output_tokens > 0 {
        format!(
            " | last in={} out={}",
            app.last_input_tokens, app.last_output_tokens
        )
    } else {
        String::new()
    };

    let mut spans = vec![
        Span::styled(
            format!(" {indicator} "),
            Style::default().fg(ind_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "ferroclaw",
            Style::default().fg(TEAL).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " | {} | tokens: {}/{}",
                app.model_name, app.tokens_used, app.token_budget
            ),
            Style::default().fg(GRAY_300),
        ),
    ];
    if app.iteration > 0 {
        spans.push(Span::styled(
            format!(" · {}", app.iteration),
            Style::default().fg(GRAY_500),
        ));
    }
    spans.push(Span::styled(" | ", Style::default().fg(GRAY_500)));
    spans.push(Span::styled(
        app.verb.as_str(),
        Style::default().fg(GRAY_300).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(" | wheel/PgUp/PgDn scroll · Ctrl+L clear · Ctrl+C quit{last_usage}"),
        Style::default().fg(GRAY_500),
    ));

    let status =
        Paragraph::new(Line::from(spans)).style(Style::default().bg(TILE_HEADER).fg(GRAY_300));
    frame.render_widget(status, area);
}

fn render_assistant_segments(text: &str, inner_width: usize) -> Vec<Line<'_>> {
    let mut lines: Vec<Line> = Vec::new();
    for seg in parse_fenced_segments(text) {
        match seg {
            ParsedSeg::Text(t) if !t.is_empty() => {
                for line in t.lines() {
                    let row = format!("  {line}");
                    push_wrapped_styled(
                        &mut lines,
                        &row,
                        inner_width.max(1),
                        Style::default().fg(GRAY_300),
                    );
                }
            }
            ParsedSeg::Code { lang, body } => {
                let label = if lang.is_empty() { "code" } else { &lang };
                lines.push(Line::from(Span::styled(
                    format!("  ─── {} ───", label.to_uppercase()),
                    Style::default().fg(CYAN),
                )));
                let w = inner_width.saturating_sub(4).max(1);
                for line in body.lines() {
                    let chars: Vec<char> = line.chars().collect();
                    for chunk in chars.chunks(w) {
                        let part: String = chunk.iter().collect();
                        lines.push(Line::from(Span::styled(
                            format!("    {part}"),
                            Style::default().fg(CODE_FG),
                        )));
                    }
                }
            }
            _ => {}
        }
    }
    if lines.is_empty() && !text.is_empty() {
        for line in text.lines() {
            let row = format!("  {line}");
            push_wrapped_styled(
                &mut lines,
                &row,
                inner_width.max(1),
                Style::default().fg(GRAY_300),
            );
        }
    }
    lines
}

fn draw_chat(frame: &mut Frame, app: &mut App, area: Rect) {
    let inner_width = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    for entry in &app.chat_history {
        match entry {
            ChatEntry::UserMessage(text) => {
                const PREFIX: &str = "● You: ";
                let prefix_cols = PREFIX.chars().count();
                let content_width = inner_width.saturating_sub(prefix_cols).max(1);
                for (i, line) in text.lines().enumerate() {
                    if i == 0 {
                        let chunks = chunk_string_chars(line, content_width);
                        for (j, chunk) in chunks.into_iter().enumerate() {
                            if j == 0 {
                                lines.push(Line::from(vec![
                                    Span::styled(
                                        "●",
                                        Style::default()
                                            .fg(HERMES_ORANGE)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        " You: ",
                                        Style::default().fg(GRAY_300).add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(chunk, Style::default().fg(GRAY_300)),
                                ]));
                            } else {
                                lines.push(Line::from(Span::styled(
                                    format!("       {chunk}"),
                                    Style::default().fg(GRAY_300),
                                )));
                            }
                        }
                    } else {
                        let row = format!("       {line}");
                        push_wrapped_styled(
                            &mut lines,
                            &row,
                            inner_width.max(1),
                            Style::default().fg(GRAY_300),
                        );
                    }
                }
            }
            ChatEntry::TranscriptLine(s) => {
                let st = transcript_style(s);
                push_wrapped_styled(&mut lines, s, inner_width.max(1), st);
            }
            ChatEntry::AssistantMessage(text) => {
                lines.push(Line::from(vec![Span::styled(
                    "Ferroclaw: ",
                    Style::default()
                        .fg(HERMES_ASSIST)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.extend(render_assistant_segments(text, inner_width));
            }
            ChatEntry::ToolCall { name, args } => {
                let preview = if args.is_empty() {
                    String::new()
                } else {
                    format!("({})", args_preview(args, 120))
                };
                let row = format!("→ {name}{preview}");
                push_wrapped_styled(
                    &mut lines,
                    &row,
                    inner_width.max(1),
                    Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
                );
            }
            ChatEntry::ToolResult {
                name,
                content,
                is_error,
            } => {
                let color = if *is_error { RED_LIGHT } else { EMERALD };
                let row = format!("← {} [{}]", name, if *is_error { "ERR" } else { "OK" });
                push_wrapped_styled(
                    &mut lines,
                    &row,
                    inner_width.max(1),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                );
                if *is_error && !content.is_empty() {
                    for line in content.lines().take(8) {
                        let r = format!("    {line}");
                        push_wrapped_styled(
                            &mut lines,
                            &r,
                            inner_width.max(1),
                            Style::default().fg(RED_LIGHT),
                        );
                    }
                }
            }
            ChatEntry::SystemInfo(text) => {
                push_wrapped_styled(
                    &mut lines,
                    text,
                    inner_width.max(1),
                    Style::default().fg(CYAN).add_modifier(Modifier::ITALIC),
                );
            }
            ChatEntry::Error(text) => {
                let row = format!("[Error] {text}");
                push_wrapped_styled(
                    &mut lines,
                    &row,
                    inner_width.max(1),
                    Style::default().fg(RED_LIGHT),
                );
            }
        }
        lines.push(Line::from(""));
    }

    app.total_chat_lines = lines.len() as u16;
    app.visible_chat_height = area.height.saturating_sub(2);

    let total = lines.len() as u16;
    let visible = app.visible_chat_height;
    let scroll = if total > visible {
        let max_offset = total - visible;
        let from_bottom = app.scroll_offset.min(max_offset);
        max_offset - from_bottom
    } else {
        0
    };

    let chat = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(TILE_BORDER))
                .bg(TILE_BG)
                .title(Span::styled(
                    " Chat ",
                    Style::default().fg(TEAL).add_modifier(Modifier::BOLD),
                )),
        )
        .style(Style::default().bg(TILE_BG))
        .scroll((scroll, 0));

    frame.render_widget(chat, area);
}

fn args_preview(raw: &str, max: usize) -> String {
    let raw = raw.trim();
    if raw.len() <= max {
        raw.to_string()
    } else {
        format!("{}…", raw.chars().take(max).collect::<String>())
    }
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_text: Vec<Line> = app
        .input_lines
        .iter()
        .map(|l: &String| Line::from(Span::styled(l.as_str(), Style::default().fg(GRAY_300))))
        .collect();

    let input = Paragraph::new(Text::from(input_text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(TEAL))
                .bg(TILE_BG)
                .title(Span::styled(
                    " Type your message… (Enter send · Shift+Enter newline) ",
                    Style::default().fg(GRAY_500),
                )),
        )
        .style(Style::default().bg(TILE_BG));

    frame.render_widget(input, area);

    let cursor_x = area.x + 1 + app.cursor_col as u16;
    let cursor_y = area.y + 1 + app.cursor_line as u16;
    if cursor_x < area.x + area.width - 1 && cursor_y < area.y + area.height - 1 {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
