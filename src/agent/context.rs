//! Context window management: token tracking, pruning, and budget enforcement.

use crate::types::Message;

/// Manages the conversation context within token budget constraints.
pub struct ContextManager {
    pub token_budget: u64,
    pub tokens_used: u64,
    /// Minimum messages to preserve (system + last N user/assistant pairs)
    pub min_preserve: usize,
}

impl ContextManager {
    pub fn new(token_budget: u64) -> Self {
        Self {
            token_budget,
            tokens_used: 0,
            min_preserve: 6,
        }
    }

    /// Estimate total tokens for a message list.
    pub fn estimate_total(messages: &[Message]) -> u64 {
        messages.iter().map(|m| m.estimated_tokens()).sum()
    }

    /// Check if adding a message would exceed the budget.
    pub fn would_exceed(&self, messages: &[Message], new_msg: &Message) -> bool {
        let current = Self::estimate_total(messages);
        let additional = new_msg.estimated_tokens();
        current + additional > self.token_budget
    }

    /// Prune messages to fit within budget. Preserves system messages and
    /// the most recent conversation turns. Removes from the middle.
    pub fn prune_to_fit(&self, messages: &mut Vec<Message>) {
        let total = Self::estimate_total(messages);
        if total <= self.token_budget {
            return;
        }

        let target = (self.token_budget as f64 * 0.8) as u64; // Prune to 80% of budget

        // Separate system messages (always kept) from conversation
        let system_count = messages
            .iter()
            .take_while(|m| m.role == crate::types::Role::System)
            .count();

        // Keep system messages + first 2 user messages + last N messages
        let keep_start = system_count + 2;
        let keep_end = self.min_preserve;

        if messages.len() <= keep_start + keep_end {
            return; // Can't prune further
        }

        // Remove messages from the middle until we're under target
        let mut current_total = total;
        let mut to_remove = Vec::new();

        for i in (keep_start..messages.len().saturating_sub(keep_end)).rev() {
            if current_total <= target {
                break;
            }
            current_total -= messages[i].estimated_tokens();
            to_remove.push(i);
        }

        // Insert a structured compaction summary where we pruned.
        if !to_remove.is_empty() {
            let removed_count = to_remove.len();
            let removed_messages: Vec<Message> = to_remove
                .iter()
                .rev()
                .filter_map(|i| messages.get(*i).cloned())
                .collect();

            for i in to_remove.into_iter() {
                messages.remove(i);
            }

            if keep_start < messages.len() {
                messages.insert(
                    keep_start,
                    Message::system(build_compaction_summary(removed_count, &removed_messages)),
                );
            }
        }
    }

    /// Update token usage tracking from provider response.
    pub fn record_usage(&mut self, input_tokens: u64, output_tokens: u64) {
        self.tokens_used += input_tokens + output_tokens;
    }

    /// Remaining token budget.
    pub fn remaining(&self) -> u64 {
        self.token_budget.saturating_sub(self.tokens_used)
    }

    /// Fraction of budget consumed.
    pub fn usage_fraction(&self) -> f64 {
        if self.token_budget == 0 {
            return 1.0;
        }
        self.tokens_used as f64 / self.token_budget as f64
    }
}

fn build_compaction_summary(removed_count: usize, removed: &[Message]) -> String {
    let active_task = collect_role_hints(removed, crate::types::Role::User, 3);
    let key_decisions = collect_role_hints(removed, crate::types::Role::Assistant, 3);
    let pending_asks = collect_pending_asks(removed, 3);
    let critical_context = collect_critical_context(removed, removed_count, 4);

    let mut out = String::new();
    out.push_str("[Context compaction summary]\n");

    out.push_str("## Active Task\n");
    append_summary_items(&mut out, &active_task);

    out.push_str("## Key Decisions\n");
    append_summary_items(&mut out, &key_decisions);

    out.push_str("## Pending Asks\n");
    append_summary_items(&mut out, &pending_asks);

    out.push_str("## Critical Context\n");
    append_summary_items(&mut out, &critical_context);

    out
}

fn collect_role_hints(messages: &[Message], role: crate::types::Role, limit: usize) -> Vec<String> {
    messages
        .iter()
        .filter(|m| m.role == role)
        .filter_map(|m| first_non_empty_line(m.text()))
        .map(|s| truncate_for_summary(&s, 140))
        .take(limit)
        .collect()
}

