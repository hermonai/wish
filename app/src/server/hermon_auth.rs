//! Hermon auth adapter for the Wish terminal.
//!
//! Provides Hermon backend authentication as an alternative to the
//! Firebase/Warp server auth flow. When a Hermon API token is configured
//! (via `WISH_API_KEY` or `wish login --hermon`), it is used for
//! AI model routing and session management through the Hermon control plane.
//!
//! This module bridges `hermon_client` into the wish app's auth system
//! without replacing the existing Firebase flow — both can coexist.

use std::sync::Arc;

use hermon_client::config::HermonClientConfig;
use hermon_client::store::MemoryTokenStore;
use hermon_client::{HermonClient, TokenStore};
use wish_core::channel::ChannelState;

/// The production Hermon API base URL.
const PRODUCTION_HERMON_URL: &str = "https://api.hermon.ai";

/// The production Hermon dashboard URL (manage API keys, account, preferences).
/// Hosted at the `wish` sub-path of the Hermon marketing site.
const PRODUCTION_HERMON_DASHBOARD_URL: &str = "https://www.hermon.ai/wish";

/// The local development Hermon gateway URL.
const DEV_GATEWAY_URL: &str = "http://localhost:8080";

/// The local development Hermon dashboard URL.
const DEV_DASHBOARD_URL: &str = "http://localhost:3000";

/// Create a Hermon client configured from environment or config.
///
/// Checks (in order):
/// 1. `HERMON_API_URL` env var for base URL (explicit developer override)
/// 2. Channel-based default: Stable/Preview/Oss → wish.hermon.ai, Local/Dev → localhost
/// 3. `WISH_API_KEY` env var for API token auth
pub fn create_client() -> Option<HermonClient> {
    let base_url = api_url();

    let api_key = std::env::var("WISH_API_KEY").ok();

    let config = HermonClientConfig {
        base_url,
        user_agent: format!("wish-terminal/{}", env!("CARGO_PKG_VERSION")),
        ..Default::default()
    };

    let store = Arc::new(MemoryTokenStore::new());

    // If we have an API key, store it so the client uses it for auth
    if let Some(key) = &api_key {
        store.set_api_token(key.clone());
    }

    Some(HermonClient::new(config, store))
}

/// Get the configured Hermon API (gateway) URL.
///
/// Resolution order:
/// 1. `HERMON_API_URL` or `WISH_API_URL` env var — explicit developer override, highest priority
/// 2. Channel default:
///    - **Stable / Preview / Oss** → `https://wish.hermon.ai`  (production)
///    - **Local / Dev / Integration** → `http://localhost:8080` (local gateway)
pub fn api_url() -> String {
    if let Ok(url) = std::env::var("HERMON_API_URL") {
        return url;
    }
    if let Ok(url) = std::env::var("WISH_API_URL") {
        return url;
    }

    match ChannelState::channel() {
        wish_core::channel::Channel::Stable
        | wish_core::channel::Channel::Preview
        | wish_core::channel::Channel::Oss => PRODUCTION_HERMON_URL.to_string(),
        _ => DEV_GATEWAY_URL.to_string(),
    }
}

/// Get the configured Hermon dashboard URL.
///
/// Resolution order:
/// 1. `HERMON_DASHBOARD_URL` env var — explicit developer override
/// 2. Channel default:
///    - **Stable / Preview / Oss / Local** → `https://www.hermon.ai/wish`
///    - **Dev / Integration** → `http://localhost:3000`
pub fn dashboard_url() -> String {
    if let Ok(url) = std::env::var("HERMON_DASHBOARD_URL") {
        return url;
    }

    match ChannelState::channel() {
        wish_core::channel::Channel::Stable
        | wish_core::channel::Channel::Preview
        | wish_core::channel::Channel::Oss
        | wish_core::channel::Channel::Local => PRODUCTION_HERMON_DASHBOARD_URL.to_string(),
        _ => DEV_DASHBOARD_URL.to_string(),
    }
}

/// Returns true if the current configuration points to local development servers.
pub fn is_local_dev() -> bool {
    let url = api_url();
    url.contains("localhost") || url.contains("127.0.0.1")
}
