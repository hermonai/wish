//! Core HTTP transport for Hermon API.
//!
//! Handles auth injection, 401→refresh→retry, RFC 7807 problem+json
//! error mapping, and bounded exponential backoff for idempotent
//! requests.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::{HermonClientConfig, RetryConfig};
use crate::error::{status_to_error, HermonError, ProblemDetails};
use crate::store::TokenStore;

/// HTTP method classification for idempotency.
fn is_idempotent(method: &reqwest::Method) -> bool {
    matches!(
        *method,
        reqwest::Method::GET
            | reqwest::Method::HEAD
            | reqwest::Method::PUT
            | reqwest::Method::DELETE
    )
}

/// Compute backoff with jitter: base * 2^attempt * random(0.5..1.0).
fn backoff_ms(attempt: u32, config: &RetryConfig) -> u64 {
    let base = config.base_ms as u64 * (1u64 << attempt);
    let capped = base.min(config.cap_ms as u64);
    // deterministic jitter: 75% of capped value
    capped * 3 / 4
}

/// Request configuration for a single API call.
pub struct RequestOptions {
    pub method: reqwest::Method,
    pub path: String,
    pub query: Option<Vec<(String, String)>>,
    pub body: Option<serde_json::Value>,
    pub idempotent: bool,
    pub skip_auth: bool,
}

impl RequestOptions {
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            method: reqwest::Method::GET,
            path: path.into(),
            query: None,
            body: None,
            idempotent: true,
            skip_auth: false,
        }
    }

    pub fn post(path: impl Into<String>) -> Self {
        Self {
            method: reqwest::Method::POST,
            path: path.into(),
            query: None,
            body: None,
            idempotent: false,
            skip_auth: false,
        }
    }

    pub fn put(path: impl Into<String>) -> Self {
        Self {
            method: reqwest::Method::PUT,
            path: path.into(),
            query: None,
            body: None,
            idempotent: true,
            skip_auth: false,
        }
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self {
            method: reqwest::Method::DELETE,
            path: path.into(),
            query: None,
            body: None,
            idempotent: true,
            skip_auth: false,
        }
    }

    pub fn with_body(mut self, body: impl serde::Serialize) -> Self {
        self.body = Some(serde_json::to_value(body).expect("serialize body"));
        self
    }

    pub fn with_query(mut self, params: Vec<(String, String)>) -> Self {
        self.query = Some(params);
        self
    }

    pub fn with_idempotent(mut self, v: bool) -> Self {
        self.idempotent = v;
        self
    }

    pub fn with_skip_auth(mut self, v: bool) -> Self {
        self.skip_auth = v;
        self
    }
}

/// Core HTTP transport.
pub struct HermonHttp {
    client: reqwest::Client,
    config: HermonClientConfig,
    token_store: Arc<dyn TokenStore>,
    /// Coalesces concurrent refresh attempts.
    refresh_lock: Mutex<()>,
}

impl HermonHttp {
    pub fn new(config: HermonClientConfig, token_store: Arc<dyn TokenStore>) -> Self {
        // Ensure a crypto provider is installed (no-op if already set).
        let _ = rustls::crypto::ring::default_provider().install_default();

        let client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("build reqwest client");

        Self {
            client,
            config,
            token_store,
            refresh_lock: Mutex::new(()),
        }
    }

    /// Full URL for a path.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    /// Attach auth header from token store.
    fn auth_header(&self) -> Option<String> {
        if let Some(access) = self.token_store.get_access() {
            return Some(format!("Bearer {access}"));
        }
        if let Some(api) = self.token_store.get_api_token() {
            return Some(format!("Bearer {api}"));
        }
        None
    }

    /// Execute a typed request with retry.
    pub async fn request<T: serde::de::DeserializeOwned>(
        &self,
        opts: RequestOptions,
    ) -> Result<T, HermonError> {
        let should_retry = opts.idempotent || is_idempotent(&opts.method);
        let max_attempts = if should_retry {
            self.config.retry.max_attempts
        } else {
            1
        };

        let mut last_err = None;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                let delay = backoff_ms(attempt - 1, &self.config.retry);
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }

