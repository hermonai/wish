//! **Domain Plugin System** — extensibility layer of the Universal
//! Reality Engine. Strategic frame:
//! [`01-strategy/11-universal-reality-engine.md`](../../../wish-design/wish-plan-20260514/01-strategy/11-universal-reality-engine.md).
//!
//! A `DomainPlugin` declares:
//!   - which [`Realm`] it owns (`code`, `chemistry`, `finance`, …)
//!   - how to extract URE [`Primitive`]s from a domain-specific input
//!   - what [`Constraint`]s govern its realm
//!   - which `Perspective`s the renderer should offer for it
//!   - how to advance simulation state for it (optional)
//!
//! **The plugin is the lever that makes adding a new domain a
//! registration, not a core change.** Wish v0.5.0 ships two reference
//! plugins (`EngineeringPlugin`, `ChemistryPlugin`); domains like
//! Legal, Medical, Music, Civic, Climate land as v0.7.0 plugins
//! without modifying this crate.

use crate::primitives::{Constraint, Primitive};
use crate::semantic_id::Realm;
use crate::WishWorld;
use std::collections::HashMap;

/// A domain plugin — extends the URE with a new realm of structured
/// reality (chemistry, finance, biology, …) without touching core.
///
/// All methods have sensible defaults so a minimal plugin only needs
/// to implement [`realm`](Self::realm) + [`name`](Self::name).
pub trait DomainPlugin: Send + Sync {
    /// The [`Realm`] this plugin owns. Every `Primitive` it produces
    /// has this realm on its `SemanticId`.
    fn realm(&self) -> Realm;

    /// Human-readable plugin name. Shown in plugin lists, telemetry.
    fn name(&self) -> &str;

    /// Plugin version (semver string).
    fn version(&self) -> &str {
        "0.1.0"
    }

    /// One-line description of what this plugin models.
    fn description(&self) -> &str {
        ""
    }

    /// Extract URE primitives from a [`WishWorld`] for this realm.
    /// Default: empty (the plugin doesn't consume worlds directly).
    fn primitives_for_world(&self, _world: &WishWorld) -> Vec<Primitive> {
        Vec::new()
    }

    /// Static constraints that always apply in this realm (physical
    /// laws, accounting identities, valence rules, etc.). Default:
    /// none. Per-instance constraints come from the primitives.
    fn realm_constraints(&self) -> Vec<Constraint> {
        Vec::new()
    }

    /// Perspective slugs this plugin contributes to the renderer
    /// dropdown. Default: none. Plugins typically declare 1–3 slugs.
    fn perspective_slugs(&self) -> Vec<&str> {
        Vec::new()
    }

    /// Advance the plugin's simulation state by one tick. Default:
    /// no-op. Domains like physics, chemistry, finance override this
    /// to produce events from current state. Returns the events
    /// created this tick.
    fn simulate_tick(&self, _world: &mut WishWorld) -> Vec<Primitive> {
        Vec::new()
    }
}

