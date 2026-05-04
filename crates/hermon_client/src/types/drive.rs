//! Wish Drive (cloud storage) wire types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Drive object — any storable entity in Wish Drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveObject {
    pub id: String,
    pub name: String,
    pub object_type: DriveObjectType,
    pub owner_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub visibility: DriveVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub content_hash: Option<String>,
    pub size_bytes: Option<u64>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

/// Drive object types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveObjectType {
    /// Folder (container for other objects).
    Folder,
    /// Terminal workflow (sequence of commands).
    Workflow,
    /// AI-powered agent workflow.
    AgentWorkflow,
    /// Notebook (code + markdown cells).
    Notebook,
    /// AI fact / knowledge snippet.
    AiFact,
    /// AI fact collection.
    AiFactCollection,
    /// Environment variable collection.
    EnvVarCollection,
    /// MCP server configuration.
    McpServer,
    /// MCP server collection.
    McpServerCollection,
    /// Agent definition (stored in drive).
    AgentConfig,
    /// Generic file.
    File,
    /// Prompt template.
    PromptTemplate,
    /// Code snippet.
    Snippet,
}

/// Drive visibility / sharing level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveVisibility {
    Private,
    Org,
    Public,
}

/// Request to create a drive object.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDriveObjectRequest {
    pub name: String,
    pub object_type: DriveObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<DriveVisibility>,
    /// Inline content (for small objects). For large objects, use upload endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
}

/// Request to update a drive object.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDriveObjectRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<DriveVisibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
}

/// Drive listing filters.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveListFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_type: Option<DriveObjectType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<DriveVisibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Include soft-deleted objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_deleted: Option<bool>,
}

/// Drive sync state — used for incremental sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveSyncState {
    pub cursor: String,
    pub has_more: bool,
    pub changes: Vec<DriveSyncChange>,
}

/// A single change in the sync stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveSyncChange {
    pub object_id: String,
    pub change_type: SyncChangeType,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<DriveObject>,
}

/// Type of sync change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncChangeType {
    Created,
    Updated,
    Deleted,
    Moved,
}

/// Drive object content (retrieved separately for large objects).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveObjectContent {
    pub object_id: String,
    pub content_type: String,
    pub content: serde_json::Value,
    pub version: u64,
}
