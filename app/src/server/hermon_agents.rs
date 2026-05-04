//! Hermon Agents service layer.
//!
//! Bridges the `hermon_client` agent/conversation/drive namespaces into the
//! Wish app runtime. Manages agent listing, invocation, and conversation state
//! through the Hermon control plane.

use std::sync::Arc;

use hermon_client::types::agent::{
    Agent, AgentFilters, AgentStreamEvent, CreateAgentRequest, InvokeAgentRequest,
    UpdateAgentRequest,
};
use hermon_client::types::common::{Page, PageParams};
use hermon_client::types::conversation::{
    Conversation, ConversationFilters, ConversationMessage, ConversationSummary,
    SendMessageRequest,
};
use hermon_client::types::drive::{
    CreateDriveObjectRequest, DriveListFilters, DriveObject, DriveSyncState,
    UpdateDriveObjectRequest,
};
use hermon_client::HermonClient;

use super::hermon_auth;

/// Service for managing agents through the Hermon backend.
///
/// Thread-safe wrapper around `HermonClient` that can be shared across
/// the app's async tasks.
#[allow(dead_code)]
pub struct HermonAgentService {
    client: Arc<HermonClient>,
}

#[allow(dead_code)]
impl HermonAgentService {
    /// Create a new agent service using the global Hermon client configuration.
    pub fn new() -> Option<Self> {
        let client = hermon_auth::create_client()?;
        Some(Self {
            client: Arc::new(client),
        })
    }

    /// Create from an existing client (for testing).
    pub fn from_client(client: Arc<HermonClient>) -> Self {
        Self { client }
    }

    /// Access the underlying client.
    pub fn client(&self) -> &HermonClient {
        &self.client
    }

    // ── Agents ────────────────────────────────────────────────────

    /// List all available agents (user's own + system built-ins).
    pub async fn list_agents(
        &self,
        filters: Option<AgentFilters>,
        page: Option<PageParams>,
    ) -> Result<Page<Agent>, hermon_client::HermonError> {
        self.client.agents.list(filters, page).await
    }

    /// List only built-in SDLC agents.
    pub async fn list_builtin_agents(&self) -> Result<Vec<Agent>, hermon_client::HermonError> {
        self.client.agents.list_builtin().await
    }

    /// Get an agent by ID.
    pub async fn get_agent(
        &self,
        agent_id: &str,
    ) -> Result<Option<Agent>, hermon_client::HermonError> {
        self.client.agents.get(agent_id).await
    }

    /// Get an agent by slug (e.g. "wish-coder").
    pub async fn get_agent_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<Agent>, hermon_client::HermonError> {
        self.client.agents.get_by_slug(slug).await
    }

    /// Create a custom agent.
    pub async fn create_agent(
        &self,
        request: CreateAgentRequest,
    ) -> Result<Agent, hermon_client::HermonError> {
        self.client.agents.create(request).await
    }

    /// Update an existing agent.
    pub async fn update_agent(
        &self,
        agent_id: &str,
        request: UpdateAgentRequest,
    ) -> Result<Agent, hermon_client::HermonError> {
        self.client.agents.update(agent_id, request).await
    }

    /// Delete a custom agent.
    pub async fn delete_agent(
        &self,
        agent_id: &str,
    ) -> Result<(), hermon_client::HermonError> {
        self.client.agents.delete(agent_id).await
    }

    /// Invoke an agent with streaming response.
    pub async fn invoke_agent(
        &self,
        agent_id: &str,
        request: InvokeAgentRequest,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<AgentStreamEvent, hermon_client::HermonError>>,
        hermon_client::HermonError,
    > {
        self.client.agents.invoke(agent_id, request).await
    }

    // ── Conversations ─────────────────────────────────────────────

    /// List conversations with optional filters.
    pub async fn list_conversations(
        &self,
        filters: Option<ConversationFilters>,
        page: Option<PageParams>,
    ) -> Result<Page<ConversationSummary>, hermon_client::HermonError> {
        self.client.conversations.list(filters, page).await
    }

    /// Get a specific conversation.
    pub async fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<Conversation>, hermon_client::HermonError> {
        self.client.conversations.get(conversation_id).await
    }

    /// Get messages in a conversation.
    pub async fn get_messages(
        &self,
        conversation_id: &str,
        page: Option<PageParams>,
    ) -> Result<Page<ConversationMessage>, hermon_client::HermonError> {
        self.client.conversations.messages(conversation_id, page).await
    }

    /// Send a message to a conversation with streaming response.
    pub async fn send_message(
        &self,
        conversation_id: &str,
        request: SendMessageRequest,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<AgentStreamEvent, hermon_client::HermonError>>,
        hermon_client::HermonError,
    > {
        self.client
            .conversations
            .send_message(conversation_id, request)
            .await
    }

    /// Archive a conversation.
    pub async fn archive_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Conversation, hermon_client::HermonError> {
        self.client.conversations.archive(conversation_id).await
    }

    /// Delete a conversation.
    pub async fn delete_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<(), hermon_client::HermonError> {
        self.client.conversations.delete(conversation_id).await
    }

    // ── Drive ─────────────────────────────────────────────────────

    /// List drive objects (workflows, notebooks, etc.).
    pub async fn list_drive_objects(
        &self,
        filters: Option<DriveListFilters>,
        page: Option<PageParams>,
    ) -> Result<Page<DriveObject>, hermon_client::HermonError> {
        self.client.drive.list(filters, page).await
    }

    /// Create a drive object.
    pub async fn create_drive_object(
        &self,
        request: CreateDriveObjectRequest,
    ) -> Result<DriveObject, hermon_client::HermonError> {
        self.client.drive.create(request).await
    }

    /// Update a drive object.
    pub async fn update_drive_object(
        &self,
        object_id: &str,
        request: UpdateDriveObjectRequest,
    ) -> Result<DriveObject, hermon_client::HermonError> {
        self.client.drive.update(object_id, request).await
    }

    /// Delete a drive object.
    pub async fn delete_drive_object(
        &self,
        object_id: &str,
    ) -> Result<(), hermon_client::HermonError> {
        self.client.drive.delete(object_id).await
    }

    /// Sync drive changes since cursor.
    pub async fn sync_drive(
        &self,
        cursor: Option<&str>,
    ) -> Result<DriveSyncState, hermon_client::HermonError> {
        self.client.drive.sync(cursor).await
    }

    // ── Health ────────────────────────────────────────────────────

    /// Check if the Hermon backend is reachable.
    pub async fn health_check(&self) -> bool {
        self.client.health().await.is_ok()
    }

    /// Get Hermon server version.
    pub async fn server_version(&self) -> Option<String> {
        self.client
            .version()
            .await
            .ok()
            .map(|v| v.version)
    }
}
