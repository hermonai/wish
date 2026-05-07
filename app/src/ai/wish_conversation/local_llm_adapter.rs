//! [`LocalLlmAdapter`] — drives a Wish conversation against an
//! OpenAI-compatible local endpoint (Ollama, llama.cpp's
//! `llama-server`, LM Studio, vllm, etc.).
//!
//! # Why this matters
//!
//! With this adapter shipped, **Wish has a working chat surface
//! without needing Hermon login**. Users who have Ollama running
//! locally can talk to a built-in SDLC agent purely on their
//! machine — the killer "no account required" path.
//!
//! # Implementation strategy
//!
//! - **Pure functions for request building and response parsing**
//!   so the wire-format logic is fully unit-tested without any
//!   HTTP roundtrip.
//! - **`reqwest::blocking::Client` on a `std::thread::spawn`** for
//!   the actual network call — the adapter trait is sync, this
//!   keeps the implementation straightforward and the test surface
//!   small.
//! - **Non-streaming first**. Ollama supports SSE streaming via
//!   `"stream": true`, but the simpler non-stream path is enough
//!   to validate the architecture; streaming is a follow-up that
//!   only changes the response-parser, not the request-builder.
//!
//! # Cross-references
//!
//! - [`crate::ai::local_llm`] — the `LocalLlmProviderConfig` we
//!   consume here. That module owns the wire types
//!   (`ChatCompletionRequest`, `ChatCompletionResponse`); we use
//!   them directly so request shape stays in sync with the rest
//!   of Wish's local-LLM plumbing.
//! - [`super::adapter::ConversationAdapter`] — the trait we
//!   implement. See the trait docs for the contract (must emit
//!   `Done` or `Error` exactly once).

use std::time::Duration;

use crate::ai::local_llm::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, LocalLlmProviderConfig,
};

use super::adapter::{ChunkSink, ConversationAdapter};
use super::types::{Conversation, MessageBlock, StreamChunk, Turn};

/// Configurable adapter for an OpenAI-compatible local endpoint.
///
/// Holds a snapshot of the provider config — if the user changes
/// settings, a new adapter must be constructed. The adapter is
/// `Clone` so it's cheap to hand out from the conversation manager.
#[derive(Debug, Clone)]
pub struct LocalLlmAdapter {
    config: LocalLlmProviderConfig,
}

impl LocalLlmAdapter {
    /// Construct an adapter from a fully-populated provider config.
    pub fn new(config: LocalLlmProviderConfig) -> Self {
        Self { config }
    }

    /// Reference to the underlying config — useful for the chat
    /// header to display "Running on llama3.2:3b" etc.
    pub fn config(&self) -> &LocalLlmProviderConfig {
        &self.config
    }
}

impl ConversationAdapter for LocalLlmAdapter {
    fn send(&self, conversation: &Conversation, new_user_message: &str, sink: ChunkSink) {
        let request = build_chat_request(
            self.config.default_model().to_string(),
            conversation,
            new_user_message,
        );
        let url = chat_completions_url(self.config.base_url());
        let timeout = self.config.http_timeout();
        let api_key = match &self.config {
            LocalLlmProviderConfig::OpenAiCompatible { api_key, .. } => api_key.clone(),
        };

        // Run the HTTP call on a background thread so the calling
        // view's event loop isn't blocked. Adapters own their own
        // threading per the trait contract.
        std::thread::spawn(move || {
            let result = perform_request(&url, &api_key, timeout, &request);
            match result {
                Ok(text) => {
                    for chunk in parse_response_to_chunks(&text) {
                        sink(chunk);
                    }
                }
                Err(message) => {
                    sink(StreamChunk::Error { message });
                    // Per the adapter contract, Error is itself a
                    // terminal chunk — no Done required after.
                }
            }
        });
    }

    fn label(&self) -> String {
        let model = self.config.default_model();
        format!("{} / {model}", self.config.type_label())
    }
}

// ── Pure functions (fully unit-tested) ────────────────────────────────

