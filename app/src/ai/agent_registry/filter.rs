//! Search filtering for [`RegistryEntry`].
//!
//! This module is intentionally side-effect-free: it has no UI, no
//! context, no I/O — just a pure predicate that decides whether a
//! given entry matches a free-text query. That's a deliberate
//! separation: the picker modal (future), the settings page search
//! box, and the command palette can all reuse the same filter without
//! re-implementing the matching rules.
//!
//! # Matching rules
//!
//! - **Empty query** matches every entry.
//! - **Multi-word queries** are split on whitespace; *every* word
//!   must match somewhere in the entry. This gives users a natural
//!   "AND" semantics — typing `coder rust` finds an agent whose name
//!   is "Rust Coder" without requiring an exact phrase.
//! - **Per-word match** is case-insensitive substring across:
//!   name, slug, agent type label, description, tool IDs,
//!   capabilities, and source label.
//!
//! Substring matching (rather than fuzzy scoring) is deliberate: it's
//! predictable, has no scoring tie-breaks to explain, and "coder"
//! already pinpoints the one agent the user likely wants.

use super::{AgentSource, RegistryEntry};

/// Convert an [`AgentSource`] tag to a label users can search for.
///
/// Exposed as a public helper because the settings page also uses it
/// for the source badge — keeping the strings in one place ensures the
/// label users see in the UI is the same string the search matches.
pub fn source_label(source: AgentSource) -> &'static str {
    match source {
        AgentSource::Hermon => "Hermon",
        AgentSource::BuiltIn => "Built-in",
    }
}

/// Convert an [`hermon_client::types::agent::AgentType`] to its short
/// human-readable label. Same source-of-truth principle as
/// [`source_label`].
pub fn agent_type_label(t: &hermon_client::types::agent::AgentType) -> &'static str {
    use hermon_client::types::agent::AgentType;
    match t {
        AgentType::Chat => "Chat",
        AgentType::Coding => "Coding",
        AgentType::Orchestrator => "Orchestrator",
        AgentType::Worker => "Worker",
        AgentType::Sdlc => "SDLC",
        AgentType::Custom => "Custom",
    }
}

/// Whether the given entry matches the free-text `query`.
///
/// Returns `true` for an empty query, or when every whitespace-
/// separated token in `query` appears (case-insensitively) somewhere
/// in the entry's searchable fields.
///
/// `entry` and `query` are borrowed; this performs no allocations
/// beyond temporary lowercase strings for matching.
pub fn matches_query(entry: &RegistryEntry, query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return true;
    }

    // Lowercase once per field, since multiple tokens may compare
    // against the same field. We collect into a small Vec rather than
    // re-iterating searchable_fields per token — typical entries have
    // ~8-30 searchable strings, so the allocation is cheap and
    // outweighs duplicated lowercase work.
    let haystacks = searchable_fields(entry);

    trimmed.split_whitespace().all(|token| {
        let lower_token = token.to_lowercase();
        haystacks
            .iter()
            .any(|h| h.to_lowercase().contains(&lower_token))
    })
}

