//! [`AgentRegistryModel`] — the single source of truth for "what agents are
//! available to invoke right now".
//!
//! # Design
//!
//! The registry merges two sources:
//!
//! 1. **Hermon backend** — fetched via [`HermonServiceModel`] when the
//!    backend is reachable. This includes the user's custom agents plus
//!    any system agents the operator has registered server-side.
//! 2. **Built-in SDLC agents** — defined in [`hermon_client::types::sdlc`]
//!    and synthesized locally. These are *always* present, so the UI
//!    works even with no network connectivity.
//!
//! When both sources contain an agent with the same slug, the **Hermon**
//! version wins. This lets operators override defaults centrally without
//! shipping a new client build.
//!
//! # Failure modes
//!
//! Network errors never empty the list. A failed refresh keeps the
//! previously successful merged list, transitioning the status to
//! [`RegistryStatus::Failed`] so the UI can surface a non-blocking
//! warning. Built-ins are always retained as a floor.
//!
//! # Concurrency
//!
//! The model is a singleton: only one task can be modifying it at a time
//! through the `ModelContext` borrow rules. Background refresh tasks
//! complete via the `ctx.spawn(...)` callback, which serializes back onto
//! the model thread.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use hermon_client::types::agent::{
    Agent, AgentFilters, AgentType, AgentVisibility,
};
use hermon_client::HermonClient;
use wishui::{Entity, ModelContext, SingletonEntity};

use crate::server::hermon_service::HermonServiceModel;

use super::builtin;

/// Source tag indicating where a registry entry originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSource {
    /// Fetched from the Hermon backend (user-owned, org-shared, or
    /// server-registered system agent).
    Hermon,
    /// Synthesized locally from a built-in SDLC definition.
    BuiltIn,
}

/// One entry in the registry.
///
/// Holds the agent definition plus a tag identifying its source. Source
/// matters for UI affordances: built-ins generally aren't editable or
/// deletable through the standard CRUD endpoints.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub agent: Agent,
    pub source: AgentSource,
}

impl RegistryEntry {
    /// Convenience accessor for the agent's slug.
    pub fn slug(&self) -> &str {
        &self.agent.slug
    }

    /// Convenience accessor for the agent's ID.
    pub fn id(&self) -> &str {
        &self.agent.id
    }

    /// Whether this entry can be edited or deleted by the user.
    ///
    /// Built-ins are read-only because they're synthesized locally; the
    /// "real" definition lives in the binary. Hermon-sourced agents are
    /// editable subject to backend authorization.
    pub fn is_editable(&self) -> bool {
        matches!(self.source, AgentSource::Hermon)
    }
}

/// State of the most recent refresh operation.
#[derive(Debug, Clone)]
pub enum RegistryStatus {
    /// No refresh has been attempted yet. The registry contains only
    /// built-ins.
    Idle,
    /// A refresh is currently in flight.
    Refreshing,
    /// The most recent refresh completed successfully at the given time.
    Loaded { at: Instant },
    /// The most recent refresh failed. The registry retains its previous
    /// successful contents.
    Failed { error: String, at: Instant },
}

impl RegistryStatus {
    /// Whether the status indicates an in-flight refresh.
    pub fn is_refreshing(&self) -> bool {
        matches!(self, RegistryStatus::Refreshing)
    }
}

/// Events emitted by [`AgentRegistryModel`].
#[derive(Debug, Clone)]
pub enum AgentRegistryEvent {
    /// The agent list changed (refresh succeeded, source priority shifted,
    /// or a manual modification was applied).
    AgentsChanged,
    /// The [`RegistryStatus`] transitioned. Subscribe to this to drive
    /// loading/error indicators in the UI.
    StatusChanged,
}

/// Singleton model holding the current agent registry.
///
/// Construct via [`AgentRegistryModel::new`] in the app initialization,
/// then access from any view via [`AgentRegistryModel::handle`].
pub struct AgentRegistryModel {
    /// Merged, deduplicated list of available agents.
    ///
    /// Order: Hermon agents (insertion order from the API) followed by
    /// built-ins that weren't overridden by a Hermon entry of the same
    /// slug. This means the UI's natural top-down listing surfaces
    /// user/org agents first, with system tooling at the bottom.
    entries: Vec<RegistryEntry>,
    /// `slug -> index into entries` for O(1) slug lookups.
    by_slug: HashMap<String, usize>,
    /// `id -> index into entries` for O(1) ID lookups.
    by_id: HashMap<String, usize>,
    /// State of the most recent refresh.
    status: RegistryStatus,
}

