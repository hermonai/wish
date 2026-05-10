//! [`ConversationManagerModel`] — singleton owning the set of
//! Wish chat conversations.
//!
//! # Design
//!
//! Mirrors [`crate::ai::agent_registry`] and
//! [`crate::ai::agent_tasks`] — one singleton, granular events,
//! pure-data state. The chat view, conversation-list panel, and any
//! future "recent conversations" surface all subscribe here.
//!
//! Conversations live in memory and are flushed to disk (or to
//! Hermon when configured) by a separate persistence adapter — the
//! manager itself has no I/O. This keeps the model trivially
//! testable and lets us swap storage backends without touching
//! conversation logic.

use std::collections::HashMap;
use std::sync::Arc;

use wishui::{Entity, ModelContext, SingletonEntity};

use super::adapter::ConversationAdapter;
use super::types::{Conversation, ConversationId, MessageBlock, StreamChunk, Turn};

// ── Events ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ConversationManagerEvent {
    /// A new conversation was created.
    Created { id: ConversationId },
    /// A turn was appended to a conversation.
    TurnAppended { id: ConversationId },
    /// A streaming chunk arrived for a conversation. The view
    /// renders the chunk into the in-flight assistant turn.
    Chunk {
        id: ConversationId,
        chunk: StreamChunk,
    },
    /// The active conversation switched.
    ActiveChanged { id: Option<ConversationId> },
    /// A conversation was removed.
    Removed { id: ConversationId },
}

// ── State of an in-flight assistant response ─────────────────────────

/// Per-conversation in-flight stream state. The view uses this to
/// render the assistant's response as it arrives, before the final
/// `Turn::Assistant` is committed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InFlight {
    /// Accumulated text deltas. Cleared when the response finalizes.
    pub text_buf: String,
    /// Accumulated thinking deltas.
    pub thinking_buf: String,
    /// Tool-use blocks emitted so far this turn.
    pub blocks_so_far: Vec<MessageBlock>,
    /// Whether the stream is currently active.
    pub streaming: bool,
}

impl InFlight {
    /// Apply a chunk to the in-flight state.
    /// Pure function — does not touch any context. Returns whether
    /// the chunk should commit a turn (i.e., a `Done` arrived).
    pub fn apply(&mut self, chunk: &StreamChunk) -> bool {
        match chunk {
            StreamChunk::TextDelta { delta } => {
                // Switching from a thinking run to a text run —
                // flush thinking first so block order matches the
                // wire order: thinking blocks appear before the
                // text they led to.
                self.flush_thinking_to_blocks();
                self.text_buf.push_str(delta);
                self.streaming = true;
                false
            }
            StreamChunk::ThinkingDelta { delta } => {
                // Symmetric: if text was streaming and thinking
                // resumes, flush text first.
                self.flush_text_to_blocks();
                self.thinking_buf.push_str(delta);
                self.streaming = true;
                false
            }
            StreamChunk::ToolUse {
                task_id,
                tool_name,
                input_json,
            } => {
                // Flush any pending text first so blocks stay
                // ordered.
                self.flush_text_to_blocks();
                self.flush_thinking_to_blocks();
                self.blocks_so_far.push(MessageBlock::ToolUse {
                    task_id: task_id.clone(),
                    tool_name: tool_name.clone(),
                    input_json: input_json.clone(),
                });
                self.streaming = true;
                false
            }
            StreamChunk::ToolResult {
                task_id,
                output,
                is_error,
            } => {
                self.flush_text_to_blocks();
                self.flush_thinking_to_blocks();
                self.blocks_so_far.push(MessageBlock::ToolResult {
                    task_id: task_id.clone(),
                    output: output.clone(),
                    is_error: *is_error,
                });
                self.streaming = true;
                false
            }
            StreamChunk::Done => {
                self.flush_text_to_blocks();
                self.flush_thinking_to_blocks();
                self.streaming = false;
                true
            }
            StreamChunk::Error { .. } => {
                self.flush_text_to_blocks();
                self.flush_thinking_to_blocks();
                self.streaming = false;
                true
            }
        }
    }

