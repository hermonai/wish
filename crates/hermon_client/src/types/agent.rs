//! Agent management wire types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent definition — a reusable, configurable AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub model: AgentModelConfig,
    #[serde(default)]
    pub tools: Vec<AgentToolRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub agent_type: AgentType,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<AgentParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub owner_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    pub visibility: AgentVisibility,
    pub created_at: String,
    pub updated_at: String,
}

/// Agent type classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// General-purpose conversational agent.
    Chat,
    /// Code-generation agent with tool use.
    Coding,
    /// Orchestrator that delegates to sub-agents.
    Orchestrator,
    /// Background worker (runs on schedule or event).
    Worker,
    /// SDLC lifecycle agent (planning, review, test, deploy).
    Sdlc,
    /// Custom agent type.
    Custom,
}

/// Agent visibility scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentVisibility {
    /// Only the owner can see and use.
    Private,
    /// Visible to org members.
    Org,
    /// Publicly listed in registry.
    Public,
    /// Built-in system agent.
    System,
}

/// Agent model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelConfig {
    pub provider_id: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

/// Reference to a tool available to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolRef {
    /// Tool ID or name (e.g. "file_read", "shell_exec", "mcp:github").
    pub tool_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    /// Whether the tool requires user confirmation before execution.
    #[serde(default)]
    pub requires_approval: bool,
}

/// Agent runtime parameters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_on_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

/// Request to create a new agent.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentRequest {
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub model: AgentModelConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AgentToolRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub agent_type: AgentType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<AgentParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<AgentVisibility>,
}

/// Request to update an agent.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AgentToolRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<AgentParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<AgentVisibility>,
}

/// Agent invocation request — start a conversation turn.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokeAgentRequest {
    /// Conversation ID (creates new conversation if None).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// User message to send to the agent.
    pub message: String,
    /// Additional context (e.g., file contents, terminal output).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<AgentContext>>,
    /// Override parameters for this invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<AgentParameters>,
}

/// Context attachment for an agent invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContext {
    #[serde(rename = "type")]
    pub context_type: AgentContextType,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Types of context that can be attached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentContextType {
    File,
    Terminal,
    Selection,
    Diff,
    Error,
    Url,
    Custom,
}

/// Agent stream event (SSE payload during invocation).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub enum AgentStreamEvent {
    /// Invocation started.
    #[serde(rename = "invocation.started")]
    InvocationStarted {
        #[serde(rename = "invocationId")]
        invocation_id: String,
        #[serde(rename = "conversationId")]
        conversation_id: String,
    },
    /// Text content streaming.
    #[serde(rename = "content.delta")]
    ContentDelta { delta: String },
    /// Agent is thinking/reasoning.
    #[serde(rename = "thinking.delta")]
    ThinkingDelta { delta: String },
    /// Tool use initiated.
    #[serde(rename = "tool.started")]
    ToolStarted {
        #[serde(rename = "toolId")]
        tool_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        input: serde_json::Value,
    },
    /// Tool result returned.
    #[serde(rename = "tool.completed")]
    ToolCompleted {
        #[serde(rename = "toolId")]
        tool_id: String,
        result: serde_json::Value,
        #[serde(rename = "isError")]
        #[serde(default)]
        is_error: bool,
    },
    /// Agent requesting user approval for a tool.
    #[serde(rename = "tool.approval_required")]
    ToolApprovalRequired {
        #[serde(rename = "toolId")]
        tool_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        input: serde_json::Value,
        description: String,
    },
    /// Agent delegating to a sub-agent.
    #[serde(rename = "delegation.started")]
    DelegationStarted {
        #[serde(rename = "targetAgentId")]
        target_agent_id: String,
        #[serde(rename = "targetAgentName")]
        target_agent_name: String,
        reason: String,
    },
    /// Sub-agent delegation completed.
    #[serde(rename = "delegation.completed")]
    DelegationCompleted {
        #[serde(rename = "targetAgentId")]
        target_agent_id: String,
        result: serde_json::Value,
    },
    /// Turn completed.
    #[serde(rename = "invocation.completed")]
    InvocationCompleted {
        message: super::ai::AiMessage,
        #[serde(rename = "finishReason")]
        finish_reason: String,
        usage: Option<super::ai::AiUsage>,
    },
    /// Error during invocation.
    #[serde(rename = "error")]
    Error { code: String, message: String },
    /// Stream done.
    #[serde(rename = "done")]
    Done,
}

/// Agent listing filters.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<AgentType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<AgentVisibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
}

/// Tool approval response (sent back after ToolApprovalRequired).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalResponse {
    pub tool_id: String,
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_input: Option<serde_json::Value>,
}
