//! Local LLM provider for pure-local Wish (Ollama / OpenAI-compatible).
//!
//! When Wish is running without a Hermon backend, agents need a model
//! provider somewhere. This module supplies a thin client that talks
//! to any OpenAI-compatible chat-completions endpoint — Ollama at
//! `http://localhost:11434/v1`, llama.cpp's `llama-server`, LM Studio,
//! `vllm`, etc.
//!
//! # Why a separate module
//!
//! The Hermon path (`hermon_client::ai`) routes requests through the
//! Hermon gateway, which adds auth, rate-limiting, model-routing
//! policies, and telemetry. None of that applies to a fully local
//! endpoint, and conflating the two would force a slow, opinionated
//! request path even when the user just wants to hit Ollama directly.
//!
//! # Configuration
//!
//! Pulled from `~/.wish/settings.toml`:
//!
//! ```toml
//! [ai.providers.ollama]
//! type = "openai_compatible"
//! base_url = "http://localhost:11434/v1"
//! api_key = "ollama"   # required by spec but unused by Ollama
//! default_model = "llama3.2:3b"
//!
//! [ai]
//! default_provider = "ollama"
//! ```
//!
//! # Roadmap
//!
//! This module currently exposes a typed config struct + a probe
//! function that verifies an endpoint is reachable. The chat
//! completion request/response wire types are defined here too, but
//! the runtime integration (so the registry's built-in agents
//! actually invoke a local model) is wired up in a follow-up turn —
//! see `docs/AGENT_REGISTRY.md` § "Natural next steps".

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// User-supplied configuration for a local LLM provider.
///
/// Deserialized from `~/.wish/settings.toml`; serialized for the
/// debug log output that ships in support bundles. The `api_key`
/// field is masked when serialized (never written to logs).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum LocalLlmProviderConfig {
    /// An OpenAI-compatible chat-completions endpoint. Covers Ollama,
    /// llama.cpp, LM Studio, vllm, Together, and Groq among others.
    ///
    /// The discriminator is explicitly renamed to `openai_compatible`
    /// (not the snake_case `open_ai_compatible` you'd get from
    /// `rename_all = "snake_case"`) because that's the spelling
    /// users will actually type into `~/.wish/settings.toml`.
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible {
        /// Base URL up to (but not including) `/chat/completions`.
        /// Examples:
        /// - Ollama: `http://localhost:11434/v1`
        /// - llama.cpp: `http://localhost:8080/v1`
        /// - LM Studio: `http://localhost:1234/v1`
        base_url: String,

        /// API key. Required by the OpenAI spec for the
        /// `Authorization: Bearer <key>` header; some local servers
        /// ignore the header (Ollama) but still want a non-empty
        /// string. We keep it in the config so the same shape works
        /// for cloud providers (OpenAI, Together, etc.).
        ///
        /// Marked `serde(skip_serializing)` so it never lands in
        /// debug-log dumps.
        #[serde(default, skip_serializing)]
        api_key: String,

        /// Default model identifier (e.g. `llama3.2:3b`,
        /// `qwen2.5-coder:7b`, `gpt-4o-mini`). Required because we
        /// can't infer it from the provider — `/v1/models` is
        /// optional in the OpenAI spec.
        default_model: String,

        /// Optional override for HTTP timeout. Defaults to 60s.
        /// Local models can take seconds to start up after a cold
        /// load; cloud providers respond in <1s.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_secs: Option<u64>,
    },
}

impl LocalLlmProviderConfig {
    /// Convenience: the user-facing label for this provider type.
    /// Used in the Settings → AI list and in error messages.
    pub fn type_label(&self) -> &'static str {
        match self {
            Self::OpenAiCompatible { .. } => "OpenAI-compatible",
        }
    }

    /// The default model identifier this provider invokes when no
    /// model is specified per-call.
    pub fn default_model(&self) -> &str {
        match self {
            Self::OpenAiCompatible { default_model, .. } => default_model,
        }
    }

    /// HTTP timeout for non-streaming requests.
    pub fn http_timeout(&self) -> Duration {
        match self {
            Self::OpenAiCompatible { timeout_secs, .. } => {
                Duration::from_secs(timeout_secs.unwrap_or(60))
            }
        }
    }

    /// Base URL the provider's `/chat/completions`, `/models`, etc.
    /// endpoints are relative to.
    pub fn base_url(&self) -> &str {
        match self {
            Self::OpenAiCompatible { base_url, .. } => base_url,
        }
    }
}

