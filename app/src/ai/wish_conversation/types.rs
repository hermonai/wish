//! Pure data types for Wish conversations.
//!
//! Wire-compatible (one-to-one mapping) with
//! [`hermon_client::types::conversation`] so a future Hermon adapter
//! can construct these directly from server responses without a
//! translation layer. Pure-data, serde-friendly, no UI deps —
//! testable without an `AppContext`.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use super::super::agent_tasks::TaskId;

// ── Identifiers ──────────────────────────────────────────────────────

/// Stable identifier for a conversation.
///
/// Locally-generated (millis + counter) for offline-only mode;
/// when Hermon is the backend, we use the server-issued ID
/// verbatim. Both sides of the union just see a `String`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(pub String);

impl ConversationId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Message blocks ───────────────────────────────────────────────────

/// A single block in an assistant message.
///
/// Mirrors Anthropic's content-block model (used by Claude API and
/// ports cleanly to OpenAI's tool-call schema). Each variant maps
/// to a distinct rendered surface in the chat view: `Text` is
/// markdown, `ToolUse` becomes a chip linking into the
/// [`AgentTaskRegistry`], `ToolResult` is an inline expandable
/// block, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageBlock {
    /// Plain text. Markdown-rendered by the view.
    Text { text: String },
    /// Internal reasoning ("thinking") shown collapsed by default.
    /// Hermon strips this from cloud responses; local providers
    /// (Ollama) may emit it directly.
    Thinking { text: String },
    /// Tool invocation. The `task_id` cross-references the
    /// [`super::super::agent_tasks::AgentTaskRegistryModel`] entry,
    /// so the chat view can render the live status of the tool
    /// call by looking up the task.
    ToolUse {
        task_id: TaskId,
        tool_name: String,
        /// JSON-encoded input. Kept as a string (rather than parsed
        /// `serde_json::Value`) so this type stays trivially
        /// serializable without an extra dep on serde_json's
        /// re-export surface.
        input_json: String,
    },
    /// Tool result. References the same `task_id` as the
    /// originating `ToolUse` block. The view renders this collapsed
    /// by default with an expand affordance.
    ToolResult {
        task_id: TaskId,
        output: String,
        is_error: bool,
    },
}

impl MessageBlock {
    /// Whether this block contributes to the visible "assistant
    /// said" text. Used by the conversation summarizer (future)
    /// and by the empty-message check.
    pub fn is_visible_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }
}

// ── Turns ────────────────────────────────────────────────────────────

/// One turn in a conversation. The conversation is an ordered
/// sequence of these.
///
/// Distinguishing user/assistant turns explicitly (rather than
/// using a single `Message` with a `role` field) lets the
/// view's pattern-match be exhaustive — the compiler tells you
/// when a new turn shape needs UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Turn {
    /// A user-supplied message. Always plain text in v1; future
    /// versions may add attachments, code blocks, etc.
    User {
        content: String,
        sent_at: SystemTime,
    },
    /// An assistant response. Composed of one or more
    /// [`MessageBlock`]s. The optional `agent_slug` tracks which
    /// agent answered (e.g. `"wish-coder"`).
    Assistant {
        agent_slug: Option<String>,
        blocks: Vec<MessageBlock>,
        completed_at: Option<SystemTime>,
    },
    /// A system-injected note ("agent switched to wish-reviewer",
    /// "rate limit hit, retrying", etc.). Rendered in muted style.
    System { note: String, at: SystemTime },
}

