//! AI model routing wire types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model reference (provider + model id).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    pub provider_id: String,
    pub model_id: String,
}

/// AI send request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSendRequest {
    pub session_id: String,
    pub model: ModelRef,
    pub messages: Vec<AiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AiToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<AiRequestParameters>,
}

/// AI message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    pub id: String,
    pub role: String,
    pub blocks: Vec<serde_json::Value>,
    pub created_at: String,
}

/// Tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Request parameters.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
}

/// AI stream event (SSE payload).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub enum AiStreamEvent {
    #[serde(rename = "response.started")]
    ResponseStarted {
        #[serde(rename = "responseId")]
        response_id: String,
    },
    #[serde(rename = "content.delta")]
    ContentDelta { delta: serde_json::Value },
    #[serde(rename = "content.completed")]
    ContentCompleted { block: serde_json::Value },
    #[serde(rename = "tool_use.started")]
    ToolUseStarted { tool: serde_json::Value },
    #[serde(rename = "tool_use.completed")]
    ToolUseCompleted { invocation: serde_json::Value },
    #[serde(rename = "usage")]
    Usage { usage: AiUsage },
    #[serde(rename = "response.completed")]
    ResponseCompleted {
        message: AiMessage,
        #[serde(rename = "finishReason")]
        finish_reason: String,
    },
    #[serde(rename = "error")]
    Error { code: String, message: String },
    #[serde(rename = "done")]
    Done,
}

/// Usage counters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
}

/// Model info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: String,
    pub provider_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Model listing filters.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

/// Telemetry batch event for ingestion.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub ts: String,
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, serde_json::Value>>,
}

/// Telemetry batch request.
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryBatch {
    pub events: Vec<TelemetryEvent>,
}