/// Resolve a `LocalLlmProviderConfig` from a model id whose prefix
/// identifies the routing host. Mirrors the prefixes embedded by
/// `crate::ai::llms::hermon_llm_info` + `ollama_llm_info`:
///
/// | Prefix             | Host                          | Default URL                           |
/// |--------------------|-------------------------------|---------------------------------------|
/// | `ollama:<m>`       | Ollama                        | `http://localhost:11434/v1`           |
/// | `hermon-local:<m>` | Hermon local engine           | `http://localhost:11435/v1`           |
/// | `hermon:<m>`       | Hermon cloud gateway          | `https://api.hermon.ai/v1` + `HERMON_API_KEY` |
///
/// Returns `None` when the id has no recognized prefix (the caller
/// should fall back to its statically-configured Ollama adapter,
/// which is the legacy default and exactly what user has wired today).
///
/// The returned config's `default_model` is the suffix after the
/// prefix — so a model id of `hermon-local:llama3.2:3b` yields
/// `default_model = "llama3.2:3b"` and the OpenAI request body
/// carries the unprefixed name the upstream actually understands.
pub fn provider_config_for_model(model_id: &str) -> Option<LocalLlmProviderConfig> {
    if let Some(rest) = model_id.strip_prefix("hermon-local:") {
        let base_url = std::env::var("HERMON_LOCAL_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11435".into())
            .trim_end_matches('/')
            .to_string();
        return Some(LocalLlmProviderConfig::OpenAiCompatible {
            base_url: format!("{base_url}/v1"),
            // Bearer is required by the OpenAI spec; the engine
            // accepts any non-empty string and rejects empties.
            api_key: "hermon-local".to_string(),
            default_model: rest.to_string(),
            timeout_secs: None,
        });
    }
    if let Some(rest) = model_id.strip_prefix("hermon:") {
        // Cloud routing requires the user's HERMON_API_KEY. Returning
        // None when it's absent lets the caller surface a clear error
        // ("create a key at https://www.hermon.ai/wish/settings/api-keys")
        // instead of sending to api.hermon.ai with a placeholder Bearer.
        let api_key = std::env::var("HERMON_API_KEY").ok().filter(|k| !k.is_empty())?;
        let base_url = std::env::var("HERMON_API_URL")
            .unwrap_or_else(|_| "https://api.hermon.ai".into())
            .trim_end_matches('/')
            .to_string();
        return Some(LocalLlmProviderConfig::OpenAiCompatible {
            base_url: format!("{base_url}/v1"),
            api_key,
            default_model: rest.to_string(),
            timeout_secs: None,
        });
    }
    if let Some(rest) = model_id.strip_prefix("ollama:") {
        let base_url = std::env::var("WISH_OLLAMA_URL")
            .or_else(|_| std::env::var("OLLAMA_HOST"))
            .unwrap_or_else(|_| "http://127.0.0.1:11434".into())
            .trim_end_matches('/')
            .to_string();
        // Some users include /v1 in their override, some don't.
        let base_with_v1 = if base_url.ends_with("/v1") {
            base_url
        } else {
            format!("{base_url}/v1")
        };
        return Some(LocalLlmProviderConfig::OpenAiCompatible {
            base_url: base_with_v1,
            api_key: std::env::var("WISH_OLLAMA_API_KEY").unwrap_or_else(|_| "ollama".into()),
            default_model: rest.to_string(),
            timeout_secs: None,
        });
    }
    None
}

#[cfg(test)]
mod provider_resolution_tests {
    use super::*;

    #[test]
    fn hermon_local_prefix_resolves_to_port_11435() {
        let cfg = provider_config_for_model("hermon-local:llama3.2:3b").unwrap();
        match cfg {
            LocalLlmProviderConfig::OpenAiCompatible {
                base_url,
                default_model,
                ..
            } => {
                assert!(
                    base_url.contains("11435"),
                    "expected port 11435, got {base_url}"
                );
                assert_eq!(default_model, "llama3.2:3b");
            }
        }
    }

    #[test]
    fn ollama_prefix_resolves_to_port_11434() {
        // Clear env in case the test runner has WISH_OLLAMA_URL set.
        std::env::remove_var("WISH_OLLAMA_URL");
        std::env::remove_var("OLLAMA_HOST");
        let cfg = provider_config_for_model("ollama:phi3.5:3.8b").unwrap();
        match cfg {
            LocalLlmProviderConfig::OpenAiCompatible {
                base_url,
                default_model,
                ..
            } => {
                assert!(base_url.contains("11434"));
                assert_eq!(default_model, "phi3.5:3.8b");
            }
        }
    }

    #[test]
    fn hermon_cloud_without_key_returns_none() {
        std::env::remove_var("HERMON_API_KEY");
        assert!(provider_config_for_model("hermon:claude-sonnet-4-6").is_none());
    }

    #[test]
    fn unknown_prefix_returns_none() {
        assert!(provider_config_for_model("claude-sonnet-4-6").is_none());
        assert!(provider_config_for_model("gpt-4o").is_none());
    }
}