    fn flush_text_to_blocks(&mut self) {
        if !self.text_buf.is_empty() {
            let text = std::mem::take(&mut self.text_buf);
            self.blocks_so_far.push(MessageBlock::Text { text });
        }
    }

    fn flush_thinking_to_blocks(&mut self) {
        if !self.thinking_buf.is_empty() {
            let text = std::mem::take(&mut self.thinking_buf);
            self.blocks_so_far.push(MessageBlock::Thinking { text });
        }
    }

    /// Take ownership of the accumulated blocks, clearing state.
    /// Called when committing the in-flight stream as an
    /// `Assistant` turn.
    pub fn take_blocks(&mut self) -> Vec<MessageBlock> {
        // Belt-and-suspenders: flush any pending buffers, even if
        // the caller forgot to send `Done`.
        self.flush_text_to_blocks();
        self.flush_thinking_to_blocks();
        self.streaming = false;
        std::mem::take(&mut self.blocks_so_far)
    }
}

// ── Manager singleton ────────────────────────────────────────────────

/// All Wish conversations + per-conversation in-flight state.
pub struct ConversationManagerModel {
    conversations: HashMap<ConversationId, Conversation>,
    in_flight: HashMap<ConversationId, InFlight>,
    /// Active conversation displayed in the chat view.
    active: Option<ConversationId>,
    /// Insertion order for the conversation list — newest first.
    /// Maintained alongside the HashMap for stable rendering.
    order: Vec<ConversationId>,
    /// The active conversation adapter. Set once during init or when
    /// the user switches between guest / logged-in mode. Wrapped in
    /// `Arc` so it can be cheaply cloned to background threads.
    adapter: Option<Arc<dyn ConversationAdapter>>,
}

impl Entity for ConversationManagerModel {
    type Event = ConversationManagerEvent;
}

impl SingletonEntity for ConversationManagerModel {}

