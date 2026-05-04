//! Conversations namespace — history, messages, management.

use std::sync::Arc;

use futures::stream::Stream;

use crate::error::HermonError;
use crate::transport::http::RequestOptions;
use crate::transport::sse::parse_sse_stream;
use crate::transport::HermonHttp;
use crate::types::agent::AgentStreamEvent;
use crate::types::common::{Page, PageParams};
use crate::types::conversation::*;

/// Conversations namespace on the Hermon client.
pub struct ConversationsNamespace {
    pub(crate) http: Arc<HermonHttp>,
}

impl ConversationsNamespace {
    /// Create a new conversation.
    pub async fn create(
        &self,
        input: CreateConversationRequest,
    ) -> Result<Conversation, HermonError> {
        let opts = RequestOptions::post("/v1/conversations").with_body(&input);
        self.http.request(opts).await
    }

    /// Get a conversation by ID.
    pub async fn get(&self, conversation_id: &str) -> Result<Option<Conversation>, HermonError> {
        let path = format!("/v1/conversations/{conversation_id}");
        match self.http.request(RequestOptions::get(path)).await {
            Ok(c) => Ok(Some(c)),
            Err(HermonError::Server { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// List conversations with optional filters and pagination.
    pub async fn list(
        &self,
        filters: Option<ConversationFilters>,
        page: Option<PageParams>,
    ) -> Result<Page<ConversationSummary>, HermonError> {
        let mut params = Vec::new();

        if let Some(f) = filters {
            if let Some(aid) = f.agent_id {
                params.push(("agentId".to_string(), aid));
            }
            if let Some(s) = f.status {
                params.push((
                    "status".to_string(),
                    serde_json::to_string(&s).unwrap().trim_matches('"').to_string(),
                ));
            }
            if let Some(s) = f.search {
                params.push(("search".to_string(), s));
            }
        }

        if let Some(p) = page {
            if let Some(o) = p.offset {
                params.push(("offset".to_string(), o.to_string()));
            }
            if let Some(l) = p.limit {
                params.push(("limit".to_string(), l.to_string()));
            }
        }

        let mut opts = RequestOptions::get("/v1/conversations");
        if !params.is_empty() {
            opts = opts.with_query(params);
        }

        self.http.request(opts).await
    }

    /// Get messages in a conversation (paginated, newest first).
    pub async fn messages(
        &self,
        conversation_id: &str,
        page: Option<PageParams>,
    ) -> Result<Page<ConversationMessage>, HermonError> {
        let mut params = Vec::new();

        if let Some(p) = page {
            if let Some(o) = p.offset {
                params.push(("offset".to_string(), o.to_string()));
            }
            if let Some(l) = p.limit {
                params.push(("limit".to_string(), l.to_string()));
            }
        }

        let path = format!("/v1/conversations/{conversation_id}/messages");
        let mut opts = RequestOptions::get(path);
        if !params.is_empty() {
            opts = opts.with_query(params);
        }

        self.http.request(opts).await
    }

    /// Send a message to a conversation, returning a stream of agent events.
    pub async fn send_message(
        &self,
        conversation_id: &str,
        input: SendMessageRequest,
    ) -> Result<impl Stream<Item = Result<AgentStreamEvent, HermonError>>, HermonError> {
        let path = format!("/v1/conversations/{conversation_id}/messages");
        let opts = RequestOptions::post(path).with_body(&input);
        let response = self.http.raw_request(&opts).await?;
        Ok(parse_sse_stream(response))
    }

    /// Archive a conversation.
    pub async fn archive(&self, conversation_id: &str) -> Result<Conversation, HermonError> {
        let path = format!("/v1/conversations/{conversation_id}/archive");
        let opts = RequestOptions::post(path);
        self.http.request(opts).await
    }

    /// Delete a conversation and all its messages.
    pub async fn delete(&self, conversation_id: &str) -> Result<(), HermonError> {
        let path = format!("/v1/conversations/{conversation_id}");
        self.http.raw_request(&RequestOptions::delete(path)).await?;
        Ok(())
    }

    /// Update conversation title.
    pub async fn update_title(
        &self,
        conversation_id: &str,
        title: &str,
    ) -> Result<Conversation, HermonError> {
        let path = format!("/v1/conversations/{conversation_id}");
        let body = serde_json::json!({ "title": title });
        let opts = RequestOptions::put(path).with_body(&body);
        self.http.request(opts).await
    }

    /// Fork a conversation from a specific message (create a branch).
    pub async fn fork(
        &self,
        conversation_id: &str,
        from_message_id: &str,
    ) -> Result<Conversation, HermonError> {
        let path = format!("/v1/conversations/{conversation_id}/fork");
        let body = serde_json::json!({ "fromMessageId": from_message_id });
        let opts = RequestOptions::post(path).with_body(&body);
        self.http.request(opts).await
    }

    /// Export a conversation as a structured document.
    pub async fn export(
        &self,
        conversation_id: &str,
        format: &str,
    ) -> Result<serde_json::Value, HermonError> {
        let path = format!("/v1/conversations/{conversation_id}/export");
        let opts =
            RequestOptions::get(path).with_query(vec![("format".to_string(), format.to_string())]);
        self.http.request(opts).await
    }
}