/// Construct the `chat/completions` URL from the provider's base URL.
///
/// The OpenAI spec dictates `{base_url}/chat/completions`. Many local
/// servers expose `{base_url}/v1`; users typically include the `/v1`
/// in their config. This helper joins the suffix without
/// double-slashes.
pub(crate) fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    format!("{trimmed}/chat/completions")
}

/// Convert a Wish [`Conversation`] + new user message into the
/// OpenAI-compatible `ChatCompletionRequest`.
///
/// History conversion rules:
/// - `Turn::User { content, .. }` → role `"user"`
/// - `Turn::Assistant` → role `"assistant"`. Tool blocks are
///   inlined as text (`"[tool: bash] cargo test"`) for now —
///   proper tool-use round-tripping requires an OpenAI-tools
///   request shape, follow-up work.
/// - `Turn::System` → role `"system"`. The Wish "system note"
///   semantics map onto OpenAI's system role naturally.
/// - The new user message is appended as a final `"user"` turn.
pub(crate) fn build_chat_request(
    model: String,
    conversation: &Conversation,
    new_user_message: &str,
) -> ChatCompletionRequest {
    let mut messages: Vec<ChatMessage> = conversation.turns.iter().map(turn_to_message).collect();
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: new_user_message.to_string(),
    });
    ChatCompletionRequest {
        model,
        messages,
        temperature: None,
        max_tokens: None,
        stream: false,
    }
}

fn turn_to_message(turn: &Turn) -> ChatMessage {
    match turn {
        Turn::User { content, .. } => ChatMessage {
            role: "user".to_string(),
            content: content.clone(),
        },
        Turn::Assistant { blocks, .. } => ChatMessage {
            role: "assistant".to_string(),
            content: assistant_blocks_to_text(blocks),
        },
        Turn::System { note, .. } => ChatMessage {
            role: "system".to_string(),
            content: note.clone(),
        },
    }
}

/// Render an assistant turn's blocks back to a single text string
/// suitable for inclusion in chat history.
///
/// Tool blocks are surfaced as bracketed annotations so the local
/// LLM has *some* signal that a tool was invoked; the final
/// implementation will use OpenAI's `tool_calls` / `tool` role
/// instead, but that's a wire-format upgrade rather than a
/// semantic change.
fn assistant_blocks_to_text(blocks: &[MessageBlock]) -> String {
    blocks
        .iter()
        .map(|b| match b {
            MessageBlock::Text { text } => text.clone(),
            MessageBlock::Thinking { .. } => String::new(), // hide from history
            MessageBlock::ToolUse {
                tool_name,
                input_json,
                ..
            } => format!("[tool:{tool_name}] {input_json}"),
            MessageBlock::ToolResult {
                output, is_error, ..
            } => {
                let prefix = if *is_error {
                    "[tool-error]"
                } else {
                    "[tool-result]"
                };
                format!("{prefix} {output}")
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<String>>()
        .join("\n\n")
}

/// Parse a non-streaming `ChatCompletionResponse` JSON body into
/// a stream of [`StreamChunk`]s.
///
/// Always emits exactly one of `{TextDelta..., Done}` or
/// `{Error}` — never both, never zero. This keeps the in-flight
/// state machine in [`super::model::InFlight`] simple.
pub(crate) fn parse_response_to_chunks(body: &str) -> Vec<StreamChunk> {
    match serde_json::from_str::<ChatCompletionResponse>(body) {
        Ok(resp) => match resp.choices.into_iter().next() {
            Some(choice) => {
                let mut out = Vec::with_capacity(2);
                if !choice.message.content.is_empty() {
                    out.push(StreamChunk::TextDelta {
                        delta: choice.message.content,
                    });
                }
                out.push(StreamChunk::Done);
                out
            }
            None => vec![StreamChunk::Error {
                message: "local LLM returned no choices".to_string(),
            }],
        },
        Err(e) => vec![StreamChunk::Error {
            message: format!("local LLM response parse error: {e}"),
        }],
    }
}

// ── HTTP shell (not exercised by unit tests) ─────────────────────────

/// Issue the actual HTTP request. Returns the response body on
/// success, or a human-readable error on failure.
///
/// Lives behind a function boundary so the unit tests don't have
/// to mock HTTP — they exercise `build_chat_request` and
/// `parse_response_to_chunks` directly. Integration tests against
/// a live Ollama instance go in `tests/` (out of scope for this
/// turn).
fn perform_request(
    url: &str,
    api_key: &str,
    timeout: Duration,
    request: &ChatCompletionRequest,
) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("local LLM HTTP client init failed: {e}"))?;

    let mut req = client.post(url).json(request);
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req
        .send()
        .map_err(|e| format!("local LLM request failed (is the server running at {url}?): {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| format!("local LLM response body read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "local LLM returned HTTP {} — body: {}",
            status.as_u16(),
            truncate_for_error(&text, 200)
        ));
    }
    Ok(text)
}

