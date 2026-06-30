//! Input composer: paste coalescing and debounced submit for dictation tools (e.g. Wispr Flow).
//!
//! Voice dictation often injects `Enter` after each word on push-to-talk release. Immediate
//! submit would start one agent turn per word. We wait for a short quiet period after the
//! last `Enter` before sending, and merge paste bursts that arrive back-to-back.

use std::time::{Duration, Instant};

/// Gap between paste chunks that are still one dictation utterance.
pub const PASTE_COALESCE_GAP_MS: u64 = 120;
/// Quiet period after Enter before we actually send (dictation may fire Enter per word).
pub const SUBMIT_QUIET_MS: u64 = 650;

/// Accumulates rapid `Paste` events into a single insert.
#[derive(Debug, Default)]
pub struct PasteCoalescer {
    pending: String,
    last_at: Option<Instant>,
}

impl PasteCoalescer {
    pub fn clear(&mut self) {
        self.pending.clear();
        self.last_at = None;
    }

    /// Push a paste chunk. Returns text that should be inserted *before* holding `chunk`,
    /// when a previous burst has ended.
    pub fn push(&mut self, chunk: &str, now: Instant) -> Option<String> {
        if chunk.is_empty() {
            return self.flush_expired(now);
        }

        if let Some(last) = self.last_at {
            if now.duration_since(last) < Duration::from_millis(PASTE_COALESCE_GAP_MS) {
                self.pending.push_str(chunk);
                self.last_at = Some(now);
                return None;
            }
            let flush = std::mem::take(&mut self.pending);
            self.pending = chunk.to_string();
            self.last_at = Some(now);
            return if flush.is_empty() { None } else { Some(flush) };
        }

        self.pending = chunk.to_string();
        self.last_at = Some(now);
        None
    }

    /// Flush pending paste if the coalesce window has elapsed.
    pub fn flush_expired(&mut self, now: Instant) -> Option<String> {
        let last = self.last_at?;
        if now.duration_since(last) < Duration::from_millis(PASTE_COALESCE_GAP_MS) {
            return None;
        }
        if self.pending.is_empty() {
            self.last_at = None;
            return None;
        }
        self.last_at = None;
        Some(std::mem::take(&mut self.pending))
    }
}

/// Debounced submit after Enter (reset when the user keeps typing/pasting).
#[derive(Debug, Default)]
pub struct SubmitDebounce {
    deadline: Option<Instant>,
}

impl SubmitDebounce {
    pub fn clear(&mut self) {
        self.deadline = None;
    }

    pub fn is_pending(&self) -> bool {
        self.deadline.is_some()
    }

    /// Enter pressed: send only after quiet period; repeated Enter extends the window.
    pub fn schedule(&mut self, now: Instant) {
        self.deadline = Some(now + Duration::from_millis(SUBMIT_QUIET_MS));
    }

    pub fn cancel(&mut self) {
        self.deadline = None;
    }

    pub fn ready(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.deadline else {
            return false;
        };
        if now >= deadline {
            self.deadline = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_coalescer_merges_rapid_chunks() {
        let mut c = PasteCoalescer::default();
        let t0 = Instant::now();
        assert!(c.push("hello ", t0).is_none());
        assert!(c.push("world", t0 + Duration::from_millis(30)).is_none());
        let out = c.flush_expired(t0 + Duration::from_millis(200)).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn paste_coalescer_flushes_gap() {
        let mut c = PasteCoalescer::default();
        let t0 = Instant::now();
        assert!(c.push("one", t0).is_none());
        let flushed = c.push("two", t0 + Duration::from_millis(500));
        assert_eq!(flushed.as_deref(), Some("one"));
        assert_eq!(c.pending, "two");
    }

    #[test]
    fn submit_debounce_waits_then_fires() {
        let mut d = SubmitDebounce::default();
        let t0 = Instant::now();
        d.schedule(t0);
        assert!(!d.ready(t0 + Duration::from_millis(100)));
        assert!(d.ready(t0 + Duration::from_millis(SUBMIT_QUIET_MS + 10)));
        assert!(!d.is_pending());
    }

    #[test]
    fn submit_debounce_cancelled_by_typing() {
        let mut d = SubmitDebounce::default();
        let t0 = Instant::now();
        d.schedule(t0);
        d.cancel();
        assert!(!d.ready(t0 + Duration::from_millis(SUBMIT_QUIET_MS + 10)));
    }
}
