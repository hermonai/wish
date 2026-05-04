//! AI namespace — send (SSE streaming), list models, get model.

use std::sync::Arc;

use futures::stream::Stream;

use crate::error::HermonError;
use crate::transport::http::RequestOptions;
use crate::transport::sse::parse_sse_stream;
use crate::transport::HermonHttp;
use crate::types::ai::*;

/// AI namespace on the Hermon client.
pub struct AiNamespace {
    pub(crate) http: Arc<HermonHttp>,
}

impl AiNamespace {
    /// Send an AI request, returning a stream of SSE events.
    ///
    /// Uses fetch-based streaming (not EventSource) to support POST
    /// body + custom headers. Each AI request is unique — no reconnect.
    pub async fn send(
        &self,
        input: AiSendRequest,
    ) -> Result<impl Stream<Item = Result<AiStreamEvent, HermonError>>, HermonError> {
        let opts = RequestOptions::post("/v1/ai/send").with_body(&input);
        let response = self.http.raw_request(&opts).await?;
        Ok(parse_sse_stream(response))
    }

    /// List available AI models, optionally filtered.
    pub async fn list_models(
        &self,
        filters: Option<ModelFilters>,
    ) -> Result<Vec<AiModel>, HermonError> {
        let mut opts = RequestOptions::get("/v1/ai/models");

        if let Some(f) = filters {
            let mut params = Vec::new();
            if let Some(p) = f.provider_id {
                params.push(("providerId".to_string(), p));
            }
            if let Some(c) = f.capability {
                params.push(("capability".to_string(), c));
            }
            if !params.is_empty() {
                opts = opts.with_query(params);
            }
        }

        self.http.request(opts).await
    }

    /// Get a specific model by id. Returns `None` on 404.
    pub async fn get_model(&self, model_id: &str) -> Result<Option<AiModel>, HermonError> {
        let path = format!("/v1/ai/models/{model_id}");
        match self.http.request(RequestOptions::get(path)).await {
            Ok(m) => Ok(Some(m)),
            Err(HermonError::Server { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
