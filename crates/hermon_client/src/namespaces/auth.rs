//! Auth namespace — register, login, refresh, session, me, logout.

use std::sync::Arc;

use crate::error::HermonError;
use crate::transport::HermonHttp;
use crate::types::session::*;

/// Auth namespace on the Hermon client.
pub struct AuthNamespace {
    pub(crate) http: Arc<HermonHttp>,
}

impl AuthNamespace {
    /// Register a new device, receiving auth tokens.
    pub async fn register(&self, reg: DeviceRegistration) -> Result<AuthTokens, HermonError> {
        let tokens: AuthTokens = self
            .http
            .request(
                crate::transport::http::RequestOptions::post("/v1/auth/register")
                    .with_body(&reg)
                    .with_skip_auth(true),
            )
            .await?;

        self.http.token_store().set_tokens(
            tokens.access_token.clone(),
            tokens.refresh_token.clone(),
            tokens.expires_at,
        );

        Ok(tokens)
    }

    /// Signup — create a new user account with email + password.
    pub async fn signup(&self, req: SignupRequest) -> Result<SignupResponse, HermonError> {
        let resp: SignupResponse = self
            .http
            .request(
                crate::transport::http::RequestOptions::post("/v1/auth/register")
                    .with_body(&req)
                    .with_skip_auth(true),
            )
            .await?;

        // Store tokens for immediate use
        let expires_at = chrono::Utc::now().timestamp_millis() + (15 * 60 * 1000); // 15 min
        self.http.token_store().set_tokens(
            resp.access_token.clone(),
            resp.refresh_token.clone(),
            expires_at,
        );

        Ok(resp)
    }

    /// Login with email + password.
    pub async fn login(&self, req: LoginRequest) -> Result<AuthTokens, HermonError> {
        let tokens: AuthTokens = self
            .http
            .request(
                crate::transport::http::RequestOptions::post("/v1/auth/login")
                    .with_body(&req)
                    .with_skip_auth(true),
            )
            .await?;

        self.http.token_store().set_tokens(
            tokens.access_token.clone(),
            tokens.refresh_token.clone(),
            tokens.expires_at,
        );

        Ok(tokens)
    }

    /// Login with an API token.
    pub async fn login_with_api_token(
        &self,
        req: ApiTokenLoginRequest,
    ) -> Result<AuthTokens, HermonError> {
        let tokens: AuthTokens = self
            .http
            .request(
                crate::transport::http::RequestOptions::post("/v1/auth/login/token")
                    .with_body(&req)
                    .with_skip_auth(true),
            )
            .await?;

        self.http.token_store().set_tokens(
            tokens.access_token.clone(),
            tokens.refresh_token.clone(),
            tokens.expires_at,
        );

        Ok(tokens)
    }

    /// Get the current session.
    pub async fn session(&self) -> Result<Session, HermonError> {
        self.http
            .request(crate::transport::http::RequestOptions::get(
                "/v1/auth/session",
            ))
            .await
    }

    /// Get the current user profile.
    pub async fn me(&self) -> Result<User, HermonError> {
        self.http
            .request(crate::transport::http::RequestOptions::get("/v1/auth/me"))
            .await
    }

    /// Logout — revokes the current session and clears tokens.
    pub async fn logout(&self) -> Result<(), HermonError> {
        let result: serde_json::Value = self
            .http
            .request(crate::transport::http::RequestOptions::post(
                "/v1/auth/logout",
            ))
            .await?;
        let _ = result; // ignore response body
        self.http.token_store().clear();
        Ok(())
    }
}