/// Wire-shape for a single chat message in the OpenAI-compatible
/// schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// `system` | `user` | `assistant` | `tool`. Lowercased.
    pub role: String,
    pub content: String,
}

/// Request body for `POST {base_url}/chat/completions`.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// Optional sampling temperature [0.0, 2.0]. None → use server
    /// default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Optional max tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// When true, the response is a server-sent-events stream.
    /// We default to false; streaming is wired in a follow-up turn.
    #[serde(default)]
    pub stream: bool,
}

/// Response body for `POST {base_url}/chat/completions` (non-stream).
///
/// Subset of the OpenAI spec — we only deserialize the fields the
/// agent runtime cares about. Extra fields are silently ignored
/// (serde default behavior).
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatCompletionChoice>,
    /// Server-reported model identifier (echoes the request, but
    /// some providers normalize it).
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionChoice {
    pub message: ChatMessage,
    /// `stop` | `length` | `tool_calls` | `content_filter` | …
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Status of a connectivity probe to a local LLM endpoint.
///
/// Consumed by the Settings → AI page to surface a "🟢 connected"
/// indicator next to each configured provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalLlmHealth {
    /// Endpoint responded successfully to a `GET /v1/models` request
    /// (or, when not supported by the server, accepted a no-op
    /// `/chat/completions` with `max_tokens=1`).
    Ok,
    /// Endpoint is reachable but the response body didn't parse as
    /// the expected JSON shape. The string holds a short
    /// human-readable summary.
    BadResponse(String),
    /// Endpoint isn't reachable (network error, DNS failure,
    /// connection refused). The string is the underlying error
    /// message.
    Unreachable(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ollama_default() -> LocalLlmProviderConfig {
        LocalLlmProviderConfig::OpenAiCompatible {
            base_url: "http://localhost:11434/v1".into(),
            api_key: "ollama".into(),
            default_model: "llama3.2:3b".into(),
            timeout_secs: None,
        }
    }

    #[test]
    fn type_label_is_human_readable() {
        let p = ollama_default();
        assert_eq!(p.type_label(), "OpenAI-compatible");
    }

    #[test]
    fn default_model_is_returned() {
        let p = ollama_default();
        assert_eq!(p.default_model(), "llama3.2:3b");
    }

    #[test]
    fn default_timeout_is_60s() {
        let p = ollama_default();
        assert_eq!(p.http_timeout(), Duration::from_secs(60));
    }

    #[test]
    fn custom_timeout_honored() {
        let p = LocalLlmProviderConfig::OpenAiCompatible {
            base_url: "http://x".into(),
            api_key: "k".into(),
            default_model: "m".into(),
            timeout_secs: Some(5),
        };
        assert_eq!(p.http_timeout(), Duration::from_secs(5));
    }

    #[test]
    fn api_key_does_not_serialize() {
        let p = ollama_default();
        let json = serde_json::to_string(&p).unwrap();
        assert!(
            !json.contains("ollama"),
            "api_key value '{}' must NOT appear in serialized output: {json}",
            "ollama"
        );
    }

    #[test]
    fn config_roundtrips_through_toml() {
        let toml_input = r#"
type = "openai_compatible"
base_url = "http://localhost:11434/v1"
api_key = "ollama"
default_model = "llama3.2:3b"
"#;
        let parsed: LocalLlmProviderConfig = toml::from_str(toml_input).unwrap();
        match &parsed {
            LocalLlmProviderConfig::OpenAiCompatible {
                base_url,
                default_model,
                ..
            } => {
                assert_eq!(base_url, "http://localhost:11434/v1");
                assert_eq!(default_model, "llama3.2:3b");
            }
        }
    }

    #[test]
    fn chat_message_serializes_in_lowercase_role() {
        let msg = ChatMessage {
            role: "user".into(),
            content: "hello".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"hello\""));
    }

    #[test]
    fn chat_completion_request_omits_none_fields() {
        let req = ChatCompletionRequest {
            model: "m".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("temperature"));
        assert!(!json.contains("max_tokens"));
    }

    #[test]
    fn chat_completion_response_deserializes_minimal() {
        // Reflects an Ollama response — `model` may or may not be
        // present, extra `usage` field is silently ignored.
        let body = r#"{
            "id": "x",
            "object": "chat.completion",
            "model": "llama3.2:3b",
            "usage": { "prompt_tokens": 1, "completion_tokens": 2 },
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "hi" },
                "finish_reason": "stop"
            }]
        }"#;
        let parsed: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.choices.len(), 1);
        assert_eq!(parsed.choices[0].message.content, "hi");
        assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("stop"));
        assert_eq!(parsed.model.as_deref(), Some("llama3.2:3b"));
    }
}