impl ConversationManagerModel {
    /// Construct an empty manager.
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            conversations: HashMap::new(),
            in_flight: HashMap::new(),
            active: None,
            order: Vec::new(),
            adapter: None,
        }
    }

    /// Set the active conversation adapter. Call this during app
    /// initialization or when the user switches auth modes.
    pub fn set_adapter(&mut self, adapter: Arc<dyn ConversationAdapter>) {
        self.adapter = Some(adapter);
    }

    /// The human-readable label of the current adapter (e.g.,
    /// "OpenAI-compatible / llama3.2:3b"). `None` when no adapter
    /// is configured.
    pub fn adapter_label(&self) -> Option<String> {
        self.adapter.as_ref().map(|a| a.label())
    }

    /// Send a user message in the given conversation, routing
    /// through the currently configured adapter. Returns `false`
    /// if the conversation doesn't exist or no adapter is set.
    ///
    /// The adapter runs on its own background thread and emits
    /// `StreamChunk`s through the sink. Each chunk is dispatched
    /// back to this model via a `ModelSpawner`, which safely
    /// marshals from the adapter's thread to the main thread.
    pub fn send_user_message(
        &mut self,
        id: &ConversationId,
        message: String,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let Some(adapter) = self.adapter.clone() else {
            log::warn!("No conversation adapter configured — cannot send message");
            return false;
        };

        // Append the user turn first.
        if !self.append_user(id, message.clone(), ctx) {
            return false;
        }

        // Snapshot the conversation for the adapter.
        let Some(conversation) = self.conversations.get(id).cloned() else {
            return false;
        };

        // Pre-create in-flight state so the view can show a spinner.
        self.in_flight
            .entry(id.clone())
            .or_insert_with(InFlight::default);

        // Build a sink that dispatches chunks back to this model
        // through the spawner. The adapter calls the sink from its
        // own background thread; the spawner marshals each chunk to
        // the main UI thread via an async channel. We use
        // `block_on` because the sink callback is synchronous.
        let spawner = ctx.spawner();
        let convo_id = id.clone();
        let sink: super::adapter::ChunkSink = Box::new(move |chunk: StreamChunk| {
            let convo_id = convo_id.clone();
            let spawner_clone = spawner.clone();
            // The adapter already runs on a background thread, so
            // blocking here is fine — we just need to queue the
            // chunk onto the main thread.
            let _ = futures::executor::block_on(spawner_clone.spawn(move |mgr, ctx| {
                mgr.apply_chunk(&convo_id, chunk, ctx);
            }));
        });

        adapter.send(&conversation, &message, sink);
        true
    }

    // ── Read API ─────────────────────────────────────────────────

    pub fn get(&self, id: &ConversationId) -> Option<&Conversation> {
        self.conversations.get(id)
    }

    /// Conversations in newest-first order — what the conversation
    /// list panel renders.
    pub fn list(&self) -> Vec<&Conversation> {
        self.order
            .iter()
            .filter_map(|id| self.conversations.get(id))
            .collect()
    }

    /// Active conversation ID. `None` when no conversations exist.
    pub fn active_id(&self) -> Option<&ConversationId> {
        self.active.as_ref()
    }

    /// Active conversation's full state.
    pub fn active_conversation(&self) -> Option<&Conversation> {
        self.active
            .as_ref()
            .and_then(|id| self.conversations.get(id))
    }

    /// In-flight stream state for a conversation.
    pub fn in_flight(&self, id: &ConversationId) -> Option<&InFlight> {
        self.in_flight.get(id)
    }

    /// Total number of conversations.
    pub fn len(&self) -> usize {
        self.conversations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.conversations.is_empty()
    }

    // ── Write API ────────────────────────────────────────────────

    /// Create a new conversation with the given primary agent.
    /// Auto-becomes active. Returns the new ID.
    pub fn create(
        &mut self,
        primary_agent_slug: String,
        ctx: &mut ModelContext<Self>,
    ) -> ConversationId {
        let id = ConversationId::new(generate_conversation_id());
        let conversation = Conversation::new(id.clone(), primary_agent_slug);
        self.conversations.insert(id.clone(), conversation);
        self.order.insert(0, id.clone());
        self.active = Some(id.clone());
        ctx.emit(ConversationManagerEvent::Created { id: id.clone() });
        ctx.emit(ConversationManagerEvent::ActiveChanged {
            id: Some(id.clone()),
        });
        id
    }

    /// Switch which conversation is the active one. No-op for an
    /// unknown ID.
    pub fn set_active(&mut self, id: Option<ConversationId>, ctx: &mut ModelContext<Self>) {
        let valid = id
            .as_ref()
            .map(|id| self.conversations.contains_key(id))
            .unwrap_or(true);
        if !valid {
            return;
        }
        if self.active == id {
            return;
        }
        self.active = id.clone();
        ctx.emit(ConversationManagerEvent::ActiveChanged { id });
    }

    /// Append a user turn to the given conversation.
    pub fn append_user(
        &mut self,
        id: &ConversationId,
        content: String,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let Some(conv) = self.conversations.get_mut(id) else {
            return false;
        };
        conv.append_user(content);
        // Promote to top of the recents list.
        self.order.retain(|x| x != id);
        self.order.insert(0, id.clone());
        ctx.emit(ConversationManagerEvent::TurnAppended { id: id.clone() });
        true
    }

    /// Apply a streaming chunk to the in-flight buffer for a
    /// conversation. If the chunk is `Done` or `Error`, flushes the
    /// buffer to a final `Turn::Assistant`.
    pub fn apply_chunk(
        &mut self,
        id: &ConversationId,
        chunk: StreamChunk,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        if !self.conversations.contains_key(id) {
            return false;
        }
        let in_flight = self.in_flight.entry(id.clone()).or_default();
        let should_commit = in_flight.apply(&chunk);
        ctx.emit(ConversationManagerEvent::Chunk {
            id: id.clone(),
            chunk: chunk.clone(),
        });

        if should_commit {
            let blocks = in_flight.take_blocks();
            // Don't append empty turns (e.g., a Done with no
            // content because the assistant only ran tools).
            if !blocks.is_empty() {
                let agent_slug = self
                    .conversations
                    .get(id)
                    .map(|c| c.primary_agent_slug.clone());
                if let Some(conv) = self.conversations.get_mut(id) {
                    conv.append_assistant(agent_slug, blocks);
                    ctx.emit(ConversationManagerEvent::TurnAppended { id: id.clone() });
                }
            }
            // Drop the in-flight state once committed.
            self.in_flight.remove(id);
        }
        true
    }

    /// Append a system note (e.g., "agent switched", "rate limit").
    pub fn append_system(
        &mut self,
        id: &ConversationId,
        note: String,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let Some(conv) = self.conversations.get_mut(id) else {
            return false;
        };
        conv.append_system(note);
        ctx.emit(ConversationManagerEvent::TurnAppended { id: id.clone() });
        true
    }

    /// Remove a conversation. Clears active if it was active.
    pub fn remove(&mut self, id: &ConversationId, ctx: &mut ModelContext<Self>) -> bool {
        if self.conversations.remove(id).is_none() {
            return false;
        }
        self.in_flight.remove(id);
        self.order.retain(|x| x != id);
        if self.active.as_ref() == Some(id) {
            self.active = self.order.first().cloned();
            let active = self.active.clone();
            ctx.emit(ConversationManagerEvent::ActiveChanged { id: active });
        }
        ctx.emit(ConversationManagerEvent::Removed { id: id.clone() });
        true
    }

    /// Test-only constructor: creates a manager with the given
    /// conversations pre-loaded. Used by integration tests that
    /// don't have a real `ModelContext`.
    #[cfg(test)]
    pub(crate) fn new_for_testing(conversations: Vec<Conversation>) -> Self {
        let mut order = Vec::new();
        let mut map = HashMap::new();
        for c in conversations {
            order.push(c.id.clone());
            map.insert(c.id.clone(), c);
        }
        let active = order.first().cloned();
        Self {
            conversations: map,
            in_flight: HashMap::new(),
            active,
            order,
            adapter: None,
        }
    }
}

