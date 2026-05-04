//! Client configuration.

/// Configuration for the Hermon client.
#[derive(Debug, Clone)]
pub struct HermonClientConfig {
    /// Base URL of the Hermon server (e.g. "https://api.hermon.ai").
    pub base_url: String,
    /// User-Agent header value.
    pub user_agent: String,
    /// Retry configuration for idempotent requests.
    pub retry: RetryConfig,
}

/// Retry configuration.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the first).
    pub max_attempts: u32,
    /// Base delay in milliseconds for exponential backoff.
    pub base_ms: u32,
    /// Maximum delay cap in milliseconds.
    pub cap_ms: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_ms: 200,
            cap_ms: 10_000,
        }
    }
}

impl Default for HermonClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:9100".into(),
            user_agent: "hermon-client-rs/0.1.0".into(),
            retry: RetryConfig::default(),
        }
    }
}

impl HermonClientConfig {
    /// Create a new config pointing at the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }
}