impl Turn {
    /// Convenience: text view of the turn for summarization.
    /// `Assistant` turns return the concatenation of their
    /// `Text`-block contents.
    pub fn plain_text(&self) -> String {
        match self {
            Self::User { content, .. } => content.clone(),
            Self::Assistant { blocks, .. } => blocks
                .iter()
                .filter_map(|b| match b {
                    MessageBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<&str>>()
                .join("\n\n"),
            Self::System { note, .. } => note.clone(),
        }
    }

    /// Whether this turn is from the user.
    pub fn is_user(&self) -> bool {
        matches!(self, Self::User { .. })
    }

    /// Whether this turn is from the assistant.
    pub fn is_assistant(&self) -> bool {
        matches!(self, Self::Assistant { .. })
    }
}

// ── Conversation ─────────────────────────────────────────────────────

/// A complete conversation.
///
/// The `agent_slug` is the *primary* agent driving the conversation;
/// individual assistant turns may be from a different agent (for
/// orchestrator → sub-agent dispatch), tracked per-turn in
/// [`Turn::Assistant::agent_slug`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    /// User-visible title. Empty until the first response, then
    /// auto-generated from the first user turn (truncated).
    pub title: String,
    /// Slug of the primary agent (e.g. `"wish-coder"`).
    pub primary_agent_slug: String,
    pub turns: Vec<Turn>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl Conversation {
    /// Construct a fresh conversation seeded with no turns.
    pub fn new(id: ConversationId, primary_agent_slug: String) -> Self {
        let now = SystemTime::now();
        Self {
            id,
            title: String::new(),
            primary_agent_slug,
            turns: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Append a user turn. Updates `updated_at`. If `title` is
    /// empty, auto-derives it from the first user message
    /// (truncated to ~60 chars).
    pub fn append_user(&mut self, content: String) {
        if self.title.is_empty() && !content.is_empty() {
            self.title = derive_title_from(&content);
        }
        self.turns.push(Turn::User {
            content,
            sent_at: SystemTime::now(),
        });
        self.updated_at = SystemTime::now();
    }

    /// Append an assistant turn (typically when streaming finishes).
    pub fn append_assistant(&mut self, agent_slug: Option<String>, blocks: Vec<MessageBlock>) {
        self.turns.push(Turn::Assistant {
            agent_slug,
            blocks,
            completed_at: Some(SystemTime::now()),
        });
        self.updated_at = SystemTime::now();
    }

    /// Append a system note.
    pub fn append_system(&mut self, note: String) {
        self.turns.push(Turn::System {
            note,
            at: SystemTime::now(),
        });
        self.updated_at = SystemTime::now();
    }

    /// Number of user turns so far. Used for cost-per-conversation
    /// telemetry and for the "first message?" check.
    pub fn user_turn_count(&self) -> usize {
        self.turns.iter().filter(|t| t.is_user()).count()
    }

    /// True if no user has spoken yet.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }
}

/// Auto-derive a conversation title from the first user message.
///
/// Strips newlines, collapses whitespace, truncates at the nearest
/// word boundary <= 60 chars. Pure function, fully tested.
pub fn derive_title_from(first_message: &str) -> String {
    let normalized: String = first_message
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");
    if normalized.len() <= 60 {
        return normalized;
    }
    // Truncate at the last space within the 60-char window.
    let mut truncated = &normalized[..60];
    if let Some(last_space) = truncated.rfind(' ') {
        truncated = &truncated[..last_space];
    }
    format!("{truncated}…")
}

// ── Streaming chunks ─────────────────────────────────────────────────

/// One chunk of a streamed assistant response. Adapters emit these
/// as the response arrives; the conversation manager applies them
/// to the in-flight assistant turn.
///
/// Mirrors the SSE event shape from
/// [`hermon_client::types::ai::AgentStreamEvent`] and from
/// OpenAI's `chat.completion.chunk` schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamChunk {
    /// Text delta to append to the current `Text` block.
    TextDelta { delta: String },
    /// A `Thinking` block delta.
    ThinkingDelta { delta: String },
    /// The assistant decided to invoke a tool. The adapter has
    /// already created the corresponding `AgentTask` and supplies
    /// the resulting `task_id` here.
    ToolUse {
        task_id: TaskId,
        tool_name: String,
        input_json: String,
    },
    /// Result of a previously-emitted tool call.
    ToolResult {
        task_id: TaskId,
        output: String,
        is_error: bool,
    },
    /// Stream finished successfully.
    Done,
    /// Stream errored. The string is a short user-facing summary.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_id_round_trips() {
        let id = ConversationId::new("conv-abc");
        assert_eq!(id.as_str(), "conv-abc");
        assert_eq!(id.to_string(), "conv-abc");
    }

    #[test]
    fn new_conversation_is_empty() {
        let c = Conversation::new(ConversationId::new("c1"), "wish-coder".into());
        assert!(c.is_empty());
        assert_eq!(c.user_turn_count(), 0);
        assert_eq!(c.title, "");
    }

    #[test]
    fn append_user_sets_title_on_first_message() {
        let mut c = Conversation::new(ConversationId::new("c1"), "wish-coder".into());
        c.append_user("Help me refactor this file".into());
        assert_eq!(c.title, "Help me refactor this file");
        assert_eq!(c.user_turn_count(), 1);
    }

    #[test]
    fn append_user_does_not_overwrite_existing_title() {
        let mut c = Conversation::new(ConversationId::new("c1"), "wish-coder".into());
        c.append_user("First".into());
        c.append_user("Second".into());
        // Title stays as the first message — second turn doesn't
        // touch it.
        assert_eq!(c.title, "First");
        assert_eq!(c.user_turn_count(), 2);
    }

    #[test]
    fn derive_title_passes_short_messages_through() {
        assert_eq!(derive_title_from("Hi there"), "Hi there");
    }

    #[test]
    fn derive_title_collapses_whitespace() {
        assert_eq!(
            derive_title_from("Hello\n  there\tworld"),
            "Hello there world"
        );
    }

    #[test]
    fn derive_title_truncates_long_messages_at_word_boundary() {
        let long =
            "This is a really really really really really really really really really long message";
        let title = derive_title_from(long);
        assert!(title.ends_with("…"));
        assert!(title.len() <= 65); // 60 chars + ellipsis
                                    // Should not split mid-word.
        assert!(!title[..title.len() - "…".len()].ends_with("rea"));
    }

    #[test]
    fn assistant_turn_plain_text_concatenates_text_blocks() {
        let turn = Turn::Assistant {
            agent_slug: Some("wish-coder".into()),
            blocks: vec![
                MessageBlock::Text {
                    text: "Hello".into(),
                },
                MessageBlock::ToolUse {
                    task_id: TaskId::new("t1"),
                    tool_name: "bash".into(),
                    input_json: "{}".into(),
                },
                MessageBlock::Text {
                    text: "World".into(),
                },
            ],
            completed_at: None,
        };
        assert_eq!(turn.plain_text(), "Hello\n\nWorld");
    }

    #[test]
    fn user_turn_plain_text_returns_content() {
        let turn = Turn::User {
            content: "Hello".into(),
            sent_at: SystemTime::now(),
        };
        assert_eq!(turn.plain_text(), "Hello");
    }

    #[test]
    fn turn_role_predicates() {
        let user = Turn::User {
            content: "x".into(),
            sent_at: SystemTime::now(),
        };
        let asst = Turn::Assistant {
            agent_slug: None,
            blocks: vec![],
            completed_at: None,
        };
        let sys = Turn::System {
            note: "hi".into(),
            at: SystemTime::now(),
        };
        assert!(user.is_user() && !user.is_assistant());
        assert!(asst.is_assistant() && !asst.is_user());
        assert!(!sys.is_user() && !sys.is_assistant());
    }

    #[test]
    fn message_block_is_visible_text() {
        assert!(MessageBlock::Text { text: "x".into() }.is_visible_text());
        assert!(!MessageBlock::Thinking { text: "x".into() }.is_visible_text());
        assert!(!MessageBlock::ToolUse {
            task_id: TaskId::new("t"),
            tool_name: "bash".into(),
            input_json: "{}".into(),
        }
        .is_visible_text());
    }

    #[test]
    fn append_assistant_records_completion_time() {
        let mut c = Conversation::new(ConversationId::new("c"), "x".into());
        c.append_assistant(
            Some("wish-coder".into()),
            vec![MessageBlock::Text { text: "ok".into() }],
        );
        assert_eq!(c.turns.len(), 1);
        if let Turn::Assistant { completed_at, .. } = &c.turns[0] {
            assert!(completed_at.is_some());
        } else {
            panic!("expected assistant turn");
        }
    }

    #[test]
    fn append_user_does_not_set_title_for_empty_message() {
        let mut c = Conversation::new(ConversationId::new("c"), "x".into());
        c.append_user("".into());
        assert_eq!(c.title, "");
    }

    #[test]
    fn message_block_serializes_with_type_tag() {
        let b = MessageBlock::Text { text: "hi".into() };
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"text\":\"hi\""));
    }
}