/// Collect every string field on `entry` that participates in search.
///
/// Order matches the documented priority in the module-level docs:
/// name, slug, type label, description, tool IDs, capabilities,
/// source label. Order does not affect matching (an OR over all
/// fields), but the function returns them in priority order so
/// future scoring/highlight logic has a stable basis to work from.
fn searchable_fields(entry: &RegistryEntry) -> Vec<String> {
    let mut fields =
        Vec::with_capacity(8 + entry.agent.tools.len() + entry.agent.capabilities.len());
    fields.push(entry.agent.name.clone());
    fields.push(entry.agent.slug.clone());
    fields.push(agent_type_label(&entry.agent.agent_type).to_string());
    if let Some(desc) = &entry.agent.description {
        fields.push(desc.clone());
    }
    for tool in &entry.agent.tools {
        fields.push(tool.tool_id.clone());
    }
    for cap in &entry.agent.capabilities {
        fields.push(cap.clone());
    }
    fields.push(source_label(entry.source).to_string());
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermon_client::types::agent::{
        Agent, AgentModelConfig, AgentToolRef, AgentType, AgentVisibility,
    };

    /// Construct a minimal `Agent` for testing. Uses direct field
    /// access (no builder) so that any additions to the wire-type
    /// shape force this fixture to be revisited deliberately.
    fn agent(name: &str, slug: &str) -> Agent {
        Agent {
            id: format!("test:{slug}"),
            name: name.into(),
            slug: slug.into(),
            description: None,
            model: AgentModelConfig {
                provider_id: "anthropic".into(),
                model_id: "claude-3".into(),
                fallback_model_id: None,
                temperature: None,
                max_output_tokens: None,
            },
            tools: vec![],
            system_prompt: None,
            instructions: None,
            agent_type: AgentType::Chat,
            capabilities: vec![],
            max_turns: None,
            parameters: None,
            metadata: None,
            owner_id: "test:owner".into(),
            org_id: None,
            visibility: AgentVisibility::Private,
            created_at: "1970-01-01T00:00:00Z".into(),
            updated_at: "1970-01-01T00:00:00Z".into(),
        }
    }

    fn entry(name: &str, slug: &str, source: AgentSource) -> RegistryEntry {
        RegistryEntry {
            agent: agent(name, slug),
            source,
        }
    }

    #[test]
    fn empty_query_matches_everything() {
        let e = entry("Wish Coder", "wish-coder", AgentSource::BuiltIn);
        assert!(matches_query(&e, ""));
        assert!(matches_query(&e, "   "));
    }

    #[test]
    fn matches_name_case_insensitive() {
        let e = entry("Wish Coder", "wish-coder", AgentSource::BuiltIn);
        assert!(matches_query(&e, "coder"));
        assert!(matches_query(&e, "CODER"));
        assert!(matches_query(&e, "wIsH"));
    }

    #[test]
    fn matches_slug() {
        let e = entry("Wish Coder", "wish-coder", AgentSource::BuiltIn);
        assert!(matches_query(&e, "wish-coder"));
        assert!(matches_query(&e, "ish-co")); // mid-substring works
    }

    #[test]
    fn matches_type_label() {
        let mut e = entry("X", "x", AgentSource::BuiltIn);
        e.agent.agent_type = AgentType::Sdlc;
        assert!(matches_query(&e, "sdlc"));
        assert!(matches_query(&e, "SDLC"));
    }

    #[test]
    fn matches_description() {
        let mut e = entry("X", "x", AgentSource::BuiltIn);
        e.agent.description = Some("Refactors legacy Python codebases".into());
        assert!(matches_query(&e, "python"));
        assert!(matches_query(&e, "refactor"));
    }

    #[test]
    fn matches_tool_ids() {
        let mut e = entry("X", "x", AgentSource::BuiltIn);
        e.agent.tools = vec![
            AgentToolRef {
                tool_id: "file_read".into(),
                config: None,
                requires_approval: false,
            },
            AgentToolRef {
                tool_id: "git_grep".into(),
                config: None,
                requires_approval: false,
            },
        ];
        assert!(matches_query(&e, "git_grep"));
        assert!(matches_query(&e, "grep"));
        assert!(matches_query(&e, "file_read"));
    }

    #[test]
    fn matches_capabilities() {
        let mut e = entry("X", "x", AgentSource::BuiltIn);
        e.agent.capabilities = vec!["planning".into(), "architecture".into()];
        assert!(matches_query(&e, "planning"));
        assert!(matches_query(&e, "archi"));
    }

    #[test]
    fn matches_source_label() {
        let h = entry("Custom", "custom", AgentSource::Hermon);
        let b = entry("Custom", "custom", AgentSource::BuiltIn);
        assert!(matches_query(&h, "hermon"));
        assert!(!matches_query(&h, "built-in"));
        assert!(matches_query(&b, "built-in"));
        assert!(!matches_query(&b, "hermon"));
    }

    #[test]
    fn multi_word_query_requires_all_tokens_to_match() {
        let mut e = entry("Wish Rust Coder", "wish-rust-coder", AgentSource::BuiltIn);
        e.agent.capabilities = vec!["systems".into()];

        assert!(matches_query(&e, "rust coder"));
        assert!(matches_query(&e, "wish systems")); // different fields
        assert!(matches_query(&e, "Coder Rust")); // order doesn't matter
        assert!(!matches_query(&e, "rust java")); // 'java' not present
    }

    #[test]
    fn no_match_returns_false() {
        let e = entry("Wish Coder", "wish-coder", AgentSource::BuiltIn);
        assert!(!matches_query(&e, "kubernetes"));
        assert!(!matches_query(&e, "python    flask")); // none match
    }

    #[test]
    fn agent_type_label_is_complete() {
        // Sanity-check that no AgentType variant returns an empty label.
        for t in [
            AgentType::Chat,
            AgentType::Coding,
            AgentType::Orchestrator,
            AgentType::Worker,
            AgentType::Sdlc,
            AgentType::Custom,
        ] {
            let label = agent_type_label(&t);
            assert!(!label.is_empty(), "label for {t:?} must be non-empty");
        }
    }
}
