//! Agents namespace — CRUD, invoke, list.

use std::sync::Arc;

use futures::stream::Stream;

use crate::error::HermonError;
use crate::transport::http::RequestOptions;
use crate::transport::sse::parse_sse_stream;
use crate::transport::HermonHttp;
use crate::types::agent::*;
use crate::types::common::{Page, PageParams};

/// Agents namespace on the Hermon client.
pub struct AgentsNamespace {
    pub(crate) http: Arc<HermonHttp>,
}

impl AgentsNamespace {
    /// Create a new agent definition.
    pub async fn create(&self, input: CreateAgentRequest) -> Result<Agent, HermonError> {
        let opts = RequestOptions::post("/v1/agents").with_body(&input);
        self.http.request(opts).await
    }

    /// Get an agent by ID.
    pub async fn get(&self, agent_id: &str) -> Result<Option<Agent>, HermonError> {
        let path = format!("/v1/agents/{agent_id}");
        match self.http.request(RequestOptions::get(path)).await {
            Ok(a) => Ok(Some(a)),
            Err(HermonError::Server { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get an agent by slug.
    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<Agent>, HermonError> {
        let path = format!("/v1/agents/slug/{slug}");
        match self.http.request(RequestOptions::get(path)).await {
            Ok(a) => Ok(Some(a)),
            Err(HermonError::Server { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// List agents with optional filters and pagination.
    pub async fn list(
        &self,
        filters: Option<AgentFilters>,
        page: Option<PageParams>,
    ) -> Result<Page<Agent>, HermonError> {
        let mut params = Vec::new();

        if let Some(f) = filters {
            if let Some(t) = f.agent_type {
                params.push((
                    "agentType".to_string(),
                    serde_json::to_string(&t)
                        .unwrap()
                        .trim_matches('"')
                        .to_string(),
                ));
            }
            if let Some(v) = f.visibility {
                params.push((
                    "visibility".to_string(),
                    serde_json::to_string(&v)
                        .unwrap()
                        .trim_matches('"')
                        .to_string(),
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

        let mut opts = RequestOptions::get("/v1/agents");
        if !params.is_empty() {
            opts = opts.with_query(params);
        }

        self.http.request(opts).await
    }

    /// Update an existing agent.
    pub async fn update(
        &self,
        agent_id: &str,
        input: UpdateAgentRequest,
    ) -> Result<Agent, HermonError> {
        let path = format!("/v1/agents/{agent_id}");
        let opts = RequestOptions::put(path).with_body(&input);
        self.http.request(opts).await
    }

    /// Delete an agent.
    pub async fn delete(&self, agent_id: &str) -> Result<(), HermonError> {
        let path = format!("/v1/agents/{agent_id}");
        let opts = RequestOptions::delete(path);
        // DELETE returns 204 no content, but our request() expects a body.
        // Use raw_request and just check status.
        self.http.raw_request(&opts).await?;
        Ok(())
    }

    /// Invoke an agent, returning a stream of SSE events.
    ///
    /// This starts a new conversation turn. If `conversation_id` is set in the
    /// request, it continues an existing conversation; otherwise a new one is created.
    pub async fn invoke(
        &self,
        agent_id: &str,
        input: InvokeAgentRequest,
    ) -> Result<impl Stream<Item = Result<AgentStreamEvent, HermonError>>, HermonError> {
        let path = format!("/v1/agents/{agent_id}/invoke");
        let opts = RequestOptions::post(path).with_body(&input);
        let response = self.http.raw_request(&opts).await?;
        Ok(parse_sse_stream(response))
    }

    /// Send a tool approval response during an active invocation.
    pub async fn approve_tool(
        &self,
        agent_id: &str,
        invocation_id: &str,
        approval: ToolApprovalResponse,
    ) -> Result<(), HermonError> {
        let path = format!("/v1/agents/{agent_id}/invocations/{invocation_id}/approve");
        let opts = RequestOptions::post(path).with_body(&approval);
        self.http.raw_request(&opts).await?;
        Ok(())
    }

    /// List built-in system agents.
    pub async fn list_builtin(&self) -> Result<Vec<Agent>, HermonError> {
        self.http
            .request(RequestOptions::get("/v1/agents/builtin"))
            .await
    }

    /// Clone an agent (fork from existing).
    pub async fn clone_agent(&self, agent_id: &str, new_name: &str) -> Result<Agent, HermonError> {
        let path = format!("/v1/agents/{agent_id}/clone");
        let body = serde_json::json!({ "name": new_name });
        let opts = RequestOptions::post(path).with_body(&body);
        self.http.request(opts).await
    }
}
