//! Orgs namespace — get, list, list_members.

use std::sync::Arc;

use crate::error::HermonError;
use crate::transport::http::RequestOptions;
use crate::transport::HermonHttp;
use crate::types::common::Page;
use crate::types::session::{Org, OrgMember, User};

/// Orgs namespace on the Hermon client.
pub struct OrgsNamespace {
    pub(crate) http: Arc<HermonHttp>,
}

impl OrgsNamespace {
    /// Get an organisation by id.
    pub async fn get(&self, org_id: &str) -> Result<Org, HermonError> {
        let path = format!("/v1/orgs/{org_id}");
        self.http.request(RequestOptions::get(path)).await
    }

    /// List organisations the current user belongs to.
    pub async fn list(
        &self,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Page<Org>, HermonError> {
        let mut params = Vec::new();
        if let Some(o) = offset {
            params.push(("offset".to_string(), o.to_string()));
        }
        if let Some(l) = limit {
            params.push(("limit".to_string(), l.to_string()));
        }
        let mut opts = RequestOptions::get("/v1/orgs");
        if !params.is_empty() {
            opts = opts.with_query(params);
        }
        self.http.request(opts).await
    }

    /// List members of an organisation.
    pub async fn list_members(
        &self,
        org_id: &str,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Page<OrgMember>, HermonError> {
        let mut params = Vec::new();
        if let Some(o) = offset {
            params.push(("offset".to_string(), o.to_string()));
        }
        if let Some(l) = limit {
            params.push(("limit".to_string(), l.to_string()));
        }
        let path = format!("/v1/orgs/{org_id}/members");
        let mut opts = RequestOptions::get(path);
        if !params.is_empty() {
            opts = opts.with_query(params);
        }
        self.http.request(opts).await
    }

    /// Get a user by id.
    pub async fn get_user(&self, user_id: &str) -> Result<User, HermonError> {
        let path = format!("/v1/users/{user_id}");
        self.http.request(RequestOptions::get(path)).await
    }
}
