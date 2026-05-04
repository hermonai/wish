//! Sessions namespace — get, list, revoke, revoke_all.

use std::sync::Arc;

use crate::error::HermonError;
use crate::transport::http::RequestOptions;
use crate::transport::HermonHttp;
use crate::types::common::Page;
use crate::types::session::Session;

/// Sessions namespace on the Hermon client.
pub struct SessionsNamespace {
    pub(crate) http: Arc<HermonHttp>,
}

impl SessionsNamespace {
    /// Get a specific session by id.
    pub async fn get(&self, session_id: &str) -> Result<Session, HermonError> {
        let path = format!("/v1/sessions/{session_id}");
        self.http.request(RequestOptions::get(path)).await
    }

    /// List sessions with optional pagination.
    pub async fn list(
        &self,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Page<Session>, HermonError> {
        let mut params = Vec::new();
        if let Some(o) = offset {
            params.push(("offset".to_string(), o.to_string()));
        }
        if let Some(l) = limit {
            params.push(("limit".to_string(), l.to_string()));
        }
        let mut opts = RequestOptions::get("/v1/sessions");
        if !params.is_empty() {
            opts = opts.with_query(params);
        }
        self.http.request(opts).await
    }

    /// Revoke a specific session.
    pub async fn revoke(&self, session_id: &str) -> Result<(), HermonError> {
        let path = format!("/v1/sessions/{session_id}");
        let _: serde_json::Value = self.http.request(RequestOptions::delete(path)).await?;
        Ok(())
    }

    /// Revoke all sessions except the current one.
    pub async fn revoke_all(&self) -> Result<(), HermonError> {
        let _: serde_json::Value = self
            .http
            .request(RequestOptions::post("/v1/sessions/revoke-all"))
            .await?;
        Ok(())
    }
}