/// Used by [`ConversationManagerModel::create`]. Same scheme as
/// [`crate::ai::agent_tasks::model`]'s task IDs.
fn generate_conversation_id() -> String {
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};
    thread_local! {
        static COUNTER: Cell<u64> = const { Cell::new(0) };
    }
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let n = COUNTER.with(|c| {
        let next = c.get().wrapping_add(1);
        c.set(next);
        next
    });
    format!("conv-{millis:013}-{n:08}")
}

// ── Tests for InFlight (pure logic, no ctx) ──────────────────────────

#[cfg(test)]
mod inflight_tests {
    use super::*;
    use crate::ai::agent_tasks::TaskId;

    #[test]
    fn apply_text_delta_accumulates() {
        let mut s = InFlight::default();
        assert!(!s.apply(&StreamChunk::TextDelta {
            delta: "Hel".into()
        }));
        assert!(!s.apply(&StreamChunk::TextDelta { delta: "lo".into() }));
        assert_eq!(s.text_buf, "Hello");
        assert!(s.streaming);
    }

    #[test]
    fn apply_done_flushes_text_and_returns_commit() {
        let mut s = InFlight::default();
        s.apply(&StreamChunk::TextDelta { delta: "Hi".into() });
        let commit = s.apply(&StreamChunk::Done);
        assert!(commit);
        assert!(!s.streaming);
        let blocks = s.take_blocks();
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], MessageBlock::Text { text } if text == "Hi"));
    }

    #[test]
    fn tool_use_flushes_text_first_for_correct_ordering() {
        let mut s = InFlight::default();
        s.apply(&StreamChunk::TextDelta {
            delta: "Let me check ".into(),
        });
        s.apply(&StreamChunk::ToolUse {
            task_id: TaskId::new("t1"),
            tool_name: "bash".into(),
            input_json: "{}".into(),
        });
        s.apply(&StreamChunk::TextDelta {
            delta: "Done!".into(),
        });
        s.apply(&StreamChunk::Done);
        let blocks = s.take_blocks();
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], MessageBlock::Text { text } if text == "Let me check "));
        assert!(matches!(&blocks[1], MessageBlock::ToolUse { .. }));
        assert!(matches!(&blocks[2], MessageBlock::Text { text } if text == "Done!"));
    }

    #[test]
    fn error_chunk_commits_just_like_done() {
        let mut s = InFlight::default();
        s.apply(&StreamChunk::TextDelta {
            delta: "Partial".into(),
        });
        let commit = s.apply(&StreamChunk::Error {
            message: "rate limit".into(),
        });
        assert!(commit);
        let blocks = s.take_blocks();
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn take_blocks_flushes_unflushed_buffers() {
        // Belt-and-suspenders: even without a Done chunk, taking
        // blocks should flush any pending text/thinking.
        let mut s = InFlight::default();
        s.text_buf = "stuck".into();
        let blocks = s.take_blocks();
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], MessageBlock::Text { text } if text == "stuck"));
        // After taking, state is empty.
        assert_eq!(s.text_buf, "");
        assert_eq!(s.blocks_so_far.len(), 0);
    }

    #[test]
    fn thinking_delta_separate_from_text() {
        let mut s = InFlight::default();
        s.apply(&StreamChunk::ThinkingDelta {
            delta: "considering...".into(),
        });
        s.apply(&StreamChunk::TextDelta {
            delta: "answer".into(),
        });
        s.apply(&StreamChunk::Done);
        let blocks = s.take_blocks();
        assert_eq!(blocks.len(), 2);
        assert!(matches!(&blocks[0], MessageBlock::Thinking { text } if text == "considering..."));
        assert!(matches!(&blocks[1], MessageBlock::Text { text } if text == "answer"));
    }

    #[test]
    fn streaming_flag_clears_on_done() {
        let mut s = InFlight::default();
        s.apply(&StreamChunk::TextDelta { delta: "x".into() });
        assert!(s.streaming);
        s.apply(&StreamChunk::Done);
        assert!(!s.streaming);
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;

    #[test]
    fn list_returns_newest_first() {
        let mut a = Conversation::new(ConversationId::new("a"), "x".into());
        let mut b = Conversation::new(ConversationId::new("b"), "x".into());
        let c = Conversation::new(ConversationId::new("c"), "x".into());
        a.title = "alpha".into();
        b.title = "beta".into();
        // Manually construct in the order they'd be created
        // (newest first: c, b, a).
        let mgr = ConversationManagerModel::new_for_testing(vec![c, b, a]);
        let list = mgr.list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].id.as_str(), "c");
        assert_eq!(list[1].id.as_str(), "b");
        assert_eq!(list[2].id.as_str(), "a");
    }

    #[test]
    fn empty_manager_is_empty() {
        let mgr = ConversationManagerModel::new_for_testing(vec![]);
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
        assert!(mgr.active_id().is_none());
        assert!(mgr.active_conversation().is_none());
    }

    #[test]
    fn first_conversation_becomes_active() {
        let mgr = ConversationManagerModel::new_for_testing(vec![Conversation::new(
            ConversationId::new("a"),
            "x".into(),
        )]);
        assert_eq!(mgr.active_id().map(|i| i.as_str()), Some("a"));
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let mgr = ConversationManagerModel::new_for_testing(vec![Conversation::new(
            ConversationId::new("a"),
            "x".into(),
        )]);
        assert!(mgr.get(&ConversationId::new("a")).is_some());
        assert!(mgr.get(&ConversationId::new("nope")).is_none());
    }

    #[test]
    fn in_flight_starts_empty() {
        let mgr = ConversationManagerModel::new_for_testing(vec![Conversation::new(
            ConversationId::new("a"),
            "x".into(),
        )]);
        assert!(mgr.in_flight(&ConversationId::new("a")).is_none());
    }
}
