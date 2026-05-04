//! Token storage abstraction.
//!
//! `TokenStore` holds access/refresh/API tokens. `MemoryTokenStore` is
//! the default; production consumers wire in a persistent store (SQLite,
//! OS keychain, etc.).

use std::sync::Mutex;

/// Abstract token storage.
pub trait TokenStore: Send + Sync {
    /// Returns the access token, or `None` if absent or expired.
    fn get_access(&self) -> Option<String>;
    /// Returns the refresh token, or `None` if absent.
    fn get_refresh(&self) -> Option<String>;
    /// Returns the API token, or `None` if absent.
    fn get_api_token(&self) -> Option<String>;
    /// Persist a token pair from a login/refresh response.
    fn set_tokens(&self, access: String, refresh: String, expires_at: i64);
    /// Persist a standalone API token.
    fn set_api_token(&self, token: String);
    /// Clear all stored tokens (logout).
    fn clear(&self);
}

struct MemoryState {
    access: Option<String>,
    refresh: Option<String>,
    api_token: Option<String>,
    expires_at: i64,
}

/// In-memory token store — for tests and ephemeral sessions.
pub struct MemoryTokenStore {
    state: Mutex<MemoryState>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MemoryState {
                access: None,
                refresh: None,
                api_token: None,
                expires_at: 0,
            }),
        }
    }
}

impl Default for MemoryTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenStore for MemoryTokenStore {
    fn get_access(&self) -> Option<String> {
        let s = self.state.lock().unwrap();
        if s.access.is_some() {
            let now_ms = chrono::Utc::now().timestamp_millis();
            if now_ms >= s.expires_at {
                return None; // expired
            }
        }
        s.access.clone()
    }

    fn get_refresh(&self) -> Option<String> {
        self.state.lock().unwrap().refresh.clone()
    }

    fn get_api_token(&self) -> Option<String> {
        self.state.lock().unwrap().api_token.clone()
    }

    fn set_tokens(&self, access: String, refresh: String, expires_at: i64) {
        let mut s = self.state.lock().unwrap();
        s.access = Some(access);
        s.refresh = Some(refresh);
        s.expires_at = expires_at;
    }

    fn set_api_token(&self, token: String) {
        self.state.lock().unwrap().api_token = Some(token);
    }

    fn clear(&self) {
        let mut s = self.state.lock().unwrap();
        s.access = None;
        s.refresh = None;
        s.api_token = None;
        s.expires_at = 0;
    }
}