impl Entity for AgentRegistryModel {
    type Event = AgentRegistryEvent;
}

impl SingletonEntity for AgentRegistryModel {}

impl AgentRegistryModel {
    /// Create the registry, seed it with built-in agents, and kick off
    /// the first background refresh from Hermon (if configured).
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        // Seed with built-ins immediately so views never see an empty
        // registry, even before any network call has completed.
        let initial = Self::merge(Vec::new(), builtin::builtin_agents());
        let (by_slug, by_id) = Self::build_indexes(&initial);

        let mut model = Self {
            entries: initial,
            by_slug,
            by_id,
            status: RegistryStatus::Idle,
        };

        // Kick off a background refresh. If the Hermon service isn't
        // configured, the refresh resolves immediately as a no-op.
        model.refresh(ctx);

        model
    }

    /// All currently-known agents (merged from Hermon + built-ins).
    pub fn entries(&self) -> &[RegistryEntry] {
        &self.entries
    }

    /// The state of the most recent refresh. Use this to drive loading
    /// and error indicators in views.
    pub fn status(&self) -> &RegistryStatus {
        &self.status
    }

    /// Find an agent by its slug (e.g., `"wish-coder"`).
    ///
    /// Returns the *effective* entry — if both Hermon and a built-in
    /// have the same slug, the Hermon entry wins.
    pub fn find_by_slug(&self, slug: &str) -> Option<&RegistryEntry> {
        self.by_slug.get(slug).map(|&i| &self.entries[i])
    }

    /// Find an agent by its ID. Synthetic built-in IDs (`"builtin:..."`)
    /// are valid lookups.
    pub fn find_by_id(&self, id: &str) -> Option<&RegistryEntry> {
        self.by_id.get(id).map(|&i| &self.entries[i])
    }

    /// Filter entries by a predicate.
    ///
    /// Returns references rather than clones — callers that need owned
    /// data can `.cloned()` on the iterator.
    pub fn filter<F>(&self, predicate: F) -> Vec<&RegistryEntry>
    where
        F: Fn(&RegistryEntry) -> bool,
    {
        self.entries.iter().filter(|e| predicate(e)).collect()
    }

    /// Convenience: filter by agent type.
    pub fn by_type(&self, ty: &AgentType) -> Vec<&RegistryEntry> {
        self.filter(|e| &e.agent.agent_type == ty)
    }

    /// Convenience: filter by visibility scope.
    pub fn by_visibility(&self, v: &AgentVisibility) -> Vec<&RegistryEntry> {
        self.filter(|e| &e.agent.visibility == v)
    }

    /// Trigger a refresh from the Hermon backend.
    ///
    /// If no refresh is currently in flight and the Hermon service is
    /// configured, this spawns a background task. The status transitions
    /// to [`RegistryStatus::Refreshing`] immediately and resolves to
    /// `Loaded` or `Failed` when the network call completes.
    ///
    /// If the Hermon service is unconfigured, this is a no-op (the
    /// status stays `Idle` and built-ins remain the only entries).
    pub fn refresh(&mut self, ctx: &mut ModelContext<Self>) {
        if self.status.is_refreshing() {
            // Coalesce concurrent refresh requests — second caller is a
            // no-op rather than spawning duplicate work.
            return;
        }

        // Look up the Hermon service. If it's unconfigured, skip the
        // network call entirely and stay in `Idle`.
        let service = HermonServiceModel::handle(ctx);
        let client = match service.as_ref(ctx).client().cloned() {
            Some(c) => c,
            None => {
                log::debug!(
                    "AgentRegistry refresh skipped: Hermon service is not configured"
                );
                return;
            }
        };

        self.set_status(RegistryStatus::Refreshing, ctx);

        // Spawn the network fetch. The future returns a Result that's
        // handled in the callback — *all* state mutation happens in the
        // callback under model context, never inside the future.
        ctx.spawn(
            async move { fetch_hermon_agents(client).await },
            |me, result, ctx| {
                me.handle_refresh_result(result, ctx);
            },
        );
    }

    /// Apply the result of a refresh attempt.
    fn handle_refresh_result(
        &mut self,
        result: Result<Vec<Agent>, String>,
        ctx: &mut ModelContext<Self>,
    ) {
        match result {
            Ok(hermon_agents) => {
                let merged = Self::merge(hermon_agents, builtin::builtin_agents());
                let changed = self.entries_differ(&merged);
                self.entries = merged;
                let (by_slug, by_id) = Self::build_indexes(&self.entries);
                self.by_slug = by_slug;
                self.by_id = by_id;

                self.set_status(
                    RegistryStatus::Loaded { at: Instant::now() },
                    ctx,
                );

                if changed {
                    ctx.emit(AgentRegistryEvent::AgentsChanged);
                }
            }
            Err(error) => {
                log::warn!("AgentRegistry refresh failed: {error}");
                self.set_status(
                    RegistryStatus::Failed {
                        error,
                        at: Instant::now(),
                    },
                    ctx,
                );
                // Note: we do NOT clear `entries`. The previous
                // successful list (or the built-in seed) is retained.
            }
        }
    }

    /// Update status and emit a [`StatusChanged`] event.
    fn set_status(&mut self, new_status: RegistryStatus, ctx: &mut ModelContext<Self>) {
        self.status = new_status;
        ctx.emit(AgentRegistryEvent::StatusChanged);
    }

    /// Compute the merged list of registry entries.
    ///
    /// Pure function — exposed for testing without spinning up an app
    /// context. Hermon agents come first (in their original order),
    /// followed by built-ins whose slug isn't already claimed.
    pub fn merge(hermon_agents: Vec<Agent>, builtin: Vec<Agent>) -> Vec<RegistryEntry> {
        let mut entries: Vec<RegistryEntry> = Vec::with_capacity(
            hermon_agents.len() + builtin.len(),
        );
        let mut seen_slugs: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(hermon_agents.len());

        for a in hermon_agents {
            seen_slugs.insert(a.slug.clone());
            entries.push(RegistryEntry {
                agent: a,
                source: AgentSource::Hermon,
            });
        }

        for a in builtin {
            // Skip built-ins whose slug is already claimed by a Hermon
            // agent — the Hermon version wins. This is what enables
            // central override of built-in defaults.
            if seen_slugs.contains(&a.slug) {
                log::debug!(
                    "Built-in agent {} overridden by Hermon-registered version",
                    a.slug
                );
                continue;
            }
            entries.push(RegistryEntry {
                agent: a,
                source: AgentSource::BuiltIn,
            });
        }

        entries
    }

    /// Build the slug and ID lookup indexes for a list of entries.
    fn build_indexes(
        entries: &[RegistryEntry],
    ) -> (HashMap<String, usize>, HashMap<String, usize>) {
        let mut by_slug = HashMap::with_capacity(entries.len());
        let mut by_id = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            by_slug.insert(e.agent.slug.clone(), i);
            by_id.insert(e.agent.id.clone(), i);
        }
        (by_slug, by_id)
    }

    /// Whether `new_entries` differs from the current `entries`.
    ///
    /// Compares by `(id, source)` tuple in order. This is a cheap
    /// approximation that doesn't catch metadata-only changes; if the
    /// UI needs finer granularity in future, switch to a content hash.
    fn entries_differ(&self, new_entries: &[RegistryEntry]) -> bool {
        if self.entries.len() != new_entries.len() {
            return true;
        }
        for (old, new) in self.entries.iter().zip(new_entries.iter()) {
            if old.agent.id != new.agent.id || old.source != new.source {
                return true;
            }
        }
        false
    }
}

