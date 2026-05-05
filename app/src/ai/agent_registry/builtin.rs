//! Conversion of built-in SDLC agent definitions into the runtime [`Agent`] form.
//!
//! [`hermon_client::types::sdlc::builtin_agents`] returns a list of
//! [`CreateAgentRequest`]s — the *creation* shape used to register agents on
//! the Hermon backend. The runtime UI works with [`Agent`] (the *persisted*
//! shape, with stable IDs and timestamps).
//!
//! When the Hermon backend is unreachable, we still want to expose the SDLC
//! agents in the registry so users can read their descriptions, capabilities,
//! and tool lists. This module performs that conversion locally, assigning
//! deterministic synthetic IDs derived from the slug so downstream callers
//! don't have to special-case "no ID yet" cases.

use hermon_client::types::agent::{Agent, AgentVisibility, CreateAgentRequest};
use hermon_client::types::sdlc;

/// Owner ID stamped on locally-synthesized built-in agents.
///
/// This is a sentinel value — it's never a real user. Code that needs to
/// distinguish "synthesized locally" from "fetched from Hermon" should
/// check the [`super::model::AgentSource`] tag, not this string.
pub(crate) const BUILTIN_OWNER_ID: &str = "system:wish";

/// Stable synthetic timestamp used for built-in agents.
///
/// We use the Unix epoch so deterministic outputs are easy to test and so
/// "newest first" sorts naturally place real agents above the built-ins.
const BUILTIN_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

/// Build a deterministic synthetic ID for a built-in agent from its slug.
///
/// The prefix makes built-in IDs visually distinct from Hermon-issued IDs
/// (which are typically ULIDs). It also guarantees no collision: real
/// Hermon IDs would never start with `builtin:`.
fn synthetic_id(slug: &str) -> String {
    format!("builtin:{slug}")
}

/// Convert a [`CreateAgentRequest`] (the SDLC builtin shape) into a runtime
/// [`Agent`].
///
/// Fields not present in the create request are filled in with sensible
/// defaults — see the inline comments for each field.
pub(crate) fn create_request_to_agent(req: CreateAgentRequest) -> Agent {
    Agent {
        // Synthetic ID derived from slug — stable across runs.
        id: synthetic_id(&req.slug),
        name: req.name,
        slug: req.slug,
        description: req.description,
        model: req.model,
        // `tools` defaults to empty rather than `None` to match the
        // `Vec<AgentToolRef>` shape on `Agent`.
        tools: req.tools.unwrap_or_default(),
        system_prompt: req.system_prompt,
        instructions: req.instructions,
        agent_type: req.agent_type,
        // `capabilities` similarly defaults to empty.
        capabilities: req.capabilities.unwrap_or_default(),
        max_turns: req.max_turns,
        parameters: req.parameters,
        metadata: req.metadata,
        // Sentinel owner — see [`BUILTIN_OWNER_ID`].
        owner_id: BUILTIN_OWNER_ID.into(),
        // Built-ins are not scoped to any org; they are universal.
        org_id: None,
        // Default to System visibility unless the request explicitly
        // requested something more permissive.
        visibility: req.visibility.unwrap_or(AgentVisibility::System),
        created_at: BUILTIN_TIMESTAMP.into(),
        updated_at: BUILTIN_TIMESTAMP.into(),
    }
}

/// Return the full set of built-in SDLC agents in runtime [`Agent`] form.
///
/// This is a pure function — it performs no I/O and is safe to call from
/// any thread. The returned vector's order matches
/// [`sdlc::builtin_agents`], which is the canonical "presentation order"
/// for SDLC tooling.
pub fn builtin_agents() -> Vec<Agent> {
    sdlc::builtin_agents()
        .into_iter()
        .map(create_request_to_agent)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_id_is_prefixed() {
        assert_eq!(synthetic_id("wish-coder"), "builtin:wish-coder");
    }

    #[test]
    fn all_sdlc_agents_convert_successfully() {
        let agents = builtin_agents();
        assert_eq!(
            agents.len(),
            sdlc::builtin_agents().len(),
            "every CreateAgentRequest should convert to an Agent"
        );
    }

    #[test]
    fn converted_agents_have_unique_synthetic_ids() {
        let agents = builtin_agents();
        let mut seen = std::collections::HashSet::new();
        for a in &agents {
            assert!(seen.insert(a.id.clone()), "duplicate id: {}", a.id);
            assert!(
                a.id.starts_with("builtin:"),
                "id {} should be synthetic",
                a.id
            );
        }
    }

    #[test]
    fn slugs_match_canonical_constants() {
        // Sanity check that the conversion preserves slugs — slugs are the
        // public API for user-facing references like @wish-coder.
        let agents = builtin_agents();
        let slugs: std::collections::HashSet<_> = agents.iter().map(|a| a.slug.as_str()).collect();
        assert!(slugs.contains(sdlc::slugs::PLANNER));
        assert!(slugs.contains(sdlc::slugs::CODER));
        assert!(slugs.contains(sdlc::slugs::REVIEWER));
        assert!(slugs.contains(sdlc::slugs::ORCHESTRATOR));
    }

    #[test]
    fn owner_id_is_sentinel() {
        for a in builtin_agents() {
            assert_eq!(a.owner_id, BUILTIN_OWNER_ID);
        }
    }

    #[test]
    fn visibility_defaults_to_system() {
        for a in builtin_agents() {
            assert_eq!(a.visibility, AgentVisibility::System);
        }
    }

    #[test]
    fn timestamps_are_deterministic() {
        // Calling twice must produce identical timestamps (and IDs) so the
        // registry can compare snapshots cheaply.
        let a = builtin_agents();
        let b = builtin_agents();
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.created_at, y.created_at);
            assert_eq!(x.updated_at, y.updated_at);
        }
    }
}