/// A registry of [`DomainPlugin`]s, indexed by realm. Plugins are
/// `Arc`'d so they can be cheaply cloned into agent threads.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: HashMap<Realm, std::sync::Arc<dyn DomainPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin. Overwrites any existing plugin for the same
    /// realm — last-registration-wins is the right policy for hot
    /// reload + dev iteration.
    pub fn register(&mut self, plugin: std::sync::Arc<dyn DomainPlugin>) {
        let realm = plugin.realm();
        self.plugins.insert(realm, plugin);
    }

    /// Look up a plugin by realm.
    pub fn get(&self, realm: &Realm) -> Option<&std::sync::Arc<dyn DomainPlugin>> {
        self.plugins.get(realm)
    }

    /// Iterate every registered plugin.
    pub fn iter(&self) -> impl Iterator<Item = &std::sync::Arc<dyn DomainPlugin>> {
        self.plugins.values()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Collect all perspective slugs declared by every registered
    /// plugin, in registration order. The renderer's perspective
    /// dropdown calls this once at startup + on hot-reload.
    pub fn all_perspective_slugs(&self) -> Vec<String> {
        let mut out = Vec::new();
        for p in self.plugins.values() {
            for slug in p.perspective_slugs() {
                out.push(slug.to_string());
            }
        }
        out
    }

    /// Collect every realm-level constraint from every plugin. The
    /// URE's safety layer consults this before applying any patch
    /// from an agent.
    pub fn all_realm_constraints(&self) -> Vec<Constraint> {
        self.plugins
            .values()
            .flat_map(|p| p.realm_constraints())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{ConstraintSeverity, Object};
    use crate::semantic_id::SemanticId;
    use std::sync::Arc;

    struct MinimalPlugin;
    impl DomainPlugin for MinimalPlugin {
        fn realm(&self) -> Realm {
            Realm::Custom("minimal".to_string())
        }
        fn name(&self) -> &str {
            "Minimal Reference Plugin"
        }
    }

    #[test]
    fn registry_register_get_realm() {
        let mut reg = PluginRegistry::new();
        reg.register(Arc::new(MinimalPlugin));
        assert_eq!(reg.len(), 1);
        let realm = Realm::Custom("minimal".to_string());
        let p = reg.get(&realm).unwrap();
        assert_eq!(p.name(), "Minimal Reference Plugin");
        assert_eq!(p.version(), "0.1.0");
    }

    #[test]
    fn registry_last_registration_wins() {
        struct V1;
        impl DomainPlugin for V1 {
            fn realm(&self) -> Realm {
                Realm::Code
            }
            fn name(&self) -> &str {
                "v1"
            }
        }
        struct V2;
        impl DomainPlugin for V2 {
            fn realm(&self) -> Realm {
                Realm::Code
            }
            fn name(&self) -> &str {
                "v2"
            }
        }
        let mut reg = PluginRegistry::new();
        reg.register(Arc::new(V1));
        reg.register(Arc::new(V2));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get(&Realm::Code).unwrap().name(), "v2");
    }

    #[test]
    fn plugin_can_declare_constraints_and_perspectives() {
        struct PhysicsPlugin;
        impl DomainPlugin for PhysicsPlugin {
            fn realm(&self) -> Realm {
                Realm::Custom("physics".to_string())
            }
            fn name(&self) -> &str {
                "Physics"
            }
            fn perspective_slugs(&self) -> Vec<&str> {
                vec!["physics", "mechanics"]
            }
            fn realm_constraints(&self) -> Vec<Constraint> {
                vec![Constraint::new(
                    SemanticId::new(Realm::Custom("physics".into()), "law", "energy_conservation"),
                    "physical_law",
                    ConstraintSeverity::Hard,
                    "energy is conserved in closed systems",
                )]
            }
        }
        let mut reg = PluginRegistry::new();
        reg.register(Arc::new(PhysicsPlugin));
        assert_eq!(reg.all_perspective_slugs(), vec!["physics", "mechanics"]);
        let cs = reg.all_realm_constraints();
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].severity, ConstraintSeverity::Hard);
    }

    #[test]
    fn plugin_extracts_primitives_from_world() {
        struct CountingPlugin;
        impl DomainPlugin for CountingPlugin {
            fn realm(&self) -> Realm {
                Realm::Code
            }
            fn name(&self) -> &str {
                "Counting"
            }
            fn primitives_for_world(&self, world: &WishWorld) -> Vec<Primitive> {
                // Emit one Object per entity.
                world
                    .entities
                    .values()
                    .map(|e| {
                        Primitive::Object(Object::new(
                            e.id.clone(),
                            "counted",
                            e.display_name.clone(),
                        ))
                    })
                    .collect()
            }
        }
        use crate::{EntityKind, WorldEntity, WorldKind};
        let mut world = WishWorld::new("test", WorldKind::GenericProject);
        world.upsert_entity(WorldEntity::stub(
            SemanticId::code_file("a.rs"),
            "a.rs",
            EntityKind::File,
        ));
        let plugin = CountingPlugin;
        assert_eq!(plugin.primitives_for_world(&world).len(), 1);
    }
}
