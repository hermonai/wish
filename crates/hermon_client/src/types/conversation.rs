//! Conversation and message history wire types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A conversation with an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub agent_id: String,
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    pub title: Option<String>,
    pub status: ConversationStatus,
    pub message_count: u32,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: String,
    pub updated_at: String,
}

/// Conversation status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    Active,
    Completed,
    Archived,
    Error,
}

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: Vec<MessageBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_invocations: Option<Vec<ToolInvocation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<super::ai::AiUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: String,
}

/// Message role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// A content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageBlock {
    /// Plain text content.
    Text { text: String },
    /// Code block with optional language.
    Code {
        code: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        language: Option<String>,
    },
    /// Thinking/reasoning content (may be hidden in UI).
    Thinking { text: String },
    /// Tool use block.
    ToolUse {
        #[serde(rename = "toolId")]
        tool_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        input: serde_json::Value,
    },
    /// Tool result block.
    ToolResult {
        #[serde(rename = "toolId")]
        tool_id: String,
        content: serde_json::Value,
        #[serde(default, rename = "isError")]
        is_error: bool,
    },
    /// File reference.
    FileRef {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
    },
    /// Image content.
    Image {
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        base64: Option<String>,
        #[serde(rename = "mediaType")]
        media_type: String,
    },
}

/// Record of a tool invocation within a turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvocation {
    pub id: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub status: ToolInvocationStatus,
    pub duration_ms: Option<u64>,
    pub created_at: String,
}

/// Tool invocation status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    ApprovalRequired,
}

/// Request to create a conversation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversationRequest {
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Conversation listing filters.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ConversationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
}

/// Request to send a message in a conversation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<super::agent::AgentContext>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Conversation summary (for listing without full message history).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub title: Option<String>,
    pub status: ConversationStatus,
    pub message_count: u32,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<String>,
    pub created_at: String,
}
