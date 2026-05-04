//! Pure-logic tests for the agent registry.
//!
//! These tests exercise [`AgentRegistryModel::merge`] and the index
//! construction without spinning up a full `AppContext`. The async
//! refresh path is tested separately in integration tests against a
//! mocked Hermon client.

use hermon_client::types::agent::{
    Agent, AgentModelConfig, AgentType, AgentVisibility,
};

use super::model::{AgentRegistryModel, AgentSource, RegistryEntry};

/// Build a minimal `Agent` for fixtures. Only fields the tests inspect
/// are interesting; everything else is filled with defaults.
fn fixture_agent(id: &str, slug: &str, name: &str) -> Agent {
    Agent {
        id: id.into(),
        name: name.into(),
        slug: slug.into(),
        description: None,
        model: AgentModelConfig {
            provider_id: "anthropic".into(),
            model_id: "claude-sonnet-4-6".into(),
            fallback_model_id: None,
            temperature: None,
            max_output_tokens: None,
        },
        tools: Vec::new(),
        system_prompt: None,
        instructions: None,
        agent_type: AgentType::Chat,
        capabilities: Vec::new(),
        max_turns: None,
        parameters: None,
        metadata: None,
        owner_id: "user-1".into(),
        org_id: None,
        visibility: AgentVisibility::Private,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

// ── merge() ────────────────────────────────────────────────────────────

#[test]
fn merge_with_only_builtin() {
    let entries = AgentRegistryModel::merge(Vec::new(), super::builtin::builtin_agents());
    assert!(!entries.is_empty(), "built-ins should populate the registry");
    assert!(
        entries.iter().all(|e| e.source == AgentSource::BuiltIn),
        "all entries should be tagged BuiltIn when there are no Hermon agents"
    );
}

#[test]
fn merge_with_only_hermon() {
    let hermon = vec![
        fixture_agent("a-1", "alpha", "Alpha"),
        fixture_agent("a-2", "beta", "Beta"),
    ];
    let entries = AgentRegistryModel::merge(hermon, Vec::new());
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.source == AgentSource::Hermon));
}

#[test]
fn merge_concatenates_disjoint_sets() {
    let hermon = vec![fixture_agent("a-1", "user-agent", "User Agent")];
    let builtin = super::builtin::builtin_agents();
    let entries = AgentRegistryModel::merge(hermon, builtin.clone());
    assert_eq!(entries.len(), 1 + builtin.len());

    // Hermon comes first.
    assert_eq!(entries[0].agent.slug, "user-agent");
    assert_eq!(entries[0].source, AgentSource::Hermon);

    // Built-ins follow, in order.
    for (i, b) in builtin.iter().enumerate() {
        assert_eq!(entries[1 + i].agent.slug, b.slug);
        assert_eq!(entries[1 + i].source, AgentSource::BuiltIn);
    }
}

#[test]
fn merge_hermon_overrides_builtin_with_same_slug() {
    // Pretend Hermon has a customized "wish-coder".
    let hermon_coder = fixture_agent("hermon-id-1", "wish-coder", "Custom Coder");
    let entries = AgentRegistryModel::merge(
        vec![hermon_coder.clone()],
        super::builtin::builtin_agents(),
    );

    // The slug appears exactly once...
    let coder_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.agent.slug == "wish-coder")
        .collect();
    assert_eq!(coder_entries.len(), 1, "slug must not be duplicated");

    // ...and it's the Hermon version.
    assert_eq!(coder_entries[0].source, AgentSource::Hermon);
    assert_eq!(coder_entries[0].agent.id, "hermon-id-1");
    assert_eq!(coder_entries[0].agent.name, "Custom Coder");
}

#[test]
fn merge_preserves_hermon_order() {
    let hermon = vec![
        fixture_agent("a-3", "gamma", "Gamma"),
        fixture_agent("a-1", "alpha", "Alpha"),
        fixture_agent("a-2", "beta", "Beta"),
    ];
    let entries = AgentRegistryModel::merge(hermon, Vec::new());
    assert_eq!(entries[0].agent.slug, "gamma");
    assert_eq!(entries[1].agent.slug, "alpha");
    assert_eq!(entries[2].agent.slug, "beta");
}

#[test]
fn merge_handles_duplicate_slugs_within_hermon() {
    // The Hermon API shouldn't return duplicates, but if it does, we
    // must not crash. Our merge() preserves the first occurrence's
    // entry and treats the second as part of the seen set.
    let hermon = vec![
        fixture_agent("dup-1", "duplicate", "First"),
        fixture_agent("dup-2", "duplicate", "Second"),
    ];
    let entries = AgentRegistryModel::merge(hermon, Vec::new());
    // Both Hermon entries get inserted (we don't dedup within Hermon
    // — that's the server's job). But our slug index will only point
    // at the first one. Verify we don't panic and the contract is
    // documented: merge does not deduplicate within a single source.
    assert_eq!(entries.len(), 2);
}

// ── Building from merge result ─────────────────────────────────────────

#[test]
fn registry_entry_helpers() {
    let entry = RegistryEntry {
        agent: fixture_agent("id-1", "test-agent", "Test"),
        source: AgentSource::Hermon,
    };
    assert_eq!(entry.id(), "id-1");
    assert_eq!(entry.slug(), "test-agent");
    assert!(entry.is_editable(), "Hermon-sourced agents are editable");

    let builtin_entry = RegistryEntry {
        agent: fixture_agent("builtin:foo", "foo", "Foo"),
        source: AgentSource::BuiltIn,
    };
    assert!(
        !builtin_entry.is_editable(),
        "built-in agents are read-only"
    );
}

// ── Builtin set sanity ─────────────────────────────────────────────────

#[test]
fn all_builtin_slugs_resolve_after_merge() {
    use hermon_client::types::sdlc::slugs;

    let entries = AgentRegistryModel::merge(Vec::new(), super::builtin::builtin_agents());
    let by_slug: std::collections::HashMap<&str, &RegistryEntry> =
        entries.iter().map(|e| (e.agent.slug.as_str(), e)).collect();

    for slug in [
        slugs::PLANNER,
        slugs::CODER,
        slugs::REVIEWER,
        slugs::TESTER,
        slugs::DEBUGGER,
        slugs::DEPLOYER,
        slugs::DOCUMENTER,
        slugs::REFACTORER,
        slugs::SECURITY,
        slugs::ORCHESTRATOR,
    ] {
        assert!(
            by_slug.contains_key(slug),
            "built-in slug '{slug}' should be in the merged registry"
        );
    }
}