/// Cap an error string for log/UI display. Pure helper.
fn truncate_for_error(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

// ── Tests (pure-function only — no live HTTP) ────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::wish_conversation::types::{Conversation, ConversationId};

    fn ollama_default_config() -> LocalLlmProviderConfig {
        LocalLlmProviderConfig::OpenAiCompatible {
            base_url: "http://localhost:11434/v1".into(),
            api_key: "ollama".into(),
            default_model: "llama3.2:3b".into(),
            timeout_secs: None,
        }
    }

    // ── chat_completions_url ─────────────────────────────────────

    #[test]
    fn url_appends_path_to_clean_base() {
        assert_eq!(
            chat_completions_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn url_strips_trailing_slash() {
        assert_eq!(
            chat_completions_url("http://localhost:11434/v1/"),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn url_handles_root_base() {
        assert_eq!(
            chat_completions_url("http://localhost:11434"),
            "http://localhost:11434/chat/completions"
        );
    }

    // ── build_chat_request ──────────────────────────────────────

    #[test]
    fn empty_conversation_yields_only_new_message() {
        let conv = Conversation::new(ConversationId::new("c"), "wish-coder".into());
        let req = build_chat_request("llama3.2:3b".into(), &conv, "hello");
        assert_eq!(req.model, "llama3.2:3b");
        assert!(!req.stream);
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
        assert_eq!(req.messages[0].content, "hello");
    }

    #[test]
    fn user_turn_in_history_round_trips() {
        let mut conv = Conversation::new(ConversationId::new("c"), "wish-coder".into());
        conv.append_user("first message".into());
        let req = build_chat_request("m".into(), &conv, "second");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "user");
        assert_eq!(req.messages[0].content, "first message");
        assert_eq!(req.messages[1].role, "user");
        assert_eq!(req.messages[1].content, "second");
    }

    #[test]
    fn assistant_text_blocks_concat_in_history() {
        let mut conv = Conversation::new(ConversationId::new("c"), "x".into());
        conv.append_assistant(
            None,
            vec![
                MessageBlock::Text {
                    text: "Hello".into(),
                },
                MessageBlock::Text {
                    text: "World".into(),
                },
            ],
        );
        let req = build_chat_request("m".into(), &conv, "next");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "assistant");
        assert_eq!(req.messages[0].content, "Hello\n\nWorld");
    }

    #[test]
    fn thinking_blocks_are_stripped_from_history() {
        let mut conv = Conversation::new(ConversationId::new("c"), "x".into());
        conv.append_assistant(
            None,
            vec![
                MessageBlock::Thinking {
                    text: "let me think".into(),
                },
                MessageBlock::Text {
                    text: "answer".into(),
                },
            ],
        );
        let req = build_chat_request("m".into(), &conv, "next");
        // Only the visible text reaches the wire.
        assert_eq!(req.messages[0].content, "answer");
    }

    #[test]
    fn tool_use_blocks_render_as_bracketed_annotation() {
        use crate::ai::agent_tasks::TaskId;
        let mut conv = Conversation::new(ConversationId::new("c"), "x".into());
        conv.append_assistant(
            None,
            vec![MessageBlock::ToolUse {
                task_id: TaskId::new("t1"),
                tool_name: "bash".into(),
                input_json: r#"{"cmd":"ls"}"#.into(),
            }],
        );
        let req = build_chat_request("m".into(), &conv, "next");
        assert!(req.messages[0].content.contains("[tool:bash]"));
        assert!(req.messages[0].content.contains("ls"));
    }

    #[test]
    fn system_turn_maps_to_system_role() {
        let mut conv = Conversation::new(ConversationId::new("c"), "x".into());
        conv.append_system("agent switched".into());
        let req = build_chat_request("m".into(), &conv, "next");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[0].content, "agent switched");
    }

    // ── parse_response_to_chunks ─────────────────────────────────

    #[test]
    fn parse_normal_response_yields_text_then_done() {
        let body = r#"{
            "id": "x",
            "object": "chat.completion",
            "model": "llama3.2:3b",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hello world" },
                "finish_reason": "stop"
            }]
        }"#;
        let chunks = parse_response_to_chunks(body);
        assert_eq!(chunks.len(), 2);
        assert!(matches!(
            &chunks[0],
            StreamChunk::TextDelta { delta } if delta == "Hello world"
        ));
        assert!(matches!(&chunks[1], StreamChunk::Done));
    }

    #[test]
    fn parse_empty_content_skips_text_delta_but_emits_done() {
        // Some providers respond with empty content (e.g., the
        // model produced only tool calls). We still need a `Done`
        // so the InFlight state finalizes.
        let body = r#"{
            "id": "x",
            "object": "chat.completion",
            "model": "x",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "" }
            }]
        }"#;
        let chunks = parse_response_to_chunks(body);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], StreamChunk::Done));
    }

    #[test]
    fn parse_no_choices_yields_error() {
        let body = r#"{
            "id": "x",
            "object": "chat.completion",
            "model": "x",
            "choices": []
        }"#;
        let chunks = parse_response_to_chunks(body);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], StreamChunk::Error { .. }));
    }

    #[test]
    fn parse_invalid_json_yields_error() {
        let chunks = parse_response_to_chunks("not json");
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], StreamChunk::Error { .. }));
    }

    #[test]
    fn parse_extra_fields_are_tolerated() {
        // Ollama includes a `usage` block + `created` timestamp +
        // `system_fingerprint` that vanilla OpenAI deserializers
        // would choke on. We use serde's default lenient mode.
        let body = r#"{
            "id": "x",
            "object": "chat.completion",
            "model": "x",
            "created": 1234567890,
            "system_fingerprint": "fp_abc",
            "usage": { "prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3 },
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "ok" },
                "finish_reason": "stop"
            }]
        }"#;
        let chunks = parse_response_to_chunks(body);
        assert_eq!(chunks.len(), 2);
        assert!(matches!(
            &chunks[0],
            StreamChunk::TextDelta { delta } if delta == "ok"
        ));
    }

    // ── Adapter label ────────────────────────────────────────────

    #[test]
    fn adapter_label_includes_type_and_model() {
        let adapter = LocalLlmAdapter::new(ollama_default_config());
        let label = adapter.label();
        assert!(label.contains("OpenAI-compatible"));
        assert!(label.contains("llama3.2:3b"));
    }

    #[test]
    fn adapter_exposes_underlying_config() {
        let adapter = LocalLlmAdapter::new(ollama_default_config());
        assert_eq!(adapter.config().default_model(), "llama3.2:3b");
    }

    // ── truncate_for_error ──────────────────────────────────────

    #[test]
    fn truncate_passes_short_strings_through() {
        assert_eq!(truncate_for_error("ok", 100), "ok");
    }

    #[test]
    fn truncate_appends_ellipsis_for_long_strings() {
        let long = "x".repeat(300);
        let truncated = truncate_for_error(&long, 50);
        assert_eq!(truncated.len(), 50 + "…".len());
        assert!(truncated.ends_with("…"));
    }
}
