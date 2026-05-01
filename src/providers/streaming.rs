// SSE stream parsing utilities for streaming LLM responses.

/// Parse SSE lines from a byte stream into structured events.
pub fn parse_sse_lines(raw: &str) -> Vec<SseLine> {
    let mut lines = Vec::new();
    let mut current_event = None;
    let mut current_data = Vec::new();

    for line in raw.lines() {
        if let Some(stripped) = line.strip_prefix("event: ") {
            current_event = Some(stripped.to_string());
        } else if let Some(stripped) = line.strip_prefix("data: ") {
            current_data.push(stripped.to_string());
        } else if line.is_empty() && !current_data.is_empty() {
            lines.push(SseLine {
                event: current_event.take(),
                data: current_data.join("\n"),
            });
            current_data.clear();
        }
    }

    // Handle final chunk without trailing newline
    if !current_data.is_empty() {
        lines.push(SseLine {
            event: current_event.take(),
            data: current_data.join("\n"),
        });
    }

    lines
}

#[derive(Debug, Clone)]
pub struct SseLine {
    pub event: Option<String>,
    pub data: String,
}

/// Accumulator for building a complete response from streaming chunks.
#[derive(Debug, Default)]
pub struct StreamAccumulator {
    pub text: String,
    pub tool_calls: Vec<StreamingToolCall>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StreamingToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append_text(&mut self, delta: &str) {
        self.text.push_str(delta);
    }

    pub fn start_tool_call(&mut self, id: String, name: String) {
        self.tool_calls.push(StreamingToolCall {
            id,
            name,
            arguments_json: String::new(),
        });
    }

    pub fn append_tool_arguments(&mut self, delta: &str) {
        if let Some(tc) = self.tool_calls.last_mut() {
            tc.arguments_json.push_str(delta);
        }
    }

    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// Simple token estimator (chars / 4). Used for budget tracking
/// when the API doesn't return usage stats.
pub fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64) / 4 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_lines() {
        let raw = "event: message_start\ndata: {\"type\":\"message_start\"}\n\nevent: content_block_delta\ndata: {\"delta\":{\"text\":\"Hello\"}}\n\n";
        let lines = parse_sse_lines(raw);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].event.as_deref(), Some("message_start"));
        assert_eq!(lines[1].event.as_deref(), Some("content_block_delta"));
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("Hello, world!"), 4); // 13 chars / 4 + 1
        assert_eq!(estimate_tokens(""), 1);
    }

    #[test]
    fn test_stream_accumulator() {
        let mut acc = StreamAccumulator::new();
        acc.append_text("Hello ");
        acc.append_text("world");
        assert_eq!(acc.text, "Hello world");
        assert!(!acc.has_tool_calls());

        acc.start_tool_call("tc_1".into(), "read_file".into());
        acc.append_tool_arguments("{\"path\":");
        acc.append_tool_arguments("\"/tmp\"}");
        assert!(acc.has_tool_calls());
        assert_eq!(acc.tool_calls[0].arguments_json, "{\"path\":\"/tmp\"}");
    }
}
