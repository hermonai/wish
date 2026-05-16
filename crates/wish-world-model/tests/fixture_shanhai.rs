//! Integration test: read the Shan Hai Fintech Harbor fixture
//! (`demos/shanhai-fintech-harbor.wishworld/`) and assert structural
//! correctness.
//!
//! This is also the v0.5.0 smoke proof that `.wishworld` is a real,
//! portable, version-controlled, AI-readable world format — Wish's
//! structural answer to Antigravity's task-and-artifact model.

use std::path::PathBuf;
use wish_world_model::{read_world_dir, EntityKind, Realm, WorldKind};

fn fixture_path() -> PathBuf {
    // Walk up from this crate to the repo root, then into `demos/`.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/wish-world-model -> crates
    p.pop(); // crates -> repo root
    p.push("demos");
    p.push("shanhai-fintech-harbor.wishworld");
    p
}

#[test]
fn loads_shanhai_fixture_with_entities_and_agents() {
    let bundle = read_world_dir(fixture_path()).expect("read shanhai fixture");
    let w = &bundle.world;

    assert_eq!(w.name, "Shan Hai Fintech Harbor");
    assert!(matches!(w.kind, WorldKind::EducationWorld));
    assert!(!w.intent.is_empty());

    // We expect at least the three seeded entities.
    assert!(
        w.entities.len() >= 3,
        "expected ≥3 entities, found {}",
        w.entities.len()
    );

    let temple = w
        .entities
        .values()
        .find(|e| e.display_name == "Dragon Temple")
        .expect("dragon temple entity present");
    assert!(matches!(temple.kind, EntityKind::SacredArchitecture));
    assert_eq!(temple.id.realm, Realm::Scene);
    assert!(temple.agent_editable);

    // Two NPCs.
    let npc_count = w
        .entities
        .values()
        .filter(|e| matches!(e.kind, EntityKind::Npc))
        .count();
    assert_eq!(npc_count, 2);

    // The world architect agent should be present.
    assert!(
        w.agents
            .values()
            .any(|a| a.display_name == "World Architect"),
        "world architect agent missing"
    );

    // One scene.
    assert!(!w.scenes.is_empty());
}