fn append_summary_items(out: &mut String, items: &[String]) {
    if items.is_empty() {
        out.push_str("- (none)\n");
    } else {
        for item in items {
            out.push_str(&format!("- {item}\n"));
        }
    }
}

fn collect_pending_asks(messages: &[Message], limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    for m in messages
        .iter()
        .filter(|m| m.role == crate::types::Role::User)
    {
        if let Some(line) = first_non_empty_line(m.text()) {
            let lower = line.to_ascii_lowercase();
            if line.contains('?')
                || lower.contains("please")
                || lower.contains("need")
                || lower.contains("follow up")
                || lower.contains("next")
                || lower.contains("can you")
            {
                out.push(truncate_for_summary(&line, 140));
            }
        }
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn collect_critical_context(
    messages: &[Message],
    removed_count: usize,
    limit: usize,
) -> Vec<String> {
    let mut out = vec![format!("Removed messages: {removed_count}")];

    for m in messages {
        if let Some(line) = first_non_empty_line(m.text()) {
            let lower = line.to_ascii_lowercase();
            if lower.contains("error")
                || lower.contains("failed")
                || lower.contains("limit")
                || lower.contains("429")
                || lower.contains("token")
                || lower.contains("budget")
                || lower.contains("path")
                || lower.contains("config")
                || line.chars().any(|c| c.is_ascii_digit())
            {
                out.push(truncate_for_summary(&line, 140));
            }
        }
        if out.len() >= limit {
            break;
        }
    }

    out
}

fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn truncate_for_summary(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    #[test]
    fn test_estimate_total() {
        let msgs = vec![
            Message::system("You are helpful."),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];
        let total = ContextManager::estimate_total(&msgs);
        assert!(total > 0);
    }

    #[test]
    fn test_prune_to_fit() {
        let ctx = ContextManager::new(100); // Very small budget

        let mut msgs: Vec<Message> = Vec::new();
        msgs.push(Message::system("System prompt"));
        for i in 0..20 {
            msgs.push(Message::user(format!(
                "Message {i} with some content padding"
            )));
            msgs.push(Message::assistant(format!(
                "Response {i} with more padding text"
            )));
        }

        let original_len = msgs.len();
        ctx.prune_to_fit(&mut msgs);
        assert!(msgs.len() < original_len);
    }

    #[test]
    fn test_compaction_inserts_structured_summary_marker() {
        let ctx = ContextManager::new(40);

        let mut msgs: Vec<Message> = vec![Message::system("System prompt")];
        let pad = " lorem ipsum dolor sit amet, consectetur adipiscing elit";
        msgs.push(Message::user(format!(
            "Task: inspect OCR failure and limit errors.{pad}{pad}"
        )));
        msgs.push(Message::assistant(format!(
            "I will inspect logs and provider responses.{pad}{pad}"
        )));
        msgs.push(Message::user(format!(
            "Install pytesseract and run OCR on temp images.{pad}{pad}"
        )));
        msgs.push(Message::assistant(format!(
            "Tool result: tesseract not found in PATH.{pad}{pad}"
        )));
        msgs.push(Message::user(format!(
            "Please harden retries and rate limiting.{pad}{pad}"
        )));
        msgs.push(Message::assistant(format!(
            "Added retry-after handling for 429 responses.{pad}{pad}"
        )));
        msgs.push(Message::user(format!(
            "Now I see 210604/200000 in status strip.{pad}{pad}"
        )));
        msgs.push(Message::assistant(format!(
            "That indicates local token budget exhaustion.{pad}{pad}"
        )));
        msgs.push(Message::user(format!(
            "Please apply Hermes-style context compaction.{pad}{pad}"
        )));

        let total = ContextManager::estimate_total(&msgs);
        assert!(total > 40, "expected total tokens > budget, got {total}");
        ctx.prune_to_fit(&mut msgs);

        let summary = msgs
            .iter()
            .find(|m| {
                m.role == crate::types::Role::System
                    && m.text().contains("Context compaction summary")
            })
            .expect("structured compaction summary should be inserted")
            .text()
            .to_string();

        assert!(summary.contains("## Active Task"));
        assert!(summary.contains("## Key Decisions"));
        assert!(summary.contains("## Pending Asks"));
        assert!(summary.contains("## Critical Context"));
    }

    #[test]
    fn test_remaining_budget() {
        let mut ctx = ContextManager::new(1000);
        assert_eq!(ctx.remaining(), 1000);
        ctx.record_usage(100, 50);
        assert_eq!(ctx.remaining(), 850);
    }
}
