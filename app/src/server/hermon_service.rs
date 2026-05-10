//! Hermon service model — singleton providing access to the Hermon backend.
//!
//! Registered at app startup. Views and models access it via
//! `HermonServiceModel::handle(ctx)` to get agent, drive, and conversation APIs.

use std::sync::Arc;

use hermon_client::HermonClient;
use wishui::{Entity, ModelContext, SingletonEntity};

use super::hermon_auth;

/// Events emitted by the Hermon service model.
pub enum HermonServiceEvent {
    /// Connection status changed (connected/disconnected).
    ConnectionStatusChanged,
    /// Agent list was refreshed.
    AgentsRefreshed,
    /// Drive sync completed.
    DriveSynced,
}

/// Connection status to the Hermon backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HermonConnectionStatus {
    /// Not configured (no API key or credentials).
    Unconfigured,
    /// Configured but not yet connected.
    Connecting,
    /// Connected and healthy.
    Connected,
    /// Connection failed or lost.
    Disconnected,
}

/// Singleton model providing access to the Hermon backend services.
///
/// Initialized once at app startup. If the Hermon backend is not configured
/// (no `WISH_API_KEY` and not logged in), the client field is `None`.
pub struct HermonServiceModel {
    client: Option<Arc<HermonClient>>,
    connection_status: HermonConnectionStatus,
}

impl Entity for HermonServiceModel {
    type Event = HermonServiceEvent;
}

impl SingletonEntity for HermonServiceModel {}

impl HermonServiceModel {
    /// Create a new Hermon service model.
    ///
    /// Attempts to create a client from environment/config. If no credentials
    /// are available, the model starts in `Unconfigured` state.
    /// When a client is created, fires an async health check to validate the
    /// connection and transitions to `Connected` or `Disconnected`.
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let client = hermon_auth::create_client().map(Arc::new);

        let connection_status = if client.is_some() {
            HermonConnectionStatus::Connecting
        } else {
            HermonConnectionStatus::Unconfigured
        };

        // Fire an async health check so we discover connection issues early
        // instead of waiting for the first user-initiated request to fail.
        if let Some(ref client) = client {
            let client = Arc::clone(client);
            let _ = ctx.spawn(
                {
                    let client = client;
                    async move {
                        match client.health().await {
                            Ok(_) => Some(true),
                            Err(e) => {
                                log::warn!("Hermon health check failed: {e}");
                                Some(false)
                            }
                        }
                    }
                },
                |me, result, ctx| match result {
                    Some(true) => me.set_connected(ctx),
                    Some(false) | None => me.set_disconnected(ctx),
                },
            );
        }

        Self {
            client,
            connection_status,
        }
    }

    /// Returns `true` if a Hermon backend is configured and available.
    pub fn is_available(&self) -> bool {
        self.client.is_some()
    }

    /// Get the current connection status.
    pub fn connection_status(&self) -> HermonConnectionStatus {
        self.connection_status
    }

    /// Get a reference to the underlying Hermon client, if configured.
    pub fn client(&self) -> Option<&Arc<HermonClient>> {
        self.client.as_ref()
    }

    /// Get the API URL this model is configured for.
    pub fn api_url(&self) -> String {
        hermon_auth::api_url()
    }

    /// Get the dashboard URL.
    pub fn dashboard_url(&self) -> String {
        hermon_auth::dashboard_url()
    }

    /// Whether currently targeting local development servers.
    pub fn is_local_dev(&self) -> bool {
        hermon_auth::is_local_dev()
    }

    /// Update connection status after a health check.
    pub fn set_connected(&mut self, ctx: &mut ModelContext<Self>) {
        if self.connection_status != HermonConnectionStatus::Connected {
            self.connection_status = HermonConnectionStatus::Connected;
            ctx.emit(HermonServiceEvent::ConnectionStatusChanged);
        }
    }

    /// Mark as disconnected (e.g., after a failed request).
    pub fn set_disconnected(&mut self, ctx: &mut ModelContext<Self>) {
        if self.connection_status != HermonConnectionStatus::Disconnected {
            self.connection_status = HermonConnectionStatus::Disconnected;
            ctx.emit(HermonServiceEvent::ConnectionStatusChanged);
        }
    }

    /// Reconfigure the client (e.g., after login or API key change).
    pub fn reconfigure(&mut self, ctx: &mut ModelContext<Self>) {
        self.client = hermon_auth::create_client().map(Arc::new);
        self.connection_status = if self.client.is_some() {
            HermonConnectionStatus::Connecting
        } else {
            HermonConnectionStatus::Unconfigured
        };
        ctx.emit(HermonServiceEvent::ConnectionStatusChanged);
    }
}