            match self.single_request::<T>(&opts).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if !e.is_retryable() || !should_retry {
                        return Err(e);
                    }
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| HermonError::Network {
            message: "all retry attempts exhausted".into(),
            source: None,
        }))
    }

    /// Execute a single request (no retry).
    async fn single_request<T: serde::de::DeserializeOwned>(
        &self,
        opts: &RequestOptions,
    ) -> Result<T, HermonError> {
        let url = self.url(&opts.path);
        let mut builder = self.client.request(opts.method.clone(), &url);

        // Query params
        if let Some(ref params) = opts.query {
            builder = builder.query(params);
        }

        // Auth
        if !opts.skip_auth {
            if let Some(auth) = self.auth_header() {
                builder = builder.header("Authorization", auth);
            }
        }

        // Body
        if let Some(ref body) = opts.body {
            builder = builder
                .header("Content-Type", "application/json")
                .json(body);
        }

        builder = builder.header("Accept", "application/json");

        let response = builder.send().await?;
        let status = response.status().as_u16();

        // Handle 401 — refresh and retry once
        if status == 401 && !opts.skip_auth {
            if self.try_refresh().await {
                // Retry with new token
                return self.retry_with_new_token::<T>(opts).await;
            }
            // Refresh failed — return auth error
            return Err(HermonError::Auth {
                message: "authentication failed".into(),
                request_id: None,
            });
        }

        // Handle 429 — parse Retry-After
        if status == 429 {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(|secs| secs * 1000);
            return Err(HermonError::RateLimited {
                retry_after_ms: retry_after,
                request_id: None,
            });
        }

        // Error responses
        if !response.status().is_success() {
            let problem = response.json::<ProblemDetails>().await.ok();
            return Err(status_to_error(status, problem));
        }

        // Success — parse JSON
        let body = response
            .json::<T>()
            .await
            .map_err(|e| HermonError::Network {
                message: format!("failed to parse response: {e}"),
                source: Some(e),
            })?;

        Ok(body)
    }

    /// Attempt to refresh the access token. Returns true on success.
    async fn try_refresh(&self) -> bool {
        let _lock = self.refresh_lock.lock().await;

        // Double-check: maybe another concurrent call already refreshed
        if self.token_store.get_access().is_some() {
            return true;
        }

        let refresh_token = match self.token_store.get_refresh() {
            Some(t) => t,
            None => return false,
        };

        let url = self.url("/v1/auth/refresh");
        let body = serde_json::json!({ "refreshToken": refresh_token });

        let result = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(tokens) = resp.json::<crate::types::session::AuthTokens>().await {
                    self.token_store.set_tokens(
                        tokens.access_token,
                        tokens.refresh_token,
                        tokens.expires_at,
                    );
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Retry a request with the freshly refreshed token.
    async fn retry_with_new_token<T: serde::de::DeserializeOwned>(
        &self,
        opts: &RequestOptions,
    ) -> Result<T, HermonError> {
        let url = self.url(&opts.path);
        let mut builder = self.client.request(opts.method.clone(), &url);

        if let Some(ref params) = opts.query {
            builder = builder.query(params);
        }

        if let Some(auth) = self.auth_header() {
            builder = builder.header("Authorization", auth);
        }

        if let Some(ref body) = opts.body {
            builder = builder
                .header("Content-Type", "application/json")
                .json(body);
        }

        builder = builder.header("Accept", "application/json");

        let response = builder.send().await?;
        let status = response.status().as_u16();

        if !response.status().is_success() {
            let problem = response.json::<ProblemDetails>().await.ok();
            return Err(status_to_error(status, problem));
        }

        response
            .json::<T>()
            .await
            .map_err(|e| HermonError::Network {
                message: format!("failed to parse response: {e}"),
                source: Some(e),
            })
    }

    /// Raw request returning a reqwest::Response (for downloads, SSE setup, etc.).
    pub async fn raw_request(
        &self,
        opts: &RequestOptions,
    ) -> Result<reqwest::Response, HermonError> {
        let url = self.url(&opts.path);
        let mut builder = self.client.request(opts.method.clone(), &url);

        if let Some(ref params) = opts.query {
            builder = builder.query(params);
        }

        if !opts.skip_auth {
            if let Some(auth) = self.auth_header() {
                builder = builder.header("Authorization", auth);
            }
        }

        if let Some(ref body) = opts.body {
            builder = builder
                .header("Content-Type", "application/json")
                .json(body);
        }

        let response = builder.send().await?;
        let status = response.status().as_u16();

        if !response.status().is_success() {
            let problem = response.json::<ProblemDetails>().await.ok();
            return Err(status_to_error(status, problem));
        }

        Ok(response)
    }

    /// Access the config.
    pub fn config(&self) -> &HermonClientConfig {
        &self.config
    }

    /// Access the token store.
    pub fn token_store(&self) -> &Arc<dyn TokenStore> {
        &self.token_store
    }
}
