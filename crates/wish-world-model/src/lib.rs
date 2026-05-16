//! Wish World Model — the semantic source of truth for everything Wish creates.
//!
//! Every visible object (canvas node, scene mesh, code symbol, terminal block,
//! agent, tool-call, file, commit, test, service, asset, NPC, quest) is an
//! entity in the Wish World Model (WWM).
//!
//! This crate has **no UI or rendering dependencies**. It is the gravitational
//! center of the v0.5.0+ Wish architecture. See
//! `wish-design/wish-plan-20260514/03-crates/01-wish-world-model.md` for the
//! full design.

pub mod mission;
pub mod patch;
/// **URE Domain Plugin** — extensibility layer. See `plugin.rs`.
pub mod plugin;
/// **Reference plugins** — Engineering + Chemistry, proving the URE
/// substrate handles disparate domains identically.
pub mod plugins;
/// **Universal Reality Engine primitives** — the Five Universal
/// Primitives + Constraint that any structured reality reduces to.
/// See `wish-design/.../01-strategy/11-universal-reality-engine.md`.
pub mod primitives;
pub mod semantic_id;
pub mod tensorium;
pub mod wishworld_io;
pub mod world;

pub use mission::{
    ApprovalDecision, ApprovalGate, ApprovalRecord, ArtifactKind, ArtifactValidation, BranchId,
    Evidence, MerkleProof, Mission, MissionId, MissionStatus, MissionStep, Signature, SignatureId,
    Signer, VerifiableArtifact, VerifiableArtifactId, DEFAULT_BRANCH,
};
pub use patch::{apply_patch, risk_score, Actor, PatchId, PatchOp, WorldPatch};
pub use plugin::{DomainPlugin, PluginRegistry};
pub use plugins::{ChemistryPlugin, EngineeringPlugin, FinancePlugin};
pub use primitives::{
    Agent as PrimitiveAgent, Constraint, ConstraintSeverity, Event as PrimitiveEvent, Field, Graph,
    GraphEdge, Object, Primitive, PropertyValue,
};
pub use semantic_id::{ParseSemanticIdError, Realm, SemanticId};
pub use tensorium::{TensorAxis, TensorAxisKind, Tensorium};
pub use wishworld_io::{read_world_dir, write_world_dir, WishWorldBundle, WishWorldIoError};
pub use world::{
    AgentRef, Component, EntityKind, EntityStatus, SourceRef, Transform, WishWorld, WorldAgent,
    WorldAsset, WorldEntity, WorldEvent, WorldEventId, WorldId, WorldKind, WorldMemory, WorldRule,
    WorldScene,
};

/// Schema version emitted in `.wishworld/world.json`.
pub const WISHWORLD_SCHEMA: &str = "wishworld/1.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_construct_and_apply_patch() {
        let mut world = WishWorld::new("test-world", WorldKind::GenericProject);
        let entity = WorldEntity::stub(
            SemanticId::code_function("test_module::test_fn"),
            "test_fn",
            EntityKind::Function,
        );
        let patch = WorldPatch::new(
            Actor::Human {
                user_id: "u_test".into(),
            },
            "add a function",
            vec![PatchOp::AddEntity(entity.clone())],
        );
        apply_patch(&mut world, &patch).expect("apply");
        assert_eq!(world.entities.len(), 1);
        assert!(world.entity(&entity.id).is_some());
        assert!(risk_score(&patch) <= 1.0);
        assert!(risk_score(&patch) >= 0.0);
    }

    #[test]
    fn smoke_roundtrip_json() {
        let mut world = WishWorld::new("rt", WorldKind::GenericProject);
        let entity =
            WorldEntity::stub(SemanticId::code_function("a::b"), "b", EntityKind::Function);
        world.upsert_entity(entity);
        let json = serde_json::to_string(&world).expect("ser");
        let parsed: WishWorld = serde_json::from_str(&json).expect("de");
        assert_eq!(parsed.name, "rt");
        assert_eq!(parsed.entities.len(), 1);
    }
}
