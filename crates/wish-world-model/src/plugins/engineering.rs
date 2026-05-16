//! Engineering domain plugin — wraps Wish's code-domain extraction
//! as a [`DomainPlugin`].
//!
//! This plugin demonstrates the **migration path**: an existing
//! domain that lived in core (via the `primitives_from_world` adapter
//! in [`crate::primitives`]) becomes addressable through the plugin
//! interface without losing functionality. Future plugins can live
//! out-of-tree by depending on `wish-world-model` only.

use crate::plugin::DomainPlugin;
use crate::primitives::{primitives_from_world, Primitive};
use crate::semantic_id::Realm;
use crate::WishWorld;

/// The Engineering plugin — code crates, files, functions, and the
/// call graph between them. Wraps [`primitives_from_world`].
pub struct EngineeringPlugin;

impl DomainPlugin for EngineeringPlugin {
    fn realm(&self) -> Realm {
        Realm::Code
    }

    fn name(&self) -> &str {
        "Engineering"
    }

    fn version(&self) -> &str {
        "0.5.0"
    }

    fn description(&self) -> &str {
        "Code as a URE domain — crates, files, functions, call graphs, deps."
    }

    fn perspective_slugs(&self) -> Vec<&str> {
        vec!["engineering", "architecture", "function_graph"]
    }

    fn primitives_for_world(&self, world: &WishWorld) -> Vec<Primitive> {
        // Filter the universal adapter to only code-realm primitives.
        primitives_from_world(world)
            .into_iter()
            .filter(|p| matches!(p.realm(), Realm::Code))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_id::SemanticId;
    use crate::{EntityKind, WorldEntity, WorldKind};

    #[test]
    fn engineering_plugin_owns_code_realm() {
        let plugin = EngineeringPlugin;
        assert_eq!(plugin.realm(), Realm::Code);
        assert_eq!(plugin.name(), "Engineering");
        assert_eq!(plugin.perspective_slugs().len(), 3);
    }

    #[test]
    fn engineering_filters_to_code_realm_only() {
        let mut world = WishWorld::new("mixed", WorldKind::GenericProject);
        world.upsert_entity(WorldEntity::stub(
            SemanticId::code_file("a.rs"),
            "a.rs",
            EntityKind::File,
        ));
        // Non-code entity should be filtered out.
        world.upsert_entity(WorldEntity::stub(
            SemanticId::new(Realm::Finance, "order", "1"),
            "buy order",
            EntityKind::Custom("order".into()),
        ));
        let plugin = EngineeringPlugin;
        let prims = plugin.primitives_for_world(&world);
        // Only the code-realm entity survives the filter.
        assert_eq!(prims.len(), 1);
        assert_eq!(prims[0].realm(), &Realm::Code);
    }
}
