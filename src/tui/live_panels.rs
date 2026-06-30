//! Bounded live snippet panels (thinking trace + diff) for the Hermes TUI.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Visible content rows inside the thinking panel (excluding borders/title).
pub const THINKING_PANEL_LINES: u16 = 6;
/// Visible content rows inside the diff panel (excluding borders/title).
pub const DIFF_PANEL_LINES: u16 = 8;

/// Max stored raw lines per panel (tail is rendered; prevents unbounded growth).
const MAX_STORED_LINES: usize = 400;

/// Rolling buffer for a fixed-height live snippet widget.
#[derive(Debug, Clone, Default)]
pub struct LiveSnippetBuffer {
    lines: Vec<String>,
    streaming_line: String,
}

impl LiveSnippetBuffer {
    pub fn clear(&mut self) {
        self.lines.clear();
        self.streaming_line.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.streaming_line.is_empty()
    }

    pub fn push_line(&mut self, line: impl Into<String>) {
        self.flush_streaming();
        let line = line.into();
        if line.is_empty() {
            return;
        }
        self.lines.push(line);
        self.trim_storage();
    }

    pub fn push_delta(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.streaming_line.push_str(delta);
        self.trim_storage();
    }

    pub fn flush_streaming(&mut self) {
        if !self.streaming_line.is_empty() {
            let line = std::mem::take(&mut self.streaming_line);
            self.lines.push(line);
            self.trim_storage();
        }
    }

    /// Snapshot all trace lines for archiving into a collapsed thought block.
    pub fn collect_all_lines(&mut self) -> Vec<String> {
        self.flush_streaming();
        self.lines.clone()
    }

    fn trim_storage(&mut self) {
        if self.lines.len() > MAX_STORED_LINES {
            let drop = self.lines.len() - MAX_STORED_LINES;
            self.lines.drain(0..drop);
        }
    }

    /// Flatten stored lines with wrapping, return the last `max_rows` for display.
    pub fn tail_rows(&self, width: usize, max_rows: usize) -> Vec<Line<'static>> {
        let mut wrapped: Vec<Line<'static>> = Vec::new();
        for raw in &self.lines {
            for seg in wrap_to_width(raw, width) {
                wrapped.push(Line::from(Span::raw(seg)));
            }
        }
        if !self.streaming_line.is_empty() {
            for seg in wrap_to_width(&self.streaming_line, width) {
                wrapped.push(Line::from(Span::styled(
                    seg,
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
        }
        if max_rows == 0 {
            return wrapped;
        }
        if wrapped.len() <= max_rows {
            return wrapped;
        }
        wrapped.split_off(wrapped.len() - max_rows)
    }
}

/// Push unified-diff-looking content into the diff buffer (line-by-line, styled later).
pub fn ingest_diff_text(buffer: &mut LiveSnippetBuffer, text: &str) {
    if !looks_like_unified_diff(text) {
        return;
    }
    for line in text.lines() {
        buffer.push_line(line.to_string());
    }
}

pub fn looks_like_unified_diff(text: &str) -> bool {
    let mut signals = 0u32;
    for line in text.lines().take(40) {
        if line.starts_with("diff --git ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("@@ ")
        {
            signals += 2;
        } else if line.starts_with('+') || line.starts_with('-') {
            signals += 1;
        }
    }
    signals >= 2
}

/// Style a single diff line for the diff panel.
pub fn style_diff_line(line: &str) -> Line<'static> {
    if line.starts_with("+++ ") || line.starts_with("--- ") || line.starts_with("diff --git") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Rgb(186, 201, 224)),
        ));
    }
    if line.starts_with("@@") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Rgb(112, 188, 255)),
        ));
    }
    if line.starts_with('+') {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Rgb(120, 190, 155)),
        ));
    }
    if line.starts_with('-') {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Rgb(255, 130, 130)),
        ));
    }
    Line::from(Span::styled(
        line.to_string(),
        Style::default().fg(Color::Rgb(120, 138, 168)),
    ))
}

pub fn style_trace_line(line: &str) -> Line<'static> {
    if line.contains("◦ reason:")
        || line.contains("thinking")
        || line.starts_with('⋯')
    {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(Color::Rgb(120, 138, 168))
                .add_modifier(Modifier::DIM),
        ));
    }
    if line.starts_with('→') || line.starts_with('←') || line.contains("tool") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Rgb(112, 188, 255)),
        ));
    }
    if line.starts_with('◆') || line.starts_with('⋯') {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Rgb(186, 201, 224)),
        ));
    }
    Line::from(Span::styled(
        line.to_string(),
        Style::default().fg(Color::Rgb(120, 138, 168)),
    ))
}

pub fn tail_trace_rows(buffer: &LiveSnippetBuffer, width: usize, max_rows: usize) -> Vec<Line<'static>> {
    buffer
        .tail_rows(width, max_rows)
        .into_iter()
        .map(|line| {
            let text = line
                .spans
                .first()
                .map(|s| s.content.clone())
                .unwrap_or_default()
                .to_string();
            style_trace_line(&text)
        })
        .collect()
}

pub fn tail_diff_rows(buffer: &LiveSnippetBuffer, width: usize, max_rows: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for raw in &buffer.lines {
        for seg in wrap_to_width(raw, width) {
            out.push(style_diff_line(&seg));
        }
    }
    if !buffer.streaming_line.is_empty() {
        for seg in wrap_to_width(&buffer.streaming_line, width) {
            out.push(style_diff_line(&seg));
        }
    }
    if max_rows == 0 {
        return out;
    }
    if out.len() <= max_rows {
        return out;
    }
    out.split_off(out.len() - max_rows)
}

fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    if text.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unified_diff() {
        let text = "--- a/foo\n+++ b/foo\n@@ -1 +1 @@\n-old\n+new\n";
        assert!(looks_like_unified_diff(text));
    }

    #[test]
    fn rejects_plain_text() {
        assert!(!looks_like_unified_diff("hello\nworld\n"));
    }

    #[test]
    fn tail_rows_keeps_last_n() {
        let mut buf = LiveSnippetBuffer::default();
        for i in 0..20 {
            buf.push_line(format!("line-{i}"));
        }
        let rows = buf.tail_rows(40, 3);
        assert_eq!(rows.len(), 3);
        assert!(rows[2].spans[0].content.contains("line-19"));
    }
}