/// Fetch the user's full agent list from Hermon.
///
/// This function intentionally lives outside [`AgentRegistryModel`] so it
/// has no `&self` and no `ctx` — that simplifies the future returned to
/// `ctx.spawn(...)`, which has strict `Send + 'static` requirements.
///
/// The error type is `String` rather than `HermonError` so we can drop
/// the source error after logging it; the registry only needs the
/// human-readable message for status display.
async fn fetch_hermon_agents(client: Arc<HermonClient>) -> Result<Vec<Agent>, String> {
    // Page through the agent list. We use a generous page size to
    // minimize round-trips for the typical case (< 200 agents).
    use hermon_client::types::common::PageParams;

    let mut all = Vec::new();
    let mut offset: u64 = 0;
    let page_size: u64 = 200;

    loop {
        let page = client
            .agents
            .list(
                None::<AgentFilters>,
                Some(PageParams {
                    offset: Some(offset),
                    limit: Some(page_size),
                }),
            )
            .await
            .map_err(|e| format!("{e}"))?;

        let returned = page.items.len() as u64;
        all.extend(page.items);

        if !page.has_more || returned == 0 {
            break;
        }
        offset += returned;

        // Safety belt — never page more than 50 times to avoid
        // accidental unbounded fetches if the server misreports
        // `has_more`.
        if offset > page_size * 50 {
            log::warn!(
                "AgentRegistry refresh aborted pagination at offset {offset}: \
                 server may be misreporting has_more"
            );
            break;
        }
    }

    Ok(all)
}
