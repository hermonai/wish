//! Agent registry — the single source of truth for the agents that are
//! available to invoke from Wish.
//!
//! See [`model`] for the design notes and architecture overview.
//!
//! # Quick start
//!
//! ```no_run
//! # use wish::ai::agent_registry::AgentRegistryModel;
//! # use wishui::AppContext;
//! # fn example(ctx: &mut AppContext) {
//! // Anywhere in a view or model, get a handle to the registry:
//! let registry = AgentRegistryModel::handle(ctx);
//! let entries = registry.as_ref(ctx).entries();
//!
//! // Look up an agent by slug (e.g., for command palette `@wish-coder`):
//! if let Some(entry) = registry.as_ref(ctx).find_by_slug("wish-coder") {
//!     println!("Found agent: {}", entry.agent.name);
//! }
//! # }
//! ```

pub mod builtin;
pub mod model;
pub mod ui_state;

#[cfg(test)]
mod tests;

pub use model::{
    AgentRegistryEvent, AgentRegistryModel, AgentSource, RegistryEntry, RegistryStatus,
};
pub use ui_state::{BuiltInAgentsUiState, BuiltInAgentsUiStateEvent};
