//! Conversation adapters — pluggable backends that drive a chat
//! response.
//!
//! Three impls planned (one shipped this turn):
//!
//! | Adapter         | Backend              | When used                           |
//! |-----------------|----------------------|-------------------------------------|
//! | `StubAdapter`   | canned responses     | tests, demo button (this turn)      |
//! | `LocalLlmAdapter` | Ollama / OpenAI-compat | local mode, no Hermon login         |
//! | `HermonAdapter`   | Hermon `/v1/conversations/.../send` | logged in, full features      |
//!
//! The trait signature is intentionally minimal: take a snapshot
//! of the conversation, take a new user message, and a callback for
//! streaming chunks. Adapters own their own threading model — they
//! can spawn a task or compute synchronously, the caller doesn't
//! care.

use super::types::{Conversation, StreamChunk};

/// Sink for streaming chunks. Boxed dyn so adapters don't carry
/// generic params, which would force the manager to monomorphize.
pub type ChunkSink = Box<dyn Fn(StreamChunk) + Send + Sync>;

/// Pluggable conversation backend.
///
/// `send` is non-async and non-Result: the adapter is expected to
/// emit `StreamChunk::Error { message }` through the sink for
/// errors, and `StreamChunk::Done` for normal completion. Keeping
/// the signature simple makes it easy to test with a synchronous
/// stub and easy to spawn from any context.
pub trait ConversationAdapter: Send + Sync {
    /// Send `new_user_message` against the conversation history.
    /// Adapters MUST emit either `StreamChunk::Done` or
    /// `StreamChunk::Error` exactly once at the end, so the
    /// manager can finalize the in-flight state.
    fn send(&self, conversation: &Conversation, new_user_message: &str, sink: ChunkSink);

    /// Human-readable label for the active backend. Drives the
    /// "Running on Ollama / llama3.2:3b" indicator in the chat
    /// header.
    fn label(&self) -> String;
}

// ── Stub adapter ─────────────────────────────────────────────────────

/// Canned-response adapter. Used by tests and by the
/// "Try it out" demo button on the chat page until real
/// Hermon/local-LLM adapters are wired.
///
/// Echoes the user message with a fixed prefix and emits one
/// `Done` chunk. No tool use, no streaming delays.
#[derive(Debug, Default, Clone)]
pub struct StubAdapter {
    /// Optional prefix prepended to the echoed reply. Defaults to
    /// `"You said: "`.
    prefix: Option<String>,
}

impl StubAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: Some(prefix.into()),
        }
    }
}

impl ConversationAdapter for StubAdapter {
    fn send(&self, _conversation: &Conversation, new_user_message: &str, sink: ChunkSink) {
        let prefix = self.prefix.as_deref().unwrap_or("You said: ").to_string();
        sink(StreamChunk::TextDelta {
            delta: format!("{prefix}{new_user_message}"),
        });
        sink(StreamChunk::Done);
    }

    fn label(&self) -> String {
        "stub".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::ConversationId;
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Spy sink: collects chunks into a vector for assertions.
    fn make_spy() -> (ChunkSink, Arc<Mutex<Vec<StreamChunk>>>) {
        let log: Arc<Mutex<Vec<StreamChunk>>> = Arc::new(Mutex::new(Vec::new()));
        let log_for_sink = log.clone();
        let sink: ChunkSink = Box::new(move |c| {
            log_for_sink.lock().unwrap().push(c);
        });
        (sink, log)
    }

    #[test]
    fn stub_adapter_echoes_with_default_prefix() {
        let (sink, log) = make_spy();
        let adapter = StubAdapter::new();
        let conv = Conversation::new(ConversationId::new("c"), "x".into());
        adapter.send(&conv, "hello", sink);
        let chunks = log.lock().unwrap();
        assert_eq!(chunks.len(), 2);
        assert!(matches!(
            &chunks[0],
            StreamChunk::TextDelta { delta } if delta == "You said: hello"
        ));
        assert!(matches!(&chunks[1], StreamChunk::Done));
    }

    #[test]
    fn stub_adapter_uses_custom_prefix() {
        let (sink, log) = make_spy();
        let adapter = StubAdapter::with_prefix("Echo: ");
        let conv = Conversation::new(ConversationId::new("c"), "x".into());
        adapter.send(&conv, "hi", sink);
        let chunks = log.lock().unwrap();
        assert!(matches!(
            &chunks[0],
            StreamChunk::TextDelta { delta } if delta == "Echo: hi"
        ));
    }

    #[test]
    fn stub_adapter_label_identifies_itself() {
        let adapter = StubAdapter::new();
        assert_eq!(adapter.label(), "stub");
    }

    #[test]
    fn stub_adapter_emits_done_exactly_once() {
        let (sink, log) = make_spy();
        let adapter = StubAdapter::new();
        let conv = Conversation::new(ConversationId::new("c"), "x".into());
        adapter.send(&conv, "x", sink);
        let chunks = log.lock().unwrap();
        let dones = chunks
            .iter()
            .filter(|c| matches!(c, StreamChunk::Done))
            .count();
        assert_eq!(dones, 1);
    }
}
