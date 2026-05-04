//! UI-only state for the Built-in Agents settings page.
//!
//! This singleton holds purely cosmetic, non-persisted state — currently
//! just "which agent card is expanded right now". It's separate from
//! [`super::AgentRegistryModel`] because:
//!
//! 1. The registry is the source of truth for *what* agents exist; this
//!    model is the source of truth for *how the user is currently
//!    looking at them*.
//! 2. Mixing presentation state into the registry would couple the
//!    registry's event stream to UI mutations that have nothing to do
//!    with agent data, making the registry's API harder to reason about
//!    for non-UI consumers (e.g., the conversation flow, which only
//!    cares about agent data).
//! 3. Treating it as its own model makes it natural for the workspace
//!    action handler to mutate it from anywhere in the app, without
//!    needing a reference to the page view.
//!
//! It's a singleton because there's only one Built-in Agents page in
//! the app, and the singleton pattern makes lookups by type cheap.

use wishui::{Entity, ModelContext, SingletonEntity};

/// Events emitted by [`BuiltInAgentsUiState`].
#[derive(Debug, Clone)]
pub enum BuiltInAgentsUiStateEvent {
    /// The set of expanded agent cards changed. Subscribers should
    /// re-render to reflect the new state.
    ExpansionChanged,
}

/// UI state for the Built-in Agents settings page.
pub struct BuiltInAgentsUiState {
    /// The slug of the currently-expanded agent card, or `None` if no
    /// card is expanded. We use the slug (rather than the ID) because
    /// slugs are stable across registry refreshes — Hermon-issued IDs
    /// could in principle change if a synthesized built-in is replaced
    /// by a Hermon-fetched override of the same slug.
    expanded_slug: Option<String>,
}

impl Entity for BuiltInAgentsUiState {
    type Event = BuiltInAgentsUiStateEvent;
}

impl SingletonEntity for BuiltInAgentsUiState {}

impl BuiltInAgentsUiState {
    /// Construct the UI state with no card expanded.
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            expanded_slug: None,
        }
    }

    /// The currently-expanded slug, if any.
    pub fn expanded_slug(&self) -> Option<&str> {
        self.expanded_slug.as_deref()
    }

    /// Whether the card with the given slug is currently expanded.
    pub fn is_expanded(&self, slug: &str) -> bool {
        self.expanded_slug.as_deref() == Some(slug)
    }

    /// Toggle the expansion state of the given slug.
    ///
    /// - If the slug is currently expanded, collapse it (no card open).
    /// - If a *different* slug is currently expanded, switch to the new
    ///   one (only one card open at a time).
    /// - If nothing is expanded, expand this slug.
    ///
    /// Always emits [`BuiltInAgentsUiStateEvent::ExpansionChanged`].
    pub fn toggle(&mut self, slug: &str, ctx: &mut ModelContext<Self>) {
        if self.expanded_slug.as_deref() == Some(slug) {
            self.expanded_slug = None;
        } else {
            self.expanded_slug = Some(slug.to_string());
        }
        ctx.emit(BuiltInAgentsUiStateEvent::ExpansionChanged);
    }

    /// Collapse any currently-expanded card.
    ///
    /// No-op if nothing is expanded. Useful when the underlying agent
    /// list changes in a way that might invalidate the current
    /// selection (e.g., the expanded agent was removed by a refresh).
    #[allow(dead_code)]
    pub fn collapse(&mut self, ctx: &mut ModelContext<Self>) {
        if self.expanded_slug.is_some() {
            self.expanded_slug = None;
            ctx.emit(BuiltInAgentsUiStateEvent::ExpansionChanged);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A barebones implementation of the trait surface we touch in
    /// tests. We can't construct a real `ModelContext` without an
    /// `AppContext`, so the unit tests below test only the pure
    /// `expanded_slug` / `is_expanded` accessors plus a hand-rolled
    /// state machine that mirrors `toggle`.
    fn new_state() -> BuiltInAgentsUiState {
        BuiltInAgentsUiState {
            expanded_slug: None,
        }
    }

    #[test]
    fn starts_with_nothing_expanded() {
        let s = new_state();
        assert_eq!(s.expanded_slug(), None);
        assert!(!s.is_expanded("anything"));
    }

    #[test]
    fn is_expanded_is_slug_specific() {
        let s = BuiltInAgentsUiState {
            expanded_slug: Some("wish-coder".into()),
        };
        assert!(s.is_expanded("wish-coder"));
        assert!(!s.is_expanded("wish-planner"));
        assert!(!s.is_expanded("wish-coder "));  // trailing space matters
    }

    #[test]
    fn toggle_logic_matches_spec() {
        // Pure-state simulation of the toggle rules. Mirrors the
        // implementation of `toggle` without the `ctx.emit` side
        // effect.
        fn pure_toggle(current: Option<&str>, click: &str) -> Option<String> {
            if current == Some(click) {
                None
            } else {
                Some(click.to_string())
            }
        }

        // None → click "a" → "a"
        assert_eq!(pure_toggle(None, "a").as_deref(), Some("a"));
        // "a" → click "a" → None (collapse)
        assert_eq!(pure_toggle(Some("a"), "a"), None);
        // "a" → click "b" → "b" (switch)
        assert_eq!(pure_toggle(Some("a"), "b").as_deref(), Some("b"));
    }
}
