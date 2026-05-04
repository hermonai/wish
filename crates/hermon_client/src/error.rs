//! Error hierarchy for Hermon client operations.
//!
//! Mirrors the 7-class TS hierarchy in `@wish/hermon-client`:
//! Auth (401), PolicyDenied (403), RateLimited (429),
//! Validation (400), Server (5xx), Network (transport), Stream (SSE).

use std::fmt;

/// Typed error from the Hermon API.
#[derive(Debug)]
pub enum HermonError {
    /// 401 — authentication required or failed.
    Auth {
        message: String,
        request_id: Option<String>,
    },
    /// 403 — policy denied the request.
    PolicyDenied {
        message: String,
        code: Option<String>,
        request_id: Option<String>,
    },
    /// 429 — rate limited.
    RateLimited {
        retry_after_ms: Option<u64>,
        request_id: Option<String>,
    },
    /// 400 — validation error.
    Validation {
        message: String,
        detail: Option<String>,
        request_id: Option<String>,
    },
    /// 5xx — server error.
    Server {
        status: u16,
        message: String,
        request_id: Option<String>,
    },
    /// Transport-layer error (DNS, TLS, timeout).
    Network {
        message: String,
        source: Option<reqwest::Error>,
    },
    /// SSE stream error.
    Stream { message: String },
}

impl HermonError {
    /// Whether this error should be retried on idempotent requests.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            HermonError::Server { .. }
                | HermonError::Network { .. }
                | HermonError::RateLimited { .. }
        )
    }

    /// Retry-After hint in milliseconds, if available.
    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            HermonError::RateLimited { retry_after_ms, .. } => *retry_after_ms,
            _ => None,
        }
    }
}

impl fmt::Display for HermonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HermonError::Auth { message, .. } => write!(f, "auth error: {message}"),
            HermonError::PolicyDenied { message, .. } => write!(f, "policy denied: {message}"),
            HermonError::RateLimited { .. } => write!(f, "rate limited"),
            HermonError::Validation { message, .. } => write!(f, "validation error: {message}"),
            HermonError::Server {
                status, message, ..
            } => {
                write!(f, "server error {status}: {message}")
            }
            HermonError::Network { message, .. } => write!(f, "network error: {message}"),
            HermonError::Stream { message } => write!(f, "stream error: {message}"),
        }
    }
}

impl std::error::Error for HermonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HermonError::Network {
                source: Some(e), ..
            } => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for HermonError {
    fn from(e: reqwest::Error) -> Self {
        HermonError::Network {
            message: e.to_string(),
            source: Some(e),
        }
    }
}

/// RFC 7807 problem+json envelope from Hermon.
///
/// Hermon sends `request_id` (snake_case) in some contexts and
/// `requestId` (camelCase) in others. We accept both.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProblemDetails {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default, alias = "requestId")]
    pub request_id: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

/// Map a status code + optional problem body to a typed error.
pub fn status_to_error(status: u16, problem: Option<ProblemDetails>) -> HermonError {
    let msg = problem
        .as_ref()
        .and_then(|p| p.detail.clone().or_else(|| p.title.clone()))
        .unwrap_or_else(|| format!("HTTP {status}"));
    let request_id = problem.as_ref().and_then(|p| p.request_id.clone());

    match status {
        401 => HermonError::Auth {
            message: msg,
            request_id,
        },
        403 => HermonError::PolicyDenied {
            message: msg,
            code: problem.and_then(|p| p.code),
            request_id,
        },
        429 => HermonError::RateLimited {
            retry_after_ms: None,
            request_id,
        },
        400 => HermonError::Validation {
            message: msg,
            detail: problem.and_then(|p| p.detail),
            request_id,
        },
        s if s >= 500 => HermonError::Server {
            status: s,
            message: msg,
            request_id,
        },
        _ => HermonError::Server {
            status,
            message: msg,
            request_id,
        },
    }
}
